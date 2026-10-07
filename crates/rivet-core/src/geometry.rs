//! Converting between PDF page coordinates and "fractions of the page".
//!
//! The UI positions links and form fields as fractions (0..1) of the page,
//! measured from the top-left corner of the page *as PDFium displays it*.
//! PDF coordinates start at the bottom-left, may have a shifted origin, and
//! pages may carry their own rotation; PDFium's conversion functions handle
//! all of that, so we let them do the work.

use pdfium_render::prelude::*;

/// A reference "screen" size used for conversions. Large, so rounding to whole
/// pixels is negligible.
const REFERENCE_WIDTH: i32 = 10_000;

/// A rectangle as fractions of the page, top-left origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FractionRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

pub(crate) struct PageGeometry<'p, 'a> {
    page: &'p PdfPage<'a>,
    config: PdfRenderConfig,
    width: f32,
    height: f32,
}

impl<'p, 'a> PageGeometry<'p, 'a> {
    pub fn new(page: &'p PdfPage<'a>) -> Option<Self> {
        let (w, h) = (page.width().value, page.height().value);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let width = REFERENCE_WIDTH;
        let height = ((REFERENCE_WIDTH as f32) * h / w).round().max(1.0) as i32;
        Some(Self {
            page,
            config: PdfRenderConfig::new().set_target_size(width, height),
            width: width as f32,
            height: height as f32,
        })
    }

    /// Page rectangle (PDF points) → fractions of the displayed page.
    pub fn to_fraction(&self, rect: &PdfRect) -> Option<FractionRect> {
        let (x1, y1) = self
            .page
            .points_to_pixels(rect.left(), rect.top(), &self.config)
            .ok()?;
        let (x2, y2) = self
            .page
            .points_to_pixels(rect.right(), rect.bottom(), &self.config)
            .ok()?;
        let fx = |x: i32| (x as f32 / self.width).clamp(0.0, 1.0);
        let fy = |y: i32| (y as f32 / self.height).clamp(0.0, 1.0);
        Some(FractionRect {
            left: fx(x1.min(x2)),
            right: fx(x1.max(x2)),
            top: fy(y1.min(y2)),
            bottom: fy(y1.max(y2)),
        })
    }
}
