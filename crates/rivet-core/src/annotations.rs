//! Annotations: highlights, underlines, strikeouts, freehand ink, shapes,
//! lines and arrows, and sticky notes. They are stored in the PDF the
//! standard way, so every PDF reader shows them.
//!
//! How each kind is written:
//! - Text markup (Highlight, Underline, StrikeOut, Squiggly): one quadrilateral
//!   per line of text (`QuadPoints`).
//! - Freehand: an Ink annotation, one stroke per path (`InkList`).
//! - Rectangle and ellipse: Square and Circle annotations.
//! - Lines and arrows: Ink annotations too (the line plus, for an arrow, the
//!   two strokes of its head), marked with a private key so PDFRivet still
//!   knows them as lines. PDFium can't write a Line annotation's end points,
//!   and ink looks the same in every reader.
//! - Sticky notes: Text annotations.
//! - Text boxes: FreeText annotations. We lay the text out ourselves
//!   (`textlayout.rs`: Arabic joining, right-to-left order, line breaks) and
//!   draw it as outlines, and also store it the way Acrobat does (`/DA`, `/DS`,
//!   `/RC`), so Acrobat can edit it too. PDFium can't write the `/IT` name that
//!   marks a box growing with its text; the save pass adds it (`prune.rs`).
//! - Replies to an annotation's comment: Text annotations with an empty
//!   appearance (so nothing is drawn), pointing to the annotation they answer.
//!   PDFium can't write that reference (`/IRT`), so they carry the parent's
//!   name in a private key until the file is saved (see `prune.rs`).
//! - Signatures: drawn ones are Ink; pictures are Stamp annotations holding
//!   the image, marked with the private key so they can be moved and resized.
//!
//! PDFium draws the appearance of all of these itself (`CPDF_GenerateAP`) the
//! first time the page is rendered, and stores it in the file. When an
//! annotation changes, its appearance is removed so PDFium draws it again.
//!
//! pdfium-render doesn't wrap most of the `FPDFAnnot_*` functions, so this
//! module calls PDFium directly (`unsafe`) through [`Annot`], following the
//! rules of `forms.rs`: handles come from live pdfium-render objects that
//! outlive the calls, every call runs on the Engine's single PDFium thread,
//! and every annotation handle is closed exactly once.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicU32, Ordering};

use crate::document::RawBindings;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    Error, ErrorCode, Result,
    fonts::TextFont,
    geometry::{Affine, PageGeometry},
    textlayout::{self, TEXT_PADDING, TextAlign, TextDirection, TextStyle, VerticalAlign},
    textstrings::Texts,
};

/// A point as fractions (0..1) of the page, top-left origin, before view rotation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PagePoint {
    pub x: f32,
    pub y: f32,
}

/// A rectangle as fractions (0..1) of the page, top-left origin, before view rotation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PageRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MarkupStyle {
    Highlight,
    Underline,
    Strikeout,
    Squiggly,
}

/// What kind of annotation it is, with its shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum AnnotationKind {
    /// Marks text: one rectangle per line.
    Markup {
        style: MarkupStyle,
        quads: Vec<PageRect>,
    },
    /// Freehand drawing.
    Ink { strokes: Vec<Vec<PagePoint>> },
    /// A rectangle, optionally filled; the shape is the annotation's `rect`.
    Square { fill: Option<Color> },
    /// An ellipse in the annotation's `rect`, optionally filled.
    Circle { fill: Option<Color> },
    Line {
        from: PagePoint,
        to: PagePoint,
        arrow: bool,
    },
    /// A sticky note; its text is the annotation's `contents`.
    Note,
    /// A stamp (e.g. a signature image).
    Stamp,
    /// A text box: text written on the page (its colour is the annotation's).
    FreeText { text: String, style: TextStyle },
    /// Anything else (made by other apps): shown, and can be deleted.
    Other { subtype: String },
}

/// One annotation on a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct Annotation {
    /// Identifies the annotation on its page (its `/NM` name). Empty when adding one.
    #[serde(default)]
    pub id: String,
    pub kind: AnnotationKind,
    /// Where it is on the page. For squares, circles, notes and stamps this is
    /// their shape; for the others it's worked out from the shape.
    pub rect: PageRect,
    pub color: Color,
    /// 0 (invisible) to 1 (opaque).
    pub opacity: f32,
    /// Line width in points.
    pub width: f32,
    /// The comment text.
    #[serde(default)]
    pub contents: String,
    #[serde(default)]
    pub author: String,
    /// When it was last changed (ISO 8601), if known.
    #[serde(default)]
    pub modified: Option<String>,
    /// PDFRivet can change it (otherwise it can only be deleted).
    #[serde(default)]
    pub editable: bool,
    /// A reply: the id of the annotation it answers. Replies are notes that
    /// aren't drawn; readers show them under the comment they answer.
    #[serde(default)]
    pub reply_to: Option<String>,
}

/// The annotations of one page (pages without any are left out of a batch).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct PageAnnotations {
    pub page: u32,
    pub annotations: Vec<Annotation>,
}

/// Annotations of several pages, read a batch at a time (see [`crate::Document::annotations_from`]).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct AnnotationBatch {
    pub pages: Vec<PageAnnotations>,
    /// Where to continue, or `None` after the last page.
    pub next_page: Option<u32>,
}

// Values from PDFium's public headers (fpdf_annot.h), which pdfium-render
// doesn't re-export.
const FPDF_ANNOT_TEXT: u32 = 1;
const FPDF_ANNOT_FREETEXT: u32 = 3;
const FPDF_ANNOT_LINK: u32 = 2;
const FPDF_ANNOT_SQUARE: u32 = 5;
const FPDF_ANNOT_CIRCLE: u32 = 6;
const FPDF_ANNOT_HIGHLIGHT: u32 = 9;
const FPDF_ANNOT_UNDERLINE: u32 = 10;
const FPDF_ANNOT_SQUIGGLY: u32 = 11;
const FPDF_ANNOT_STRIKEOUT: u32 = 12;
const FPDF_ANNOT_STAMP: u32 = 13;
const FPDF_ANNOT_INK: u32 = 15;
const FPDF_ANNOT_POPUP: u32 = 16;
const FPDF_ANNOT_WIDGET: u32 = 20;
const FPDF_ANNOT_FLAG_HIDDEN: i32 = 2;
const FPDF_ANNOT_FLAG_PRINT: i32 = 4;
const FPDF_ANNOT_FLAG_LOCKED: i32 = 128;
const FPDF_ANNOT_APPEARANCEMODE_NORMAL: i32 = 0;
const COLOR_STROKE: FPDFANNOT_COLORTYPE = 0;
const COLOR_FILL: FPDFANNOT_COLORTYPE = 1;
const FPDF_OBJECT_ARRAY: FPDF_OBJECT_TYPE = 5;
const FPDF_PAGEOBJ_PATH: i32 = 2;

/// Private key marking an annotation deleted in PDFRivet but not yet saved:
/// it stays in the document (hidden) so undo can bring it back exactly as it
/// was, and is removed when the file is saved (see [`purge_deleted`]).
pub(crate) const DELETED_KEY: &str = "PDFRivetDeleted";

/// Private key marking an Ink annotation that PDFRivet drew as a line or arrow.
const SHAPE_KEY: &str = "PDFRivetShape";

/// Private key holding a text box's settings (style and colour, as JSON), so
/// PDFRivet reads back exactly what it wrote.
const TEXT_KEY: &str = "PDFRivetText";

/// Private key: "1" if a text box grows with its text (the save pass writes
/// `/IT /FreeTextTypeWriter`), "" if it doesn't (the save pass removes `/IT`).
pub(crate) const TYPEWRITER_KEY: &str = "PDFRivetTypewriter";

/// Private key holding the name of the annotation a reply answers, until the
/// file is saved and it becomes a real `/IRT` reference (see `prune.rs`).
pub(crate) const REPLY_KEY: &str = "PDFRivetReplyTo";

