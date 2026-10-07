use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    Error, ErrorCode, FieldChange, FormField, OutlineItem, PageLink, Result, forms, links, outline,
};

/// Files up to this size are loaded into memory when opened (see [`Pdf::open`]).
pub const IN_MEMORY_LIMIT: u64 = 512 * 1024 * 1024;

/// The loaded PDFium library. Create one per process with [`Pdf::load`].
///
/// PDFium is not thread-safe: use a `Pdf` and its documents from one thread
/// at a time. The [`crate::Engine`] does this for you.
pub struct Pdf {
    // Documents borrow the library, so we keep it alive for the whole program.
    // Leaking it once is the simplest way to give it a `'static` lifetime.
    pdfium: &'static Pdfium,
}

impl Pdf {
    /// Loads the PDFium shared library from `lib_dir` (the folder that contains
    /// `pdfium.dll`, `libpdfium.so` or `libpdfium.dylib`).
    ///
    /// PDFium can only be loaded once per process; later calls reuse it.
    pub fn load(lib_dir: &Path) -> Result<Self> {
        static LOADED: OnceLock<&'static Pdfium> = OnceLock::new();
        if let Some(pdfium) = LOADED.get() {
            return Ok(Self { pdfium });
        }
        let path = Pdfium::pdfium_platform_library_name_at_path(lib_dir);
        let bindings = Pdfium::bind_to_library(&path).map_err(|e| {
            Error::new(
                ErrorCode::LibraryNotFound,
                format!("{}: {e:?}", path.display()),
            )
        })?;
        let pdfium = *LOADED.get_or_init(|| Box::leak(Box::new(Pdfium::new(bindings))));
        Ok(Self { pdfium })
    }

    /// Opens a PDF file.
    ///
    /// Files up to [`IN_MEMORY_LIMIT`] are read into memory, so the file on disk
    /// isn't kept open: it can be saved over (Windows refuses to replace open
    /// files), renamed or moved while Rivet shows it. Bigger files are read
    /// lazily, so only the parts actually needed are loaded.
    pub fn open(&self, path: &Path, password: Option<&str>) -> Result<Document> {
        if !path.is_file() {
            return Err(Error::new(
                ErrorCode::FileNotFound,
                path.display().to_string(),
            ));
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);
        let loaded = if size <= IN_MEMORY_LIMIT {
            let bytes =
                std::fs::read(path).map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
            self.pdfium.load_pdf_from_byte_vec(bytes, password)
        } else {
            self.pdfium.load_pdf_from_file(path, password)
        };
        let inner = loaded.map_err(|e| {
            let err = Error::from(e);
            // PDFium reports the same error for "no password" and "wrong password".
            if err.code == ErrorCode::PasswordRequired && password.is_some() {
                Error::new(ErrorCode::WrongPassword, err.detail)
            } else {
                err
            }
        })?;
        Ok(Document {
            pdfium: self.pdfium,
            inner,
        })
    }

    /// Writes a simple PDF with `pages` numbered A4 pages. Used to create large
    /// files for performance tests (`rivet-cli make-test-pdf`).
    pub fn write_test_document(&self, path: &Path, pages: u32) -> Result<()> {
        // Adding text gets slower as a document grows, so pages are built in
        // small chunks and then joined.
        const CHUNK: u32 = 100;
        let mut doc = self.pdfium.create_new_pdf()?;
        let mut first = 1;
        while first <= pages {
            let last = (first + CHUNK - 1).min(pages);
            let mut chunk = self.pdfium.create_new_pdf()?;
            let font = chunk.fonts_mut().helvetica();
            for i in first..=last {
                let mut page = chunk
                    .pages_mut()
                    .create_page_at_end(PdfPagePaperSize::a4())?;
                page.objects_mut().create_text_object(
                    PdfPoints::new(72.0),
                    PdfPoints::new(760.0),
                    format!("Rivet test page {i} of {pages}"),
                    font,
                    PdfPoints::new(24.0),
                )?;
            }
            doc.pages_mut().append(&chunk)?;
            first = last + 1;
        }
        doc.save_to_file(path)?;
        Ok(())
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
    /// The text reads right to left (e.g. Arabic), so two-page spreads start on the right.
    pub rtl: bool,
}

/// Clockwise view rotation. Only affects rendering; the file is never changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Rotation {
    #[default]
    None,
    Cw90,
    Cw180,
    Cw270,
}

