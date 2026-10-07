//! Printing: printer discovery, print settings, and placing pages on paper.
//!
//! The same layout maths ([`place_page`]) drives the preview in the app and the
//! real print job, so what you see is what prints.
//!
//! - Windows: PDFium draws each page straight onto the printer (`windows.rs`),
//!   the way Chrome prints PDFs: sharp vector output, real driver settings.
//! - Linux and macOS: the chosen pages are sent as a PDF to CUPS with `lp`
//!   (`cups.rs`), which applies the settings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[cfg(unix)]
mod cups;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(crate) use cups::print_document;
#[cfg(windows)]
pub(crate) use windows::print_document;
#[cfg(windows)]
pub use windows::printer_properties;

use crate::Result;

/// A paper size a printer offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct Paper {
    /// The printer's own identifier (a Windows paper number or a CUPS keyword).
    pub id: String,
    pub name: String,
    pub width_mm: f32,
    pub height_mm: f32,
}

/// A printer and what it can do.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct PrinterInfo {
    pub name: String,
    pub is_default: bool,
    pub papers: Vec<Paper>,
    /// Id of the paper the printer uses by default.
    pub default_paper: Option<String>,
    pub duplex: bool,
    pub color: bool,
    /// The printer's own settings window is available ("Printer properties…").
    pub has_properties: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum Scaling {
    /// Grow or shrink each page to fill the printable area.
    Fit,
    /// Print at the page's real size.
    Actual,
    /// Real size, but shrink pages that don't fit.
    Shrink,
    Custom {
        percent: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Orientation {
    /// Each sheet turns to match its page (landscape pages print landscape).
    Auto,
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Duplex {
    OneSided,
    /// Two-sided, flipped on the long edge (normal for portrait).
    LongEdge,
    /// Two-sided, flipped on the short edge (for landscape / calendars).
    ShortEdge,
}

/// Everything chosen in the print dialog.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct PrintSettings {
    pub printer: String,
    /// 0-based pages, in the order they should print.
    pub pages: Vec<u32>,
    pub copies: u32,
    pub collate: bool,
    pub scaling: Scaling,
    pub orientation: Orientation,
    pub duplex: Duplex,
    pub grayscale: bool,
    /// Paper id from [`PrinterInfo::papers`]; `None` uses the printer's default.
    pub paper: Option<String>,
    /// Document title shown in the printer queue.
    pub title: String,
}

/// Where one page goes on its sheet of paper. All values are in points
/// (1/72 inch), measured from the sheet's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct Placement {
    pub landscape: bool,
    pub sheet_width: f32,
    pub sheet_height: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}

pub const MM_TO_PT: f32 = 72.0 / 25.4;

/// Unprintable border most printers have, used where the real one is unknown
/// (the preview, and CUPS which handles margins itself).
pub const DEFAULT_MARGIN_PT: f32 = 12.0;

/// Places a page (`page_width` × `page_height` points) on paper
/// (`paper_width` × `paper_height` points, portrait) with `margin` points of
/// unprintable border on every side.
pub fn place_page(
    page_width: f32,
    page_height: f32,
    paper_width: f32,
    paper_height: f32,
    margin: f32,
    scaling: Scaling,
    orientation: Orientation,
) -> Placement {
    let (short, long) = (paper_width.min(paper_height), paper_width.max(paper_height));
    let landscape = match orientation {
        Orientation::Auto => page_width > page_height,
        Orientation::Portrait => false,
        Orientation::Landscape => true,
    };
    let (sheet_width, sheet_height) = if landscape {
        (long, short)
    } else {
        (short, long)
    };
    let area_width = (sheet_width - 2.0 * margin).max(1.0);
    let area_height = (sheet_height - 2.0 * margin).max(1.0);
    let fit = (area_width / page_width.max(1.0)).min(area_height / page_height.max(1.0));
    let scale = match scaling {
        Scaling::Fit => fit,
        Scaling::Actual => 1.0,
        Scaling::Shrink => fit.min(1.0),
        Scaling::Custom { percent } => (percent / 100.0).clamp(0.01, 10.0),
    };
    let (width, height) = (page_width * scale, page_height * scale);
    Placement {
        landscape,
        sheet_width,
        sheet_height,
        // Centred on the sheet (a page bigger than the sheet is cut evenly on all sides).
        x: (sheet_width - width) / 2.0,
        y: (sheet_height - height) / 2.0,
        width,
        height,
        scale,
    }
}

/// The printers installed on this computer (empty if none or not supported).
pub fn list_printers() -> Result<Vec<PrinterInfo>> {
    #[cfg(windows)]
    return windows::list_printers();
    #[cfg(unix)]
    return cups::list_printers();
    #[cfg(not(any(windows, unix)))]
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: (f32, f32) = (595.0, 842.0);

    #[test]
    fn actual_size_is_centred() {
        let p = place_page(
            500.0,
            700.0,
            A4.0,
            A4.1,
            12.0,
            Scaling::Actual,
            Orientation::Auto,
        );
        assert!(!p.landscape);
        assert_eq!((p.width, p.height), (500.0, 700.0));
        assert!((p.x - 47.5).abs() < 0.01 && (p.y - 71.0).abs() < 0.01);
    }

    #[test]
    fn fit_fills_the_printable_area() {
        let p = place_page(
            297.5,
            421.0,
            A4.0,
            A4.1,
            0.0,
            Scaling::Fit,
            Orientation::Portrait,
        );
        assert!((p.scale - 2.0).abs() < 0.001);
    }

    #[test]
    fn shrink_only_shrinks() {
        let small = place_page(
            300.0,
            400.0,
            A4.0,
            A4.1,
            12.0,
            Scaling::Shrink,
            Orientation::Auto,
        );
        assert_eq!(small.scale, 1.0);
        let big = place_page(
            1190.0,
            1684.0,
            A4.0,
            A4.1,
            12.0,
            Scaling::Shrink,
            Orientation::Auto,
        );
        assert!(big.scale < 0.5 && big.height <= A4.1 - 24.0 + 0.01);
    }

    #[test]
    fn auto_orientation_turns_the_sheet_for_landscape_pages() {
        let p = place_page(
            842.0,
            595.0,
            A4.0,
            A4.1,
            12.0,
            Scaling::Fit,
            Orientation::Auto,
        );
        assert!(p.landscape);
        assert_eq!((p.sheet_width, p.sheet_height), (842.0, 595.0));
        let forced = place_page(
            842.0,
            595.0,
            A4.0,
            A4.1,
            12.0,
            Scaling::Fit,
            Orientation::Portrait,
        );
        assert!(!forced.landscape && forced.scale < 1.0);
    }
}