/// Reads the annotations of a page, in the order they are drawn. Links, form
/// fields and pop-up windows are left out (they're handled elsewhere).
pub(crate) fn read(pdfium: &Pdfium, page: &PdfPage, texts: &Texts) -> Vec<Annotation> {
    let Some(map) = Mapping::new(page) else {
        return Vec::new();
    };
    let annots = PageAnnots::new(pdfium, page);
    (0..annots.count())
        .filter_map(|index| {
            let annot = annots.get(index)?;
            if annot.is_deleted() {
                return None;
            }
            let mut model = annot_to_model(&annot, index, &map, texts)?;
            model.reply_to = annots.reply_target(&annot);
            Some(model)
        })
        .collect()
}

/// Adds an annotation; returns its id (the given one if it's free, else a new one).
pub(crate) fn add(pdfium: &Pdfium, page: &PdfPage, annotation: &Annotation) -> Result<String> {
    let map = Mapping::new(page).ok_or_else(|| internal("page has no size"))?;
    let annots = PageAnnots::new(pdfium, page);
    let subtype = match &annotation.kind {
        AnnotationKind::Markup { style, .. } => match style {
            MarkupStyle::Highlight => FPDF_ANNOT_HIGHLIGHT,
            MarkupStyle::Underline => FPDF_ANNOT_UNDERLINE,
            MarkupStyle::Strikeout => FPDF_ANNOT_STRIKEOUT,
            MarkupStyle::Squiggly => FPDF_ANNOT_SQUIGGLY,
        },
        AnnotationKind::Ink { .. } | AnnotationKind::Line { .. } => FPDF_ANNOT_INK,
        AnnotationKind::Square { .. } => FPDF_ANNOT_SQUARE,
        AnnotationKind::Circle { .. } => FPDF_ANNOT_CIRCLE,
        AnnotationKind::Note => FPDF_ANNOT_TEXT,
        AnnotationKind::FreeText { .. } => FPDF_ANNOT_FREETEXT,
        AnnotationKind::Stamp | AnnotationKind::Other { .. } => {
            return Err(internal("this kind of annotation can't be added"));
        }
    };
    // A reply answers a named annotation that is still there.
    if let Some(parent) = &annotation.reply_to {
        let found = annots
            .find(parent)
            .is_some_and(|(_, a)| !a.is_deleted() && !parent.starts_with('#'));
        if !found || annotation.kind != AnnotationKind::Note {
            return Err(Error::new(ErrorCode::AnnotationNotFound, parent.clone()));
        }
    }
    let annot = annots
        .create(subtype as i32)
        .ok_or_else(|| internal("PDFium couldn't create the annotation"))?;
    let id = choose_id(&annots, &annotation.id);
    let (now, _) = crate::metadata::now();
    annot.set_string("NM", &id);
    annot.set_string("CreationDate", &now);
    annot.set_flags(FPDF_ANNOT_FLAG_PRINT);
    write_shape(&annot, annotation, &map)?;
    write_style(&annot, annotation, &now);
    if let Some(parent) = &annotation.reply_to {
        annot.set_string(REPLY_KEY, parent);
    }
    write_appearance(&annot, annotation, &map);
    Ok(id)
}

/// A picture placed on a page as a stamp (a signature): RGBA pixels, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct StampImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Places a picture (a signature) in `annotation.rect`, as a Stamp annotation;
/// returns its id. The picture keeps its transparency.
pub(crate) fn add_image_stamp(
    pdfium: &Pdfium,
    document: &PdfDocument,
    page: &mut PdfPage,
    annotation: &Annotation,
    image: &StampImage,
) -> Result<String> {
    let (matrix, bounds) = {
        let map = Mapping::new(page).ok_or_else(|| internal("page has no size"))?;
        (
            image_matrix(&map, &annotation.rect),
            map.rect_points(&annotation.rect),
        )
    };
    let picture = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
        .ok_or_else(|| internal("picture size doesn't match its pixels"))?;
    let picture = image::DynamicImage::ImageRgba8(picture);

    // Annotations aren't part of the page's content: don't rewrite it.
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    {
        let mut stamp = page.annotations_mut().create_stamp_annotation()?;
        // The appearance is sized to the annotation's rectangle when the picture is added.
        stamp.set_bounds(PdfRect::new_from_values(
            bounds.bottom,
            bounds.left,
            bounds.top,
            bounds.right,
        ))?;
        let mut object = PdfPageImageObject::new(document, &picture)?;
        // A new image is 1 × 1 point at the origin: stretch it over the rectangle.
        object.apply_matrix(PdfMatrix::new(
            matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
        ))?;
        stamp.objects_mut().add_image_object(object)?;
    }

    let annots = PageAnnots::new(pdfium, page);
    let annot = annots
        .get(annots.count() - 1)
        .ok_or_else(|| internal("the new stamp went missing"))?;
    let id = choose_id(&annots, &annotation.id);
    let (now, _) = crate::metadata::now();
    annot.set_string("NM", &id);
    annot.set_string("CreationDate", &now);
    annot.set_string(SHAPE_KEY, "signature");
    annot.set_flags(FPDF_ANNOT_FLAG_PRINT);
    annot.set_string("Contents", &annotation.contents);
    annot.set_string("T", &annotation.author);
    annot.set_string("M", &now);
    Ok(id)
}

/// The matrix that stretches a 1 × 1 image over `rect` (page fractions),
/// upright as the page is shown (pages may have their own rotation).
pub(crate) fn image_matrix(map: &Mapping, rect: &PageRect) -> FS_MATRIX {
    let (x0, y0) = map.points(PagePoint {
        x: rect.left,
        y: rect.bottom,
    });
    let (x1, y1) = map.points(PagePoint {
        x: rect.right,
        y: rect.bottom,
    });
    let (x2, y2) = map.points(PagePoint {
        x: rect.left,
        y: rect.top,
    });
    FS_MATRIX {
        a: x1 - x0,
        b: y1 - y0,
        c: x2 - x0,
        d: y2 - y0,
        e: x0,
        f: y0,
    }
}

/// Changes an annotation (shape, colour, opacity, width, text); returns its id
/// (an annotation from another app gets one the first time it's changed).
pub(crate) fn update(
    pdfium: &Pdfium,
    page: &PdfPage,
    annotation: &Annotation,
    texts: &Texts,
) -> Result<String> {
    let map = Mapping::new(page).ok_or_else(|| internal("page has no size"))?;
    let annots = PageAnnots::new(pdfium, page);
    let (index, annot) = annots
        .find(&annotation.id)
        .ok_or_else(|| Error::new(ErrorCode::AnnotationNotFound, annotation.id.clone()))?;
    let current = annot_to_model(&annot, index, &map, texts)
        .ok_or_else(|| Error::new(ErrorCode::AnnotationNotFound, annotation.id.clone()))?;
    if !current.editable
        || std::mem::discriminant(&current.kind) != std::mem::discriminant(&annotation.kind)
    {
        return Err(Error::new(
            ErrorCode::ReadOnlyAnnotation,
            annotation.id.clone(),
        ));
    }
    let id = match annot.string("NM").filter(|s| !s.is_empty()) {
        Some(id) => id,
        None => {
            let id = new_id();
            annot.set_string("NM", &id);
            id
        }
    };
    let (now, _) = crate::metadata::now();
    if annotation.kind == AnnotationKind::Stamp {
        // A signature picture. Readers fit an appearance's box into the
        // annotation's Rect, so a new Rect moves and scales the picture.
        annot.set_rect(&map.rect_points(&annotation.rect));
        annot.set_string("Contents", &annotation.contents);
        annot.set_string("M", &now);
        return Ok(id);
    }
    if same_style(&current, annotation) && fits_by_stretching(&current, annotation) {
        // Only moved or resized: keep the appearance exactly as it is (another
        // app's drawing included). Readers fit an appearance's box into the
        // annotation's Rect, so a new Rect moves and scales it.
        annot.set_rect(&map.rect_points(&annotation.rect));
        match &annotation.kind {
            AnnotationKind::Ink { strokes } => {
                annot.set_ink(&points_of(strokes, &map));
            }
            AnnotationKind::Line { from, to, arrow } => {
                annot.set_ink(&line_strokes(
                    map.points(*from),
                    map.points(*to),
                    *arrow,
                    annotation.width,
                ));
            }
            _ => {}
        }
        annot.set_string("Contents", &annotation.contents);
        annot.set_string("M", &now);
        return Ok(id);
    }
    // The appearance is drawn again from the new values (and PDFium refuses to
    // change the colour of an annotation that still has one).
    annot.clear_appearance();
    if current.kind != annotation.kind
        || current.rect != annotation.rect
        || current.width != annotation.width
    {
        write_shape(&annot, annotation, &map)?;
    }
    write_style(&annot, annotation, &now);
    write_appearance(&annot, annotation, &map);
    Ok(id)
}

