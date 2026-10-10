//! Redaction: permanently removing what is under chosen areas of a page.
//!
//! PDFium has no redaction API, and rewriting a page's drawing with PDFium
//! can change how it looks (its content writer drops some colour spaces), so
//! a redacted page is rebuilt from what PDFium itself shows:
//! 1. The page's content (without annotations) is rendered, and the areas are
//!    painted black in that render. This picture becomes the page's look, so
//!    nothing moves or changes colour.
//! 2. Every page object is removed, except text that doesn't touch an area:
//!    that stays as invisible text over the picture (like a scanned page with
//!    OCR), so it can still be selected, copied and searched.
//! 3. A black box is drawn over each area, and annotations touching an area
//!    are removed.
//! 4. When the document is saved, the page keeps only the resources its new
//!    content uses, and everything nothing refers to any more is left out of
//!    the file (`prune.rs`): the removed text and images can't be dug out.
//!
//! What a redacted page loses: its drawings and images become one picture
//! (at up to 216 DPI), and text inside forms (XObjects) or touching an area
//! can't be selected any more.

#![allow(unsafe_code)]

use std::io::Cursor;

use crate::document::RawBindings;
use pdfium_render::prelude::*;

use crate::{
    Error, ErrorCode, PageRect, Result,
    annotations::{Mapping, image_matrix, overlaps, remove_touching},
};

/// The picture of a redacted page is rendered at up to this many pixels per
/// point (216 DPI), and at most this many pixels in all.
const MAX_SCALE: f32 = 3.0;
const MAX_PIXELS: f32 = 16_000_000.0;
const JPEG_QUALITY: u8 = 90;

const FPDF_PAGEOBJ_TEXT: i32 = 1;
const FPDF_TEXTRENDERMODE_INVISIBLE: i32 = 3;

/// Redacts `areas` (page fractions) of a page. Can't be undone.
pub(crate) fn redact<'a>(
    pdfium: &Pdfium,
    document: &PdfDocument<'a>,
    page: &mut PdfPage<'a>,
    areas: &[PageRect],
) -> Result<()> {
    let areas: Vec<PageRect> = areas.iter().filter_map(clamp).collect();
    if areas.is_empty() {
        return Ok(());
    }
    let map = Mapping::new(page).ok_or_else(|| internal("page has no size"))?;

    // 1. The page as PDFium shows it, with the areas blacked out.
    let picture = render_page(page, &areas)?;

    // 2. Keep only text that doesn't touch an area, made invisible.
    let objects = PageObjects::new(pdfium, page);
    let mut doomed = Vec::new();
    for index in 0..objects.count() {
        let Some((object, kind, bounds)) = objects.get(index) else {
            continue;
        };
        let keep =
            kind == FPDF_PAGEOBJ_TEXT && !areas.iter().any(|a| overlaps(&map.rect(&bounds), a));
        if keep {
            objects.make_invisible(object);
        } else {
            doomed.push(object);
        }
    }
    for object in doomed {
        objects.remove(object);
    }

    // 3. The picture under the text, and crisp black boxes on top.
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    let mut image = PdfPageImageObject::new_from_jpeg_reader(document, Cursor::new(picture))?;
    let whole = PageRect {
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 1.0,
    };
    let m = image_matrix(&map, &whole);
    image.apply_matrix(PdfMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f))?;
    page.objects_mut().add_image_object(image)?;
    PageObjects::new(pdfium, page).move_last_to_bottom();
    for area in &areas {
        let r = map.rect_points(area);
        page.objects_mut().create_path_object_rect(
            PdfRect::new_from_values(r.bottom, r.left, r.top, r.right),
            None,
            None,
            Some(PdfColor::BLACK),
        )?;
    }
    remove_touching(pdfium, page, &areas);
    PageObjects::new(pdfium, page).generate_content();
    Ok(())
}

fn internal(detail: &str) -> Error {
    Error::new(ErrorCode::Internal, detail)
}

/// A rectangle clipped to the page, or `None` if nothing is left.
fn clamp(r: &PageRect) -> Option<PageRect> {
    let c = PageRect {
        left: r.left.clamp(0.0, 1.0),
        top: r.top.clamp(0.0, 1.0),
        right: r.right.clamp(0.0, 1.0),
        bottom: r.bottom.clamp(0.0, 1.0),
    };
    (c.right > c.left && c.bottom > c.top).then_some(c)
}