impl Rotation {
    /// Parses 0/90/180/270 degrees (other values are rounded to the nearest quarter turn).
    pub fn from_degrees(degrees: i32) -> Self {
        match (degrees.rem_euclid(360) + 45) / 90 % 4 {
            1 => Self::Cw90,
            2 => Self::Cw180,
            3 => Self::Cw270,
            _ => Self::None,
        }
    }

    fn to_pdfium(self) -> PdfPageRenderRotation {
        match self {
            Self::None => PdfPageRenderRotation::None,
            Self::Cw90 => PdfPageRenderRotation::Degrees90,
            Self::Cw180 => PdfPageRenderRotation::Degrees180,
            Self::Cw270 => PdfPageRenderRotation::Degrees270,
        }
    }
}

/// A rendered page as tightly packed RGBA pixels (4 bytes per pixel, row by row).
/// The pixels are shared (`Arc`), so caching and sending them doesn't copy them.
#[derive(Clone)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

/// One open PDF document.
pub struct Document {
    pdfium: &'static Pdfium,
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
            rtl: crate::direction::detect(&self.inner),
        })
    }

    /// The document's table of contents (bookmarks). Empty if it has none.
    pub fn outline(&self) -> Vec<OutlineItem> {
        outline::read(&self.inner)
    }

    /// The clickable links on a page.
    pub fn links(&self, index: u32) -> Result<Vec<PageLink>> {
        self.check_index(index)?;
        let page = self.inner.pages().get(index as PdfPageIndex)?;
        Ok(links::read(&page))
    }

    /// The interactive form fields on a page (empty if it has none).
    pub fn form_fields(&self, index: u32) -> Result<Vec<FormField>> {
        self.check_index(index)?;
        let page = self.inner.pages().get(index as PdfPageIndex)?;
        Ok(forms::read(&page))
    }

    /// Changes a form field (see [`FieldChange`]).
    pub fn change_field(&self, page: u32, field: u32, change: &FieldChange) -> Result<()> {
        self.check_index(page)?;
        let pdf_page = self.inner.pages().get(page as PdfPageIndex)?;
        forms::apply(self.pdfium, &self.inner, &pdf_page, field, change)
    }

    /// Saves the document (including filled-in forms) to `path`.
    ///
    /// The new file is written next to the target first and then moved into
    /// place, so a crash or full disk never leaves a half-written PDF behind.
    pub fn save(&self, path: &Path) -> Result<()> {
        let failed = |e: &dyn std::fmt::Display| Error::new(ErrorCode::SaveFailed, e.to_string());
        let bytes = self
            .inner
            .save_to_bytes()
            .map_err(|e| failed(&format!("{e:?}")))?;
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let temp = dir.join(format!(".{name}.rivet-saving"));
        let write = || -> std::io::Result<()> {
            use std::io::Write;
            let mut file = std::fs::File::create(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temp, path)
        };
        write().map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            failed(&e)
        })
    }

    /// Prints pages with the given settings. `printer_settings` are driver
    /// settings from "Printer properties…" (Windows only; ignored elsewhere).
    pub fn print(
        &self,
        settings: &crate::print::PrintSettings,
        printer_settings: Option<&[u8]>,
    ) -> Result<()> {
        if let Some(&bad) = settings.pages.iter().find(|&&p| p >= self.page_count()) {
            return Err(Error::new(ErrorCode::PageOutOfRange, format!("page {bad}")));
        }
        crate::print::print_document(self.pdfium, &self.inner, settings, printer_settings)
    }

    /// Renders a page. `scale` 1.0 means 1 pixel per PDF point (72 DPI);
    /// the UI passes zoom × device pixel ratio.
    pub fn render_page(&self, index: u32, scale: f32, rotation: Rotation) -> Result<RenderedPage> {
        self.check_index(index)?;
        let page = self.inner.pages().get(index as PdfPageIndex)?;
        let config = PdfRenderConfig::new()
            .scale_page_by_factor(scale.clamp(0.05, 16.0))
            .rotate(rotation.to_pdfium(), true)
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
            rgba: bitmap.as_rgba_bytes().into(),
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