/// Same colour, fill, opacity and line width.
fn same_style(a: &Annotation, b: &Annotation) -> bool {
    let fill = |x: &Annotation| match &x.kind {
        AnnotationKind::Square { fill } | AnnotationKind::Circle { fill } => *fill,
        _ => None,
    };
    a.color == b.color
        && fill(a) == fill(b)
        && (a.opacity - b.opacity).abs() < 0.005
        && (a.width - b.width).abs() < 0.01
}

/// Whether `new` is `old` moved and/or stretched from its rectangle into its
/// new one (so the old appearance, fitted into the new rectangle, is right).
fn fits_by_stretching(old: &Annotation, new: &Annotation) -> bool {
    let (o, n) = (&old.rect, &new.rect);
    let (ow, oh) = (o.right - o.left, o.bottom - o.top);
    if ow <= 0.0 || oh <= 0.0 {
        return false;
    }
    let map = |p: &PagePoint| PagePoint {
        x: n.left + (p.x - o.left) / ow * (n.right - n.left),
        y: n.top + (p.y - o.top) / oh * (n.bottom - n.top),
    };
    let close = |a: &PagePoint, b: &PagePoint| (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3;
    match (&old.kind, &new.kind) {
        (AnnotationKind::Ink { strokes: a }, AnnotationKind::Ink { strokes: b }) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|(sa, sb)| {
                    sa.len() == sb.len() && sa.iter().zip(sb).all(|(pa, pb)| close(&map(pa), pb))
                })
        }
        (
            AnnotationKind::Line {
                from: fa,
                to: ta,
                arrow: aa,
            },
            AnnotationKind::Line {
                from: fb,
                to: tb,
                arrow: ab,
            },
        ) => aa == ab && close(&map(fa), fb) && close(&map(ta), tb),
        (AnnotationKind::Markup { quads: a, .. }, AnnotationKind::Markup { quads: b, .. }) => {
            a == b
        }
        // A text box only moves: same text and style, same size.
        (a @ AnnotationKind::FreeText { .. }, b @ AnnotationKind::FreeText { .. }) => {
            a == b
                && ((n.right - n.left) - ow).abs() < 1e-4
                && ((n.bottom - n.top) - oh).abs() < 1e-4
        }
        (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

fn points_of(strokes: &[Vec<PagePoint>], map: &Mapping) -> Vec<Vec<(f32, f32)>> {
    strokes
        .iter()
        .map(|s| s.iter().map(|&p| map.points(p)).collect())
        .collect()
}

/// A line's strokes in points: the line, and for an arrow the two strokes of its head.
fn line_strokes(a: (f32, f32), b: (f32, f32), arrow: bool, width: f32) -> Vec<Vec<(f32, f32)>> {
    let mut strokes = vec![vec![a, b]];
    if arrow {
        strokes.extend(arrow_head(a, b, width));
    }
    strokes
}

/// Deletes an annotation. It is hidden and marked, and removed for good when
/// the document is saved, so undo can bring it back exactly as it was.
pub(crate) fn delete(pdfium: &Pdfium, page: &PdfPage, id: &str) -> Result<()> {
    let annots = PageAnnots::new(pdfium, page);
    let (_, annot) = annots
        .find(id)
        .filter(|(_, a)| !a.is_deleted())
        .ok_or_else(|| Error::new(ErrorCode::AnnotationNotFound, id.to_owned()))?;
    annot.set_flags(annot.flags() | FPDF_ANNOT_FLAG_HIDDEN);
    annot.set_string(DELETED_KEY, "1");
    Ok(())
}

/// Brings back a deleted annotation (undo).
pub(crate) fn restore(pdfium: &Pdfium, page: &PdfPage, id: &str) -> Result<()> {
    let annots = PageAnnots::new(pdfium, page);
    let (_, annot) = annots
        .find(id)
        .filter(|(_, a)| a.is_deleted())
        .ok_or_else(|| Error::new(ErrorCode::AnnotationNotFound, id.to_owned()))?;
    annot.set_flags(annot.flags() & !FPDF_ANNOT_FLAG_HIDDEN);
    // PDFium can't remove a key; an empty value means "not deleted".
    annot.set_string(DELETED_KEY, "");
    Ok(())
}

/// Hides or shows an annotation without changing anything else (while it is
/// being dragged, so only its preview shows).
pub(crate) fn set_hidden(pdfium: &Pdfium, page: &PdfPage, id: &str, hidden: bool) -> Result<()> {
    let annots = PageAnnots::new(pdfium, page);
    let (_, annot) = annots
        .find(id)
        .ok_or_else(|| Error::new(ErrorCode::AnnotationNotFound, id.to_owned()))?;
    let flags = annot.flags();
    annot.set_flags(if hidden {
        flags | FPDF_ANNOT_FLAG_HIDDEN
    } else {
        flags & !FPDF_ANNOT_FLAG_HIDDEN
    });
    Ok(())
}

/// Removes every annotation that touches one of `areas` (redaction), for good.
pub(crate) fn remove_touching(pdfium: &Pdfium, page: &PdfPage, areas: &[PageRect]) {
    let Some(map) = Mapping::new(page) else {
        return;
    };
    let annots = PageAnnots::new(pdfium, page);
    for index in (0..annots.count()).rev() {
        let touches = annots
            .get(index)
            .and_then(|a| a.rect())
            .is_some_and(|r| areas.iter().any(|area| overlaps(&map.rect(&r), area)));
        if touches {
            annots.remove(index);
        }
    }
}

/// Whether two page rectangles overlap.
pub(crate) fn overlaps(a: &PageRect, b: &PageRect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

/// Removes the annotations deleted on this page for good (saving a
/// password-protected file, which can't be rewritten without them).
pub(crate) fn purge_deleted(pdfium: &Pdfium, page: &PdfPage) {
    let annots = PageAnnots::new(pdfium, page);
    for index in (0..annots.count()).rev() {
        if annots.get(index).is_some_and(|a| a.is_deleted()) {
            annots.remove(index);
        }
    }
}

fn rgb(r: u32, g: u32, b: u32) -> Color {
    Color {
        r: r.min(255) as u8,
        g: g.min(255) as u8,
        b: b.min(255) as u8,
    }
}

/// Keeps a requested id (undoing a delete brings an annotation back as it
/// was), unless it's empty, an index id, or already taken on the page.
fn choose_id(annots: &PageAnnots, requested: &str) -> String {
    if requested.is_empty() || requested.starts_with('#') || annots.find(requested).is_some() {
        new_id()
    } else {
        requested.to_owned()
    }
}

fn internal(detail: &str) -> Error {
    Error::new(ErrorCode::Internal, detail)
}

/// A new, unique annotation name (`/NM`).
fn new_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("pdfrivet-{nanos:x}-{n:x}")
}

/// Converts between page fractions and PDF points for one page.
pub(crate) struct Mapping {
    to_fraction: Affine,
    to_points: Affine,
}

impl Mapping {
    pub(crate) fn new(page: &PdfPage) -> Option<Self> {
        let to_fraction = PageGeometry::new(page)?.affine()?;
        Some(Self {
            to_fraction,
            to_points: to_fraction.invert()?,
        })
    }

    fn point(&self, x: f32, y: f32) -> PagePoint {
        let (x, y) = self.to_fraction.apply(x, y);
        PagePoint { x, y }
    }

    fn points(&self, p: PagePoint) -> (f32, f32) {
        self.to_points.apply(p.x, p.y)
    }

    pub(crate) fn rect(&self, r: &FS_RECTF) -> PageRect {
        let f = self.to_fraction.rect(r.left, r.bottom, r.right, r.top);
        PageRect {
            left: f.left,
            top: f.top,
            right: f.right,
            bottom: f.bottom,
        }
    }

    /// A page rectangle in points: (left, bottom, right, top).
    pub(crate) fn rect_points(&self, r: &PageRect) -> FS_RECTF {
        let (x1, y1) = self.points(PagePoint {
            x: r.left,
            y: r.top,
        });
        let (x2, y2) = self.points(PagePoint {
            x: r.right,
            y: r.bottom,
        });
        FS_RECTF {
            left: x1.min(x2),
            right: x1.max(x2),
            bottom: y1.min(y2),
            top: y1.max(y2),
        }
    }
}

fn annot_to_model(annot: &Annot, index: i32, map: &Mapping, texts: &Texts) -> Option<Annotation> {
    let subtype = annot.subtype() as u32;
    let mut editable = true;
    let mut text_color = None;
    let kind = match subtype {
        FPDF_ANNOT_LINK | FPDF_ANNOT_WIDGET | FPDF_ANNOT_POPUP => return None,
        FPDF_ANNOT_HIGHLIGHT | FPDF_ANNOT_UNDERLINE | FPDF_ANNOT_STRIKEOUT
        | FPDF_ANNOT_SQUIGGLY => AnnotationKind::Markup {
            style: match subtype {
                FPDF_ANNOT_HIGHLIGHT => MarkupStyle::Highlight,
                FPDF_ANNOT_UNDERLINE => MarkupStyle::Underline,
                FPDF_ANNOT_STRIKEOUT => MarkupStyle::Strikeout,
                _ => MarkupStyle::Squiggly,
            },
            quads: annot.quads().iter().map(|q| quad_to_rect(q, map)).collect(),
        },
        FPDF_ANNOT_INK => {
            let strokes: Vec<Vec<PagePoint>> = annot
                .ink_strokes()
                .into_iter()
                .map(|s| s.into_iter().map(|p| map.point(p.x, p.y)).collect())
                .collect();
            match annot.string(SHAPE_KEY).as_deref() {
                Some(shape @ ("line" | "arrow"))
                    if !strokes.is_empty() && strokes[0].len() >= 2 =>
                {
                    AnnotationKind::Line {
                        from: strokes[0][0],
                        to: strokes[0][strokes[0].len() - 1],
                        arrow: shape == "arrow",
                    }
                }
                _ => AnnotationKind::Ink { strokes },
            }
        }
        FPDF_ANNOT_SQUARE => AnnotationKind::Square {
            fill: annot.fill_color(),
        },
        FPDF_ANNOT_CIRCLE => AnnotationKind::Circle {
            fill: annot.fill_color(),
        },
        FPDF_ANNOT_TEXT => AnnotationKind::Note,
        FPDF_ANNOT_FREETEXT => {
            let rect = annot.rect().map(|r| map.rect(&r))?;
            let (kind, color) = read_text_box(annot, &rect, map, texts);
            text_color = color;
            kind
        }
        FPDF_ANNOT_STAMP => {
            // Only PDFRivet's own signature pictures can be moved and resized.
            editable = annot.string(SHAPE_KEY).as_deref() == Some("signature");
            AnnotationKind::Stamp
        }
        other => {
            editable = false;
            AnnotationKind::Other {
                subtype: subtype_name(other).to_owned(),
            }
        }
    };
    let rect = annot.rect().map(|r| map.rect(&r))?;
    Some(Annotation {
        id: annot
            .string("NM")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("#{index}")),
        kind,
        rect,
        color: text_color
            .or_else(|| {
                annot.main_color(matches!(
                    subtype,
                    FPDF_ANNOT_HIGHLIGHT | FPDF_ANNOT_TEXT | FPDF_ANNOT_FREETEXT
                ))
            })
            .unwrap_or(Color { r: 0, g: 0, b: 0 }),
        // Stroke opacity, or (some apps, e.g. Apple's highlighter) only the fill opacity.
        opacity: annot
            .number("CA")
            .or_else(|| annot.number("ca"))
            .unwrap_or(1.0)
            .clamp(0.0, 1.0),
        width: annot.border_width().unwrap_or(1.0),
        contents: annot.text("Contents", texts).unwrap_or_default(),
        author: annot.text("T", texts).unwrap_or_default(),
        modified: annot
            .string("M")
            .and_then(|m| crate::metadata::pdf_date_to_iso(&m)),
        // Locked annotations stay as they are.
        editable: editable && annot.flags() & FPDF_ANNOT_FLAG_LOCKED == 0,
        reply_to: None,
    })
}

fn subtype_name(subtype: u32) -> &'static str {
    match subtype {
        3 => "FreeText",
        4 => "Line",
        7 => "Polygon",
        8 => "PolyLine",
        14 => "Caret",
        17 => "FileAttachment",
        18 => "Sound",
        19 => "Movie",
        21 => "Screen",
        22 => "PrinterMark",
        23 => "TrapNet",
        24 => "Watermark",
        25 => "3D",
        26 => "RichMedia",
        27 => "XFAWidget",
        28 => "Redact",
        _ => "Unknown",
    }
}

