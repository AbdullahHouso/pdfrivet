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

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    Error, ErrorCode, Result,
    geometry::{Affine, PageGeometry},
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
}

// Values from PDFium's public headers (fpdf_annot.h), which pdfium-render
// doesn't re-export.
const FPDF_ANNOT_TEXT: u32 = 1;
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
const FPDF_ANNOT_FLAG_PRINT: i32 = 4;
const FPDF_ANNOT_FLAG_LOCKED: i32 = 128;
const FPDF_ANNOT_APPEARANCEMODE_NORMAL: i32 = 0;
const COLOR_STROKE: FPDFANNOT_COLORTYPE = 0;
const COLOR_FILL: FPDFANNOT_COLORTYPE = 1;
const FPDF_OBJECT_ARRAY: FPDF_OBJECT_TYPE = 5;
const FPDF_PAGEOBJ_PATH: i32 = 2;

/// Private key marking an Ink annotation that PDFRivet drew as a line or arrow.
const SHAPE_KEY: &str = "PDFRivetShape";

/// Reads the annotations of a page, in the order they are drawn. Links, form
/// fields and pop-up windows are left out (they're handled elsewhere).
pub(crate) fn read(pdfium: &Pdfium, page: &PdfPage) -> Vec<Annotation> {
    let Some(map) = Mapping::new(page) else {
        return Vec::new();
    };
    let annots = PageAnnots::new(pdfium, page);
    (0..annots.count())
        .filter_map(|index| {
            let annot = annots.get(index)?;
            annot_to_model(&annot, index, &map)
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
        AnnotationKind::Stamp | AnnotationKind::Other { .. } => {
            return Err(internal("this kind of annotation can't be added"));
        }
    };
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
fn image_matrix(map: &Mapping, rect: &PageRect) -> FS_MATRIX {
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
pub(crate) fn update(pdfium: &Pdfium, page: &PdfPage, annotation: &Annotation) -> Result<String> {
    let map = Mapping::new(page).ok_or_else(|| internal("page has no size"))?;
    let annots = PageAnnots::new(pdfium, page);
    let (index, annot) = annots
        .find(&annotation.id)
        .ok_or_else(|| Error::new(ErrorCode::AnnotationNotFound, annotation.id.clone()))?;
    let current = annot_to_model(&annot, index, &map)
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

/// Removes an annotation.
pub(crate) fn delete(pdfium: &Pdfium, page: &PdfPage, id: &str) -> Result<()> {
    let annots = PageAnnots::new(pdfium, page);
    let (index, annot) = annots
        .find(id)
        .ok_or_else(|| Error::new(ErrorCode::AnnotationNotFound, id.to_owned()))?;
    drop(annot);
    if annots.remove(index) {
        Ok(())
    } else {
        Err(internal("PDFium couldn't remove the annotation"))
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
struct Mapping {
    to_fraction: Affine,
    to_points: Affine,
}

impl Mapping {
    fn new(page: &PdfPage) -> Option<Self> {
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

    fn rect(&self, r: &FS_RECTF) -> PageRect {
        let f = self.to_fraction.rect(r.left, r.bottom, r.right, r.top);
        PageRect {
            left: f.left,
            top: f.top,
            right: f.right,
            bottom: f.bottom,
        }
    }

    /// A page rectangle in points: (left, bottom, right, top).
    fn rect_points(&self, r: &PageRect) -> FS_RECTF {
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

fn annot_to_model(annot: &Annot, index: i32, map: &Mapping) -> Option<Annotation> {
    let subtype = annot.subtype() as u32;
    let mut editable = true;
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
        color: annot
            .main_color(matches!(subtype, FPDF_ANNOT_HIGHLIGHT | FPDF_ANNOT_TEXT))
            .unwrap_or(Color { r: 0, g: 0, b: 0 }),
        opacity: annot.number("CA").unwrap_or(1.0).clamp(0.0, 1.0),
        width: annot.border_width().unwrap_or(1.0),
        contents: annot.string("Contents").unwrap_or_default(),
        author: annot.string("T").unwrap_or_default(),
        modified: annot
            .string("M")
            .and_then(|m| crate::metadata::pdf_date_to_iso(&m)),
        // Locked annotations stay as they are.
        editable: editable && annot.flags() & FPDF_ANNOT_FLAG_LOCKED == 0,
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
            let strokes: Vec<Vec<(f32, f32)>> = strokes
                .iter()
                .map(|s| s.iter().map(|&p| map.points(p)).collect())
                .collect();
            write_strokes(annot, &strokes, pad)?;
            annot.set_string(SHAPE_KEY, "");
        }
        AnnotationKind::Line { from, to, arrow } => {
            let (a, b) = (map.points(*from), map.points(*to));
            let mut strokes = vec![vec![a, b]];
            if *arrow {
                strokes.extend(arrow_head(a, b, annotation.width));
            }
            write_strokes(annot, &strokes, pad)?;
            annot.set_string(SHAPE_KEY, if *arrow { "arrow" } else { "line" });
        }
        AnnotationKind::Square { .. } | AnnotationKind::Circle { .. } | AnnotationKind::Note => {
            annot.set_rect(&map.rect_points(&annotation.rect));
        }
        AnnotationKind::Stamp | AnnotationKind::Other { .. } => {}
    }
    Ok(())
}

fn write_strokes(annot: &Annot, strokes: &[Vec<(f32, f32)>], pad: f32) -> Result<()> {
    annot.remove_ink();
    for stroke in strokes {
        if stroke.is_empty() || !annot.add_ink_stroke(stroke) {
            return Err(internal("PDFium refused an ink stroke"));
        }
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
    if annotation.kind == AnnotationKind::Note {
        // PDFium always draws notes yellow; this draws them in their colour.
        annot.set_appearance(&note_appearance(
            &map.rect_points(&annotation.rect),
            annotation.color,
        ));
    }
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
            page: page.page_handle(),
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

    fn string(&self, key: &str) -> Option<String> {
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
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
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
