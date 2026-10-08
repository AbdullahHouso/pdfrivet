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

    /// The transform from PDF points to fractions of the displayed page. Used
    /// where many points are converted (every character of a page), so PDFium is
    /// asked only three times instead of once per point.
    pub fn affine(&self) -> Option<Affine> {
        let px = |x: f32, y: f32| {
            self.page
                .points_to_pixels(PdfPoints::new(x), PdfPoints::new(y), &self.config)
                .ok()
                .map(|(px, py)| (px as f32 / self.width, py as f32 / self.height))
        };
        // Three points far apart keep the whole-pixel rounding negligible.
        const STEP: f32 = 1000.0;
        let (e, f) = px(0.0, 0.0)?;
        let (x1, y1) = px(STEP, 0.0)?;
        let (x2, y2) = px(0.0, STEP)?;
        Some(Affine {
            a: (x1 - e) / STEP,
            b: (y1 - f) / STEP,
            c: (x2 - e) / STEP,
            d: (y2 - f) / STEP,
            e,
            f,
        })
    }
}

/// A 2D affine transform: `(x, y) → (a·x + c·y + e, b·x + d·y + f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Affine {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Affine {
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// The transform that undoes this one (`None` if it squashes the page flat).
    pub fn invert(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < f32::EPSILON {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }

    /// A PDF rectangle (points) → fractions of the page, top-left origin.
    pub fn rect(&self, left: f32, bottom: f32, right: f32, top: f32) -> FractionRect {
        let (x1, y1) = self.apply(left, top);
        let (x2, y2) = self.apply(right, bottom);
        FractionRect {
            left: x1.min(x2),
            top: y1.min(y2),
            right: x1.max(x2),
            bottom: y1.max(y2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_undoes_the_transform() {
        // A page rotated a quarter turn with a shifted origin.
        let t = Affine {
            a: 0.0,
            b: 0.002,
            c: 0.0015,
            d: 0.0,
            e: -0.1,
            f: 0.25,
        };
        let back = t.invert().unwrap();
        for (x, y) in [(0.0, 0.0), (100.0, 250.0), (612.0, 792.0)] {
            let (fx, fy) = t.apply(x, y);
            let (bx, by) = back.apply(fx, fy);
            assert!(
                (bx - x).abs() < 1e-2 && (by - y).abs() < 1e-2,
                "{x},{y} → {bx},{by}"
            );
        }
    }
}