/// A quadrilateral (any corner order) → the rectangle around it, as fractions.
fn quad_to_rect(q: &FS_QUADPOINTSF, map: &Mapping) -> PageRect {
    let corners = [
        map.point(q.x1, q.y1),
        map.point(q.x2, q.y2),
        map.point(q.x3, q.y3),
        map.point(q.x4, q.y4),
    ];
    let xs = corners.map(|p| p.x);
    let ys = corners.map(|p| p.y);
    PageRect {
        left: xs.iter().copied().fold(f32::INFINITY, f32::min),
        top: ys.iter().copied().fold(f32::INFINITY, f32::min),
        right: xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        bottom: ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
    }
}

/// Writes the geometry: quads, strokes, line, or rectangle (and the annotation's Rect).
fn write_shape(annot: &Annot, annotation: &Annotation, map: &Mapping) -> Result<()> {
    let pad = annotation.width.max(1.0);
    match &annotation.kind {
        AnnotationKind::Markup { quads, .. } => {
            if quads.is_empty() {
                return Err(internal("text markup needs at least one line"));
            }
            // Text markup follows its text: it is restyled, never moved, so
            // the lines are written only when it is created.
            if annot.quad_count() == 0 {
                for q in quads {
                    annot.append_quad(&rect_to_quad(q, map));
                }
            }
            let bounds = bounds(quads.iter().flat_map(|q| {
                let r = map.rect_points(q);
                [(r.left, r.bottom), (r.right, r.top)]
            }));
            annot.set_rect(&bounds);
        }
        AnnotationKind::Ink { strokes } => {
            write_strokes(annot, &points_of(strokes, map), pad)?;
            if annot.string(SHAPE_KEY).is_some() {
                annot.set_string(SHAPE_KEY, "");
            }
        }
        AnnotationKind::Line { from, to, arrow } => {
            let strokes =
                line_strokes(map.points(*from), map.points(*to), *arrow, annotation.width);
            write_strokes(annot, &strokes, pad)?;
            annot.set_string(SHAPE_KEY, if *arrow { "arrow" } else { "line" });
        }
        AnnotationKind::Square { .. } | AnnotationKind::Circle { .. } | AnnotationKind::Note => {
            annot.set_rect(&map.rect_points(&annotation.rect));
        }
        AnnotationKind::FreeText { .. } => {
            if let Some((_, rect)) = text_box(annotation, map) {
                annot.set_rect(&rect);
            }
        }
        AnnotationKind::Stamp | AnnotationKind::Other { .. } => {}
    }
    Ok(())
}