/// Renders the page's content (no annotations) with the areas painted black, as a JPEG.
fn render_page(page: &PdfPage, areas: &[PageRect]) -> Result<Vec<u8>> {
    let (w, h) = (page.width().value, page.height().value);
    let scale = MAX_SCALE.min((MAX_PIXELS / (w * h).max(1.0)).sqrt());
    let config = PdfRenderConfig::new()
        .scale_page_by_factor(scale)
        .set_format(PdfBitmapFormat::BGRA)
        .set_reverse_byte_order(true)
        .set_clear_color(PdfColor::WHITE)
        .render_annotations(false)
        .render_form_data(false);
    let bitmap = page.render_with_config(&config)?;
    let (bw, bh) = (bitmap.width() as usize, bitmap.height() as usize);
    let rgba = bitmap.as_rgba_bytes();
    let (pixels, _) = rgba.as_chunks::<4>();
    let mut rgb: Vec<u8> = pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    // The page as shown has the same proportions as the fractions, so they map
    // straight to pixels. Rounded outwards, so no sliver of an area survives.
    for area in areas {
        let x0 = (area.left * bw as f32).floor() as usize;
        let x1 = ((area.right * bw as f32).ceil() as usize).min(bw);
        let y0 = (area.top * bh as f32).floor() as usize;
        let y1 = ((area.bottom * bh as f32).ceil() as usize).min(bh);
        for y in y0..y1 {
            rgb[(y * bw + x0) * 3..(y * bw + x1) * 3].fill(0);
        }
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
        .encode(&rgb, bw as u32, bh as u32, image::ExtendedColorType::Rgb8)
        .map_err(|e| internal(&e.to_string()))?;
    Ok(jpeg)
}

/// A page's objects through PDFium's raw API (pdfium-render doesn't let us
/// remove objects by handle or insert them at an index).
struct PageObjects<'a> {
    bindings: &'a dyn PdfiumLibraryBindings,
    page: FPDF_PAGE,
}

impl<'a> PageObjects<'a> {
    fn new(pdfium: &'a Pdfium, page: &'a PdfPage) -> Self {
        Self {
            bindings: pdfium.bindings(),
            page: pdfium.bindings().get_handle_from_page(page),
        }
    }

    fn count(&self) -> i32 {
        // SAFETY: the page handle belongs to a live PdfPage borrowed for 'a;
        // everything runs on the PDFium thread.
        unsafe { self.bindings.FPDFPage_CountObjects(self.page) }
    }

    /// An object, its type and its bounds in PDF points.
    fn get(&self, index: i32) -> Option<(FPDF_PAGEOBJECT, i32, FS_RECTF)> {
        // SAFETY: as in `count`; the object belongs to the page, and the
        // bounds are written into locals that outlive the call.
        unsafe {
            let object = self.bindings.FPDFPage_GetObject(self.page, index);
            if object.is_null() {
                return None;
            }
            let (mut l, mut b, mut r, mut t) = (0.0, 0.0, 0.0, 0.0);
            let has_bounds = self
                .bindings
                .FPDFPageObj_GetBounds(object, &mut l, &mut b, &mut r, &mut t)
                != 0;
            // Objects without bounds can't be shown to be clear of the areas.
            let bounds = if has_bounds {
                FS_RECTF {
                    left: l,
                    top: t,
                    right: r,
                    bottom: b,
                }
            } else {
                FS_RECTF {
                    left: f32::MIN,
                    top: f32::MAX,
                    right: f32::MAX,
                    bottom: f32::MIN,
                }
            };
            Some((object, self.bindings.FPDFPageObj_GetType(object), bounds))
        }
    }

    /// Keeps a text object as invisible text (selectable, not drawn).
    fn make_invisible(&self, object: FPDF_PAGEOBJECT) {
        // SAFETY: `object` is a text object of this page (see `get`).
        unsafe {
            self.bindings
                .FPDFTextObj_SetTextRenderMode(object, FPDF_TEXTRENDERMODE_INVISIBLE)
        };
    }

    /// Takes an object off the page and frees it.
    fn remove(&self, object: FPDF_PAGEOBJECT) {
        // SAFETY: `object` came from this page (see `get`) and is removed once;
        // after RemoveObject the caller owns it, so destroying it is ours to do.
        unsafe {
            if self.bindings.FPDFPage_RemoveObject(self.page, object) != 0 {
                self.bindings.FPDFPageObj_Destroy(object);
            }
        }
    }

    /// Moves the topmost object to the bottom (drawn first, under everything).
    fn move_last_to_bottom(&self) {
        // SAFETY: the object is taken off the page (we own it) and inserted
        // back, so the page owns it again; nothing else holds it meanwhile.
        unsafe {
            let last = self.count() - 1;
            let object = self.bindings.FPDFPage_GetObject(self.page, last);
            if !object.is_null() && self.bindings.FPDFPage_RemoveObject(self.page, object) != 0 {
                self.bindings
                    .FPDFPage_InsertObjectAtIndex(self.page, object, 0);
            }
        }
    }

    /// Writes the page's content stream from its objects.
    fn generate_content(&self) {
        // SAFETY: as in `count`.
        unsafe { self.bindings.FPDFPage_GenerateContent(self.page) };
    }
}
