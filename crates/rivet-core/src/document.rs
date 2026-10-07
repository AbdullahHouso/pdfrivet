use std::path::Path;

use pdfium_render::prelude::*;
use serde::Serialize;
use ts_rs::TS;

use crate::{Error, ErrorCode, Result};

/// The loaded PDFium library. Create one per process with [`Pdf::load`].
pub struct Pdf {
    // Documents borrow the library, so we keep it alive for the whole program.
    // Leaking it once is the simplest way to give it a `'static` lifetime.
    pdfium: &'static Pdfium,
}

impl Pdf {
    /// Loads the PDFium shared library from `lib_dir` (the folder that contains
    /// `pdfium.dll`, `libpdfium.so` or `libpdfium.dylib`).
    pub fn load(lib_dir: &Path) -> Result<Self> {
        let path = Pdfium::pdfium_platform_library_name_at_path(lib_dir);
        let bindings = Pdfium::bind_to_library(&path).map_err(|e| {
            Error::new(
                ErrorCode::LibraryNotFound,
                format!("{}: {e:?}", path.display()),
            )
        })?;
        Ok(Self {
            pdfium: Box::leak(Box::new(Pdfium::new(bindings))),
        })
    }

    /// Opens a PDF file. PDFium reads it lazily, so big files open quickly
    /// and only the parts actually needed are loaded into memory.
    pub fn open(&self, path: &Path, password: Option<&str>) -> Result<Document> {
        if !path.is_file() {
            return Err(Error::new(
                ErrorCode::FileNotFound,
                path.display().to_string(),
            ));
        }
        let inner = self.pdfium.load_pdf_from_file(path, password)?;
        Ok(Document { inner })
    }
}

/// Page size in PDF points (1 pt = 1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// Basic facts about an open document, sent to the UI after opening.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct DocInfo {
    pub page_count: u32,
    pub title: Option<String>,
    pub author: Option<String>,
    /// Size of every page, so the viewer can lay out pages before rendering them.
    pub page_sizes: Vec<PageSize>,
}

/// A rendered page as tightly packed RGBA pixels (4 bytes per pixel, row by row).
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One open PDF document.
pub struct Document {
    inner: PdfDocument<'static>,
}

impl Document {
    pub fn page_count(&self) -> u32 {
        self.inner.pages().len().max(0) as u32
    }

    pub fn page_size(&self, index: u32) -> Result<PageSize> {
        self.check_index(index)?;
        let rect = self.inner.pages().page_size(index as PdfPageIndex)?;
        Ok(PageSize {
            width: rect.width().value,
            height: rect.height().value,
        })
    }

    pub fn info(&self) -> Result<DocInfo> {
        let meta = |tag| {
            self.inner
                .metadata()
                .get(tag)
                .map(|t| t.value().trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let page_sizes = (0..self.page_count())
            .map(|i| self.page_size(i))
            .collect::<Result<_>>()?;
        Ok(DocInfo {
            page_count: self.page_count(),
            title: meta(PdfDocumentMetadataTagType::Title),
            author: meta(PdfDocumentMetadataTagType::Author),
            page_sizes,
        })
    }

    /// Renders a page. `scale` 1.0 means 1 pixel per PDF point (72 DPI);
    /// the UI passes zoom × device pixel ratio.
    pub fn render_page(&self, index: u32, scale: f32) -> Result<RenderedPage> {
        self.check_index(index)?;
        let page = self.inner.pages().get(index as PdfPageIndex)?;
        let config = PdfRenderConfig::new()
            .scale_page_by_factor(scale.clamp(0.05, 16.0))
            .set_format(PdfBitmapFormat::BGRA)
            // Ask PDFium to write RGBA directly, so no conversion pass is needed.
            .set_reverse_byte_order(true)
            .set_clear_color(PdfColor::WHITE)
            .render_annotations(true)
            .render_form_data(true);
        let bitmap = page.render_with_config(&config)?;
        Ok(RenderedPage {
            width: bitmap.width() as u32,
            height: bitmap.height() as u32,
            rgba: bitmap.as_rgba_bytes(),
        })
    }

    fn check_index(&self, index: u32) -> Result<()> {
        if index < self.page_count() {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::PageOutOfRange,
                format!("page {index} of {}", self.page_count()),
            ))
        }
    }
}

/// Name of the prebuilt PDFium platform folder for this OS/CPU, matching the
/// folders created by `cargo xtask fetch-pdfium` (e.g. `win-x64`).
pub fn pdfium_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "aarch64") => "win-arm64",
        ("windows", _) => "win-x64",
        ("macos", _) => "mac-univ",
        ("linux", "aarch64") => "linux-arm64",
        _ => "linux-x64",
    }
}