/// A text box's layout and Rect (PDF points). A box that grows with its text
/// keeps its start corner where it is: top-left, or top-right for text that
/// runs right to left.
fn text_box(annotation: &Annotation, map: &Mapping) -> Option<(textlayout::Layout, FS_RECTF)> {
    let AnnotationKind::FreeText { text, style } = &annotation.kind else {
        return None;
    };
    let layout = textlayout::layout(text, style);
    let r = map.rect_points(&annotation.rect);
    let w = layout.width + 2.0 * TEXT_PADDING;
    let h = layout.height + 2.0 * TEXT_PADDING;
    let (left, right) = if style.width.is_none() && layout.rtl {
        (r.right - w, r.right)
    } else {
        (r.left, r.left + w)
    };
    Some((
        layout,
        FS_RECTF {
            left,
            top: r.top,
            right,
            bottom: r.top - h,
        },
    ))
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Escapes text for the rich text (XHTML) of `/RC`.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Whether a paragraph runs right to left (forced, or from its first strong letter).
fn paragraph_rtl(text: &str, direction: TextDirection) -> bool {
    match direction {
        TextDirection::Rtl => true,
        TextDirection::Ltr => false,
        TextDirection::Auto => matches!(
            unicode_bidi::get_base_direction(text),
            unicode_bidi::Direction::Rtl
        ),
    }
}

/// Writes a text box's text and style the standard ways, so other apps
/// (Acrobat above all) can show and edit it: `/Contents`, `/DA`, `/DS`, `/RC`.
fn write_text_box(annot: &Annot, text: &str, style: &TextStyle, color: Color) {
    let c = |v: u8| f32::from(v) / 255.0;
    let weight = if style.bold { "bold" } else { "normal" };
    let family = style.font.family.replace(['\'', '"', ';'], "");
    let fixed_align = |a: TextAlign| match a {
        TextAlign::Left => Some("left"),
        TextAlign::Center => Some("center"),
        TextAlign::Right => Some("right"),
        TextAlign::Auto => None,
    };
    let valign = match style.valign {
        VerticalAlign::Top => "top",
        VerticalAlign::Middle => "middle",
        VerticalAlign::Bottom => "bottom",
    };
    let first_rtl = paragraph_rtl(text.lines().next().unwrap_or(""), style.direction);
    let box_align = fixed_align(style.align).unwrap_or(if first_rtl { "right" } else { "left" });
    annot.set_string("Contents", text);
    annot.set_string(
        "DA",
        &format!(
            "/Helv {} Tf {:.3} {:.3} {:.3} rg",
            style.size,
            c(color.r),
            c(color.g),
            c(color.b)
        ),
    );
    annot.set_string(
        "DS",
        &format!(
            "font: {weight} {}pt '{family}'; text-align:{box_align}; vertical-align:{valign}; color:{}",
            style.size,
            hex(color)
        ),
    );
    let mut rc = format!(
        "<?xml version=\"1.0\"?><body xmlns=\"http://www.w3.org/1999/xhtml\" \
         xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\" xfa:APIVersion=\"Acrobat:11.0.0\" \
         xfa:spec=\"2.0.2\" style=\"font-size:{}pt;font-weight:{weight};font-family:'{family}';color:{}\">",
        style.size,
        hex(color)
    );
    for para in text.split('\n') {
        let rtl = paragraph_rtl(para, style.direction);
        let align = fixed_align(style.align).unwrap_or(if rtl { "right" } else { "left" });
        rc += &format!(
            "<p dir=\"{}\" style=\"text-align:{align}\">{}</p>",
            if rtl { "rtl" } else { "ltr" },
            xml_escape(para)
        );
    }
    rc += "</body>";
    annot.set_string("RC", &rc);
    annot.set_string(TYPEWRITER_KEY, if style.width.is_none() { "1" } else { "" });
    let settings = serde_json::json!({ "style": style, "color": color });
    annot.set_string(TEXT_KEY, &settings.to_string());
    // The box itself has no border; its text is drawn by us.
    annot.set_border_width(0.0);
}

/// A text box written by another app: its text, and its style as far as its
/// `/DS`, `/RC` or `/DA` tell.
fn read_text_box(
    annot: &Annot,
    rect: &PageRect,
    map: &Mapping,
    texts: &Texts,
) -> (AnnotationKind, Option<Color>) {
    let text = annot
        .text("Contents", texts)
        .unwrap_or_default()
        .replace('\r', "\n");
    if let Some(saved) = annot
        .string(TEXT_KEY)
        .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
    {
        let style = saved
            .get("style")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok());
        let color = saved
            .get("color")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok());
        if let Some(style) = style {
            return (AnnotationKind::FreeText { text, style }, color);
        }
    }
    let ds = annot.string("DS").unwrap_or_default();
    let rc = annot.string("RC").unwrap_or_default();
    let da = annot.string("DA").unwrap_or_default();
    let css = format!("{ds};{rc}");
    let size = css_size(&css)
        .or_else(|| {
            // "/Helv 12 Tf": the number before Tf.
            let words: Vec<&str> = da.split_whitespace().collect();
            let at = words.iter().position(|w| *w == "Tf")?;
            words.get(at.checked_sub(1)?)?.parse().ok()
        })
        .filter(|s: &f32| *s > 0.0)
        .unwrap_or(12.0);
    // `font-family: X`, or what follows the size in `font: bold 12pt X`.
    let family = css_value(&css, "font-family")
        .or_else(|| {
            let font = css_value(&css, "font")?;
            let words: Vec<&str> = font.split_whitespace().collect();
            let size_at = words.iter().position(|w| w.ends_with("pt"))?;
            Some(words[size_at + 1..].join(" "))
        })
        .map(|f| f.trim_matches(|c| c == '\'' || c == '"').to_owned())
        .filter(|f| !f.is_empty());
    let bundled = family
        .as_deref()
        .is_some_and(|f| ["rubik", "amiri"].contains(&f.to_lowercase().as_str()));
    let align = match css_value(&css, "text-align").as_deref() {
        Some("center") => TextAlign::Center,
        Some("right") => TextAlign::Right,
        Some("left") => TextAlign::Left,
        _ => TextAlign::Auto,
    };
    let typewriter = annot
        .string("IT")
        .is_some_and(|it| it.contains("TypeWriter"));
    let points = map.rect_points(rect);
    let style = TextStyle {
        font: match family {
            Some(family) => TextFont { family, bundled },
            None => TextFont::default(),
        },
        size,
        bold: css.contains("bold"),
        align,
        valign: VerticalAlign::Top,
        direction: TextDirection::Auto,
        width: (!typewriter).then_some((points.right - points.left - 2.0 * TEXT_PADDING).max(size)),
        height: None,
    };
    let color = css_value(&css, "color").and_then(|c| parse_hex(&c));
    (AnnotationKind::FreeText { text, style }, color)
}

/// The value of a CSS property in a style string (first match).
fn css_value(css: &str, property: &str) -> Option<String> {
    let lower = css.to_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(property) {
        let at = from + i;
        from = at + property.len();
        // Not part of a longer name (e.g. "color" in "background-color").
        let before = at.checked_sub(1).map(|i| lower.as_bytes()[i]);
        if before.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'-') {
            continue;
        }
        let rest = css[from..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let end = rest.find([';', '"', '>']).unwrap_or(rest.len());
        return Some(rest[..end].trim().to_owned());
    }
    None
}

/// The font size in a style string: `font-size:12pt` or `font: … 12pt …`.
fn css_size(css: &str) -> Option<f32> {
    let value = css_value(css, "font-size").or_else(|| css_value(css, "font"))?;
    value
        .split_whitespace()
        .find_map(|w| w.strip_suffix("pt").and_then(|n| n.parse().ok()))
}

fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some(Color {
        r: v(0)?,
        g: v(2)?,
        b: v(4)?,
    })
}

/// A text box's appearance: its text's outlines, filled in its colour.
fn text_box_appearance(annotation: &Annotation, map: &Mapping) -> String {
    let Some((layout, rect)) = text_box(annotation, map) else {
        return String::new();
    };
    let path = layout.to_pdf_path(rect.left + TEXT_PADDING, rect.top - TEXT_PADDING);
    if path.is_empty() {
        return String::new();
    }
    let c = |v: u8| f32::from(v) / 255.0;
    let (r, g, b) = (
        c(annotation.color.r),
        c(annotation.color.g),
        c(annotation.color.b),
    );
    if layout.fake_bold() {
        // No bold face: fill and outline the letters a little to thicken them.
        let width = layout.size * 0.035;
        format!("q {r:.3} {g:.3} {b:.3} rg {r:.3} {g:.3} {b:.3} RG {width:.2} w 1 j {path}B Q")
    } else {
        format!("q {r:.3} {g:.3} {b:.3} rg {path}f Q")
    }
}

fn write_strokes(annot: &Annot, strokes: &[Vec<(f32, f32)>], pad: f32) -> Result<()> {
    if !annot.set_ink(strokes) {
        return Err(internal("PDFium refused an ink stroke"));
    }
    let mut rect = bounds(strokes.iter().flatten().copied());
    rect.left -= pad;
    rect.bottom -= pad;
    rect.right += pad;
    rect.top += pad;
    annot.set_rect(&rect);
    Ok(())
}

/// The two strokes of an arrow head at `to`, in points.
fn arrow_head(from: (f32, f32), to: (f32, f32), width: f32) -> Vec<Vec<(f32, f32)>> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    if length < f32::EPSILON {
        return Vec::new();
    }
    let size = (width * 4.0).max(8.0).min(length * 0.6);
    let angle = dy.atan2(dx);
    let spread = 28f32.to_radians();
    [
        angle + std::f32::consts::PI - spread,
        angle + std::f32::consts::PI + spread,
    ]
    .iter()
    .map(|a| vec![to, (to.0 + size * a.cos(), to.1 + size * a.sin())])
    .collect()
}

fn bounds(points: impl Iterator<Item = (f32, f32)>) -> FS_RECTF {
    let mut r = FS_RECTF {
        left: f32::INFINITY,
        bottom: f32::INFINITY,
        right: f32::NEG_INFINITY,
        top: f32::NEG_INFINITY,
    };
    for (x, y) in points {
        r.left = r.left.min(x);
        r.right = r.right.max(x);
        r.bottom = r.bottom.min(y);
        r.top = r.top.max(y);
    }
    r
}

/// A rectangle of text → quad points in the order readers expect: top-left,
/// top-right, bottom-left, bottom-right (as the page is shown).
fn rect_to_quad(r: &PageRect, map: &Mapping) -> FS_QUADPOINTSF {
    let (x1, y1) = map.points(PagePoint {
        x: r.left,
        y: r.top,
    });
    let (x2, y2) = map.points(PagePoint {
        x: r.right,
        y: r.top,
    });
    let (x3, y3) = map.points(PagePoint {
        x: r.left,
        y: r.bottom,
    });
    let (x4, y4) = map.points(PagePoint {
        x: r.right,
        y: r.bottom,
    });
    FS_QUADPOINTSF {
        x1,
        y1,
        x2,
        y2,
        x3,
        y3,
        x4,
        y4,
    }
}

/// Colours, opacity, width, text, author and modification date.
fn write_style(annot: &Annot, annotation: &Annotation, now: &str) {
    if let AnnotationKind::FreeText { text, style } = &annotation.kind {
        // A text box's colour is its text's; `/C` would be read as its background.
        write_text_box(annot, text, style, annotation.color);
        annot.set_string("T", &annotation.author);
        annot.set_string("M", now);
        return;
    }
    let alpha = (annotation.opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    annot.set_color(COLOR_STROKE, annotation.color, alpha);
    if let AnnotationKind::Square { fill } | AnnotationKind::Circle { fill } = &annotation.kind {
        match fill {
            Some(fill) => annot.set_color(COLOR_FILL, *fill, alpha),
            None => annot.remove_interior_color(),
        }
    }
    if !matches!(
        annotation.kind,
        AnnotationKind::Markup { .. } | AnnotationKind::Note
    ) {
        annot.set_border_width(annotation.width.max(0.0));
    }
    annot.set_string("Contents", &annotation.contents);
    annot.set_string("T", &annotation.author);
    annot.set_string("M", now);
}

/// Draws the appearance of kinds PDFium would draw differently from what was
/// chosen. Everything else is drawn by PDFium the first time the page renders.
fn write_appearance(annot: &Annot, annotation: &Annotation, map: &Mapping) {
    if annotation.reply_to.is_some() {
        // Replies aren't drawn: they show under the comment they answer.
        annot.set_appearance("");
        return;
    }
    match &annotation.kind {
        // PDFium always draws notes yellow; this draws them in their colour.
        AnnotationKind::Note => annot.set_appearance(&note_appearance(
            &map.rect_points(&annotation.rect),
            annotation.color,
        )),
        // PDFium draws ink with square ends and corners; pens and highlighters
        // look right with round ones (and match what was seen while drawing).
        AnnotationKind::Ink { strokes } => {
            annot.set_appearance(&ink_appearance(&points_of(strokes, map), annotation));
        }
        AnnotationKind::Line { from, to, arrow } => {
            let strokes =
                line_strokes(map.points(*from), map.points(*to), *arrow, annotation.width);
            annot.set_appearance(&ink_appearance(&strokes, annotation));
        }
        // Text is laid out and drawn by us (PDFium can't shape Arabic).
        AnnotationKind::FreeText { .. } => {
            annot.set_appearance(&text_box_appearance(annotation, map));
        }
        _ => {}
    }
}

/// Strokes (PDF points) drawn with round ends and joins, in the annotation's
/// colour, width and opacity. PDFium adds the opacity as the `GS` graphics
/// state when the appearance is set, because the annotation has a `CA` value.
fn ink_appearance(strokes: &[Vec<(f32, f32)>], annotation: &Annotation) -> String {
    let c = |v: u8| f32::from(v) / 255.0;
    let mut out = String::from("q ");
    if annotation.opacity < 0.999 {
        out += "/GS gs ";
    }
    out += &format!(
        "{:.3} {:.3} {:.3} RG {:.2} w 1 J 1 j\n",
        c(annotation.color.r),
        c(annotation.color.g),
        c(annotation.color.b),
        annotation.width.max(0.1)
    );
    for stroke in strokes {
        let Some(((x0, y0), rest)) = stroke.split_first() else {
            continue;
        };
        out += &format!("{x0:.2} {y0:.2} m");
        if rest.is_empty() {
            // A dot: a zero-length line shows as a round dot with round caps.
            out += &format!(" {x0:.2} {y0:.2} l");
        }
        for (x, y) in rest {
            out += &format!(" {x:.2} {y:.2} l");
        }
        out += " S\n";
    }
    out + "Q"
}

/// A sticky-note icon filling `r` (PDF points): a sheet with a folded corner
/// and three lines of "text", in `color` with a darker outline.
fn note_appearance(r: &FS_RECTF, color: Color) -> String {
    let (w, h) = (r.right - r.left, r.top - r.bottom);
    let (x, y) = (r.left, r.bottom);
    let fold = w.min(h) * 0.28;
    let c = |v: u8| f32::from(v) / 255.0;
    let dark = |v: u8| c(v) * 0.45;
    let mut out = format!(
        "q {:.3} {:.3} {:.3} rg {:.3} {:.3} {:.3} RG 1 w 1 j\n",
        c(color.r),
        c(color.g),
        c(color.b),
        dark(color.r),
        dark(color.g),
        dark(color.b),
    );
    // The sheet, its top-right corner folded.
    let (l, b, rt, t) = (x + 0.5, y + 0.5, x + w - 0.5, y + h - 0.5);
    out += &format!(
        "{l:.2} {b:.2} m {rt:.2} {b:.2} l {rt:.2} {:.2} l {:.2} {t:.2} l {l:.2} {t:.2} l h B\n",
        t - fold,
        rt - fold
    );
    out += &format!(
        "{:.2} {t:.2} m {:.2} {:.2} l {rt:.2} {:.2} l S\n",
        rt - fold,
        rt - fold,
        t - fold,
        t - fold
    );
    // Lines of text.
    for i in 1..=3 {
        let ly = b + h * (0.18 + 0.17 * i as f32);
        let end = if i == 3 { x + w * 0.55 } else { x + w * 0.72 };
        out += &format!("{:.2} {ly:.2} m {end:.2} {ly:.2} l S\n", x + w * 0.22);
    }
    out + "Q"
}

/// The annotations of one page.
struct PageAnnots<'a> {
    bindings: &'a dyn PdfiumLibraryBindings,
    page: FPDF_PAGE,
}

impl<'a> PageAnnots<'a> {
    fn new(pdfium: &'a Pdfium, page: &'a PdfPage) -> Self {
        Self {
            bindings: pdfium.bindings(),
            page: pdfium.bindings().get_handle_from_page(page),
        }
    }

    fn count(&self) -> i32 {
        // SAFETY: the page handle belongs to a live PdfPage borrowed for 'a.
        unsafe { self.bindings.FPDFPage_GetAnnotCount(self.page) }
    }

    fn get(&self, index: i32) -> Option<Annot<'a>> {
        // SAFETY: as above; PDFium checks the index and returns NULL if it's bad.
        let handle = unsafe { self.bindings.FPDFPage_GetAnnot(self.page, index) };
        Annot::wrap(self.bindings, handle)
    }

    fn create(&self, subtype: FPDF_ANNOTATION_SUBTYPE) -> Option<Annot<'a>> {
        // SAFETY: as above; returns NULL for unsupported subtypes.
        let handle = unsafe { self.bindings.FPDFPage_CreateAnnot(self.page, subtype) };
        Annot::wrap(self.bindings, handle)
    }

    fn remove(&self, index: i32) -> bool {
        // SAFETY: as above. No handle to this annotation is open (callers drop theirs first).
        unsafe { self.bindings.FPDFPage_RemoveAnnot(self.page, index) != 0 }
    }

    /// The id of the annotation `annot` replies to, if it is a reply: PDFRivet's
    /// own (not saved yet), or one from the file (`/IRT`).
    fn reply_target(&self, annot: &Annot<'a>) -> Option<String> {
        if let Some(parent) = annot.string(REPLY_KEY).filter(|s| !s.is_empty()) {
            return Some(parent);
        }
        // SAFETY: `annot` is a live handle on this page; the linked annotation's
        // handle is wrapped at once, so it is closed exactly once.
        let linked = unsafe { self.bindings.FPDFAnnot_GetLinkedAnnot(annot.handle, "IRT") };
        let parent = Annot::wrap(self.bindings, linked)?;
        if let Some(name) = parent.string("NM").filter(|s| !s.is_empty()) {
            return Some(name);
        }
        // SAFETY: as above.
        let index = unsafe {
            self.bindings
                .FPDFPage_GetAnnotIndex(self.page, parent.handle)
        };
        (index >= 0).then(|| format!("#{index}"))
    }

    /// The annotation with this id: its `/NM` name, or `#<index>` for one without a name.
    fn find(&self, id: &str) -> Option<(i32, Annot<'a>)> {
        if let Some(index) = id.strip_prefix('#').and_then(|i| i.parse::<i32>().ok()) {
            let annot = self.get(index)?;
            // An index id only fits an annotation that still has no name.
            return annot
                .string("NM")
                .is_none_or(|n| n.is_empty())
                .then_some((index, annot));
        }
        (0..self.count()).find_map(|i| {
            let annot = self.get(i)?;
            (annot.string("NM").as_deref() == Some(id)).then_some((i, annot))
        })
    }
}

/// An open annotation handle; closed when dropped.
struct Annot<'a> {
    bindings: &'a dyn PdfiumLibraryBindings,
    handle: FPDF_ANNOTATION,
}

impl<'a> Annot<'a> {
    fn wrap(bindings: &'a dyn PdfiumLibraryBindings, handle: FPDF_ANNOTATION) -> Option<Self> {
        (!handle.is_null()).then_some(Self { bindings, handle })
    }

    fn subtype(&self) -> FPDF_ANNOTATION_SUBTYPE {
        // SAFETY (this and every method below): `handle` is a live annotation
        // handle (see `wrap`), used on the PDFium thread before `drop` closes it;
        // every pointer passed points to a local that outlives the call.
        unsafe { self.bindings.FPDFAnnot_GetSubtype(self.handle) }
    }

    fn flags(&self) -> i32 {
        // SAFETY: see `subtype`.
        unsafe { self.bindings.FPDFAnnot_GetFlags(self.handle) }
    }

    fn set_flags(&self, flags: i32) {
        // SAFETY: see `subtype`.
        unsafe { self.bindings.FPDFAnnot_SetFlags(self.handle, flags) };
    }

    fn rect(&self) -> Option<FS_RECTF> {
        let mut r = FS_RECTF {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        };
        // SAFETY: see `subtype`.
        let ok = unsafe { self.bindings.FPDFAnnot_GetRect(self.handle, &mut r) };
        (ok != 0).then_some(r)
    }

    fn set_rect(&self, r: &FS_RECTF) {
        // SAFETY: see `subtype`.
        unsafe { self.bindings.FPDFAnnot_SetRect(self.handle, r) };
    }

    /// The colour stored in the annotation (`C` or `IC`). PDFium only gives it
    /// while the annotation has no appearance yet (see [`Annot::drawn_colors`]).
    fn stored_color(&self, kind: FPDFANNOT_COLORTYPE) -> Option<Color> {
        let key = if kind == COLOR_FILL { "IC" } else { "C" };
        // SAFETY: see `subtype`.
        if unsafe { self.bindings.FPDFAnnot_GetValueType(self.handle, key) } != FPDF_OBJECT_ARRAY {
            return None;
        }
        let (mut r, mut g, mut b, mut a) = (0, 0, 0, 0);
        // SAFETY: see `subtype`.
        let ok = unsafe {
            self.bindings
                .FPDFAnnot_GetColor(self.handle, kind, &mut r, &mut g, &mut b, &mut a)
        };
        (ok != 0).then_some(rgb(r, g, b))
    }

    /// The stroke and fill colours the appearance is drawn with. Once PDFium (or
    /// another app) has drawn an annotation, that drawing is what you see, so
    /// its colours are read from the drawing's paths.
    fn drawn_colors(&self) -> (Option<Color>, Option<Color>) {
        let (mut stroke, mut fill) = (None, None);
        // SAFETY: see `subtype`. Objects belong to the annotation's appearance and
        // stay valid while the annotation handle is open; they are only read.
        unsafe {
            for i in 0..self.bindings.FPDFAnnot_GetObjectCount(self.handle) {
                let object = self.bindings.FPDFAnnot_GetObject(self.handle, i);
                if object.is_null()
                    || self.bindings.FPDFPageObj_GetType(object) != FPDF_PAGEOBJ_PATH
                {
                    continue;
                }
                let (mut fill_mode, mut stroked) = (0, 0);
                self.bindings
                    .FPDFPath_GetDrawMode(object, &mut fill_mode, &mut stroked);
                let (mut r, mut g, mut b, mut a) = (0, 0, 0, 0);
                if stroke.is_none()
                    && stroked != 0
                    && self
                        .bindings
                        .FPDFPageObj_GetStrokeColor(object, &mut r, &mut g, &mut b, &mut a)
                        != 0
                {
                    stroke = Some(rgb(r, g, b));
                }
                if fill.is_none()
                    && fill_mode != 0
                    && self
                        .bindings
                        .FPDFPageObj_GetFillColor(object, &mut r, &mut g, &mut b, &mut a)
                        != 0
                {
                    fill = Some(rgb(r, g, b));
                }
            }
        }
        (stroke, fill)
    }

    fn has_appearance(&self) -> bool {
        // SAFETY: see `subtype`.
        unsafe { self.bindings.FPDFAnnot_HasKey(self.handle, "AP") != 0 }
    }

    /// The annotation's colour. Highlights and notes are drawn filled with it,
    /// everything else stroked.
    fn main_color(&self, filled: bool) -> Option<Color> {
        self.stored_color(COLOR_STROKE).or_else(|| {
            let (stroke, fill) = self.drawn_colors();
            if filled {
                fill.or(stroke)
            } else {
                stroke.or(fill)
            }
        })
    }

    /// The fill of a square or circle, or `None` if it isn't filled.
    fn fill_color(&self) -> Option<Color> {
        if self.has_appearance() {
            self.drawn_colors().1
        } else {
            self.stored_color(COLOR_FILL)
        }
    }

    /// Sets a colour; PDFium stores the alpha as the annotation's opacity (`CA`).
    fn set_color(&self, kind: FPDFANNOT_COLORTYPE, color: Color, alpha: u32) {
        // SAFETY: see `subtype`.
        unsafe {
            self.bindings.FPDFAnnot_SetColor(
                self.handle,
                kind,
                color.r.into(),
                color.g.into(),
                color.b.into(),
                alpha,
            )
        };
    }

    /// Removes the fill of a square or circle. PDFium has no call for this, but
    /// an empty colour array means "no fill", and an empty string isn't a colour,
    /// so PDFium and other readers then draw no fill.
    fn remove_interior_color(&self) {
        // SAFETY: see `subtype`.
        if unsafe { self.bindings.FPDFAnnot_HasKey(self.handle, "IC") } != 0 {
            self.set_string("IC", "");
        }
    }

    fn number(&self, key: &str) -> Option<f32> {
        let mut value = 0.0;
        // SAFETY: see `subtype`.
        let ok = unsafe {
            self.bindings
                .FPDFAnnot_GetNumberValue(self.handle, key, &mut value)
        };
        (ok != 0).then_some(value)
    }

    fn border_width(&self) -> Option<f32> {
        let (mut h, mut v, mut w) = (0.0, 0.0, 0.0);
        // SAFETY: see `subtype`.
        let ok = unsafe {
            self.bindings
                .FPDFAnnot_GetBorder(self.handle, &mut h, &mut v, &mut w)
        };
        (ok != 0).then_some(w)
    }

    fn set_border_width(&self, width: f32) {
        // SAFETY: see `subtype`.
        unsafe {
            self.bindings
                .FPDFAnnot_SetBorder(self.handle, 0.0, 0.0, width)
        };
    }

    /// A string value, up to the first U+0000.
    fn string(&self, key: &str) -> Option<String> {
        let mut value = self.whole_string(key)?;
        value.truncate(value.find('\0').unwrap_or(value.len()));
        Some(value)
    }

    /// A text string (contents, author…) as its author meant it, repaired if
    /// PDFium misread it (see `textstrings.rs`).
    fn text(&self, key: &str, texts: &Texts) -> Option<String> {
        self.whole_string(key).map(|value| texts.repair(value))
    }

    /// A string value with any U+0000 inside it, which may stand for a byte
    /// PDFium couldn't decode (see `textstrings.rs`).
    fn whole_string(&self, key: &str) -> Option<String> {
        // SAFETY: see `subtype`. The first call asks for the length in bytes
        // (including the terminating NUL); the second fills a buffer that size.
        unsafe {
            if self.bindings.FPDFAnnot_HasKey(self.handle, key) == 0 {
                return None;
            }
            let bytes =
                self.bindings
                    .FPDFAnnot_GetStringValue(self.handle, key, std::ptr::null_mut(), 0)
                    as usize;
            if bytes < 2 {
                return Some(String::new());
            }
            let mut buffer = vec![0u16; bytes / 2];
            self.bindings.FPDFAnnot_GetStringValue(
                self.handle,
                key,
                buffer.as_mut_ptr(),
                bytes as _,
            );
            let len = buffer.iter().rposition(|&c| c != 0).map_or(0, |i| i + 1);
            Some(String::from_utf16_lossy(&buffer[..len]))
        }
    }

    fn set_string(&self, key: &str, value: &str) {
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: see `subtype`; `wide` is NUL-terminated and outlives the call.
        unsafe {
            self.bindings
                .FPDFAnnot_SetStringValue(self.handle, key, wide.as_ptr())
        };
    }

    /// Removes the normal appearance, so PDFium draws the annotation again from its values.
    fn clear_appearance(&self) {
        // SAFETY: see `subtype`; a NULL value removes the appearance.
        unsafe {
            self.bindings.FPDFAnnot_SetAP(
                self.handle,
                FPDF_ANNOT_APPEARANCEMODE_NORMAL,
                std::ptr::null(),
            )
        };
    }

    /// Replaces the normal appearance with a content stream (in page coordinates,
    /// inside the annotation's Rect).
    fn set_appearance(&self, content: &str) {
        let wide: Vec<u16> = content.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: see `subtype`; `wide` is NUL-terminated and outlives the call.
        unsafe {
            self.bindings.FPDFAnnot_SetAP(
                self.handle,
                FPDF_ANNOT_APPEARANCEMODE_NORMAL,
                wide.as_ptr(),
            )
        };
    }

    fn quad_count(&self) -> usize {
        // SAFETY: see `subtype`.
        unsafe { self.bindings.FPDFAnnot_CountAttachmentPoints(self.handle) }
    }

    fn quads(&self) -> Vec<FS_QUADPOINTSF> {
        (0..self.quad_count())
            .filter_map(|i| {
                let mut q = FS_QUADPOINTSF {
                    x1: 0.0,
                    y1: 0.0,
                    x2: 0.0,
                    y2: 0.0,
                    x3: 0.0,
                    y3: 0.0,
                    x4: 0.0,
                    y4: 0.0,
                };
                // SAFETY: see `subtype`.
                let ok = unsafe {
                    self.bindings
                        .FPDFAnnot_GetAttachmentPoints(self.handle, i as _, &mut q)
                };
                (ok != 0).then_some(q)
            })
            .collect()
    }

    fn append_quad(&self, q: &FS_QUADPOINTSF) {
        // SAFETY: see `subtype`.
        unsafe {
            self.bindings
                .FPDFAnnot_AppendAttachmentPoints(self.handle, q)
        };
    }

    fn ink_strokes(&self) -> Vec<Vec<FS_POINTF>> {
        // SAFETY: see `subtype`. Each path's length is asked for first, then a
        // buffer of exactly that many points is filled.
        unsafe {
            let count = self.bindings.FPDFAnnot_GetInkListCount(self.handle);
            (0..count)
                .map(|i| {
                    let len = self.bindings.FPDFAnnot_GetInkListPath(
                        self.handle,
                        i,
                        std::ptr::null_mut(),
                        0,
                    );
                    let mut points = vec![FS_POINTF { x: 0.0, y: 0.0 }; len as usize];
                    self.bindings.FPDFAnnot_GetInkListPath(
                        self.handle,
                        i,
                        points.as_mut_ptr(),
                        len,
                    );
                    points
                })
                .collect()
        }
    }

    /// Whether the annotation was deleted in PDFRivet (see [`delete`]).
    fn is_deleted(&self) -> bool {
        self.string(DELETED_KEY).as_deref() == Some("1")
    }

    /// Replaces the ink strokes (PDF points).
    fn set_ink(&self, strokes: &[Vec<(f32, f32)>]) -> bool {
        self.remove_ink();
        strokes
            .iter()
            .all(|s| !s.is_empty() && self.add_ink_stroke(s))
    }

    fn remove_ink(&self) {
        // SAFETY: see `subtype`.
        unsafe { self.bindings.FPDFAnnot_RemoveInkList(self.handle) };
    }

    fn add_ink_stroke(&self, points: &[(f32, f32)]) -> bool {
        let points: Vec<FS_POINTF> = points.iter().map(|&(x, y)| FS_POINTF { x, y }).collect();
        // SAFETY: see `subtype`; `points` outlives the call and its length is passed.
        unsafe {
            self.bindings
                .FPDFAnnot_AddInkStroke(self.handle, points.as_ptr(), points.len() as _)
                >= 0
        }
    }
}

impl Drop for Annot<'_> {
    fn drop(&mut self) {
        // SAFETY: closes the handle opened in `wrap`, exactly once.
        unsafe { self.bindings.FPDFPage_CloseAnnot(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_heads_point_back_along_the_line() {
        let head = arrow_head((0.0, 0.0), (100.0, 0.0), 2.0);
        assert_eq!(head.len(), 2);
        for wing in &head {
            assert_eq!(wing[0], (100.0, 0.0));
            assert!(wing[1].0 < 100.0, "wings go back towards the start");
        }
        assert!(
            (head[0][1].1 + head[1][1].1).abs() < 1e-3,
            "wings are symmetric"
        );
        assert!(arrow_head((5.0, 5.0), (5.0, 5.0), 2.0).is_empty());
    }

    #[test]
    fn note_icon_uses_its_colour() {
        let r = FS_RECTF {
            left: 100.0,
            bottom: 200.0,
            right: 124.0,
            top: 224.0,
        };
        let ap = note_appearance(&r, Color { r: 255, g: 0, b: 0 });
        assert!(ap.starts_with("q 1.000 0.000 0.000 rg"));
        assert!(ap.ends_with('Q'));
        assert!(
            ap.contains("100.50 200.50 m"),
            "drawn inside the rect: {ap}"
        );
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(new_id(), new_id());
    }
}
