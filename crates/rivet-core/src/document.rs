use std::{
    cell::Cell,
    io::Cursor,
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

/// PDFium keeps fonts, images and other data of every page it has shown until
/// the document is closed, so memory grows as you read (about 20 MB per 100
/// pages in image-heavy files). After this many pages the document is quietly
/// opened again, which lets go of all that (see [`Document::release_memory`]).
const RELEASE_AFTER_PAGES: u32 = 100;

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
        let source = if size <= IN_MEMORY_LIMIT {
            let bytes =
                std::fs::read(path).map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
            Source::Memory(Arc::new(bytes))
        } else {
            Source::File
        };
        let inner = load(self.pdfium, &source, path, password).map_err(|err| {
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
            path: path.to_path_buf(),
            new_metadata: None,
            source,
            password: password.map(str::to_owned),
            pages_loaded: Cell::new(0),
            unsaved_changes: Cell::new(false),
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

/// Where a document's bytes come from, so it can be opened again.
enum Source {
    /// Read into memory once; every reopening shares these bytes.
    Memory(Arc<Vec<u8>>),
    /// Too big to keep in memory; read from `Document::path` as needed.
    File,
}

/// Lets PDFium read shared in-memory bytes without copying the whole file.
struct SharedBytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

fn load(
    pdfium: &'static Pdfium,
    source: &Source,
    path: &Path,
    password: Option<&str>,
) -> Result<PdfDocument<'static>> {
    let loaded = match source {
        Source::Memory(bytes) => {
            pdfium.load_pdf_from_reader(Cursor::new(SharedBytes(bytes.clone())), password)
        }
        Source::File => pdfium.load_pdf_from_file(path, password),
    };
    Ok(loaded?)
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
    /// The document allows copying its text.
    pub can_copy: bool,
    /// The document allows adding and changing annotations.
    pub can_annotate: bool,
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
    path: std::path::PathBuf,
    /// Title/author/… changed by the user, written into the file on save.
    new_metadata: Option<crate::metadata::Metadata>,
    source: Source,
    password: Option<String>,
    /// Pages PDFium has loaded since the document was (re)opened.
    pages_loaded: Cell<u32>,
    /// Filled-in form fields and annotations live only inside PDFium until
    /// they're saved, so the document must not be reopened while there are any.
    unsaved_changes: Cell<bool>,
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
            title: match &self.new_metadata {
                Some(m) => Some(m.title.trim().to_owned()).filter(|t| !t.is_empty()),
                None => meta(PdfDocumentMetadataTagType::Title),
            },
            author: meta(PdfDocumentMetadataTagType::Author),
            page_sizes,
            rtl: crate::direction::detect(&self.inner),
            can_copy: self.can_copy(),
            can_annotate: self.can_annotate(),
        })
    }

    fn can_annotate(&self) -> bool {
        self.inner
            .permissions()
            .can_add_or_modify_text_annotations()
            .unwrap_or(true)
    }

    /// The annotations on a page (highlights, drawings, notes…), in drawing order.
    pub fn annotations(&self, index: u32) -> Result<Vec<crate::Annotation>> {
        let page = self.load_page(index)?;
        Ok(crate::annotations::read(self.pdfium, &page))
    }

    /// Adds an annotation; returns its id.
    pub fn add_annotation(&self, index: u32, annotation: &crate::Annotation) -> Result<String> {
        self.check_can_annotate()?;
        let page = self.load_page(index)?;
        let id = crate::annotations::add(self.pdfium, &page, annotation)?;
        self.unsaved_changes.set(true);
        Ok(id)
    }

    /// Changes an annotation (found by its id); returns its id, which is new
    /// for an annotation that had none.
    pub fn update_annotation(&self, index: u32, annotation: &crate::Annotation) -> Result<String> {
        self.check_can_annotate()?;
        let page = self.load_page(index)?;
        let id = crate::annotations::update(self.pdfium, &page, annotation)?;
        self.unsaved_changes.set(true);
        Ok(id)
    }

    pub fn delete_annotation(&self, index: u32, id: &str) -> Result<()> {
        self.check_can_annotate()?;
        let page = self.load_page(index)?;
        crate::annotations::delete(self.pdfium, &page, id)?;
        self.unsaved_changes.set(true);
        Ok(())
    }

    fn check_can_annotate(&self) -> Result<()> {
        if self.can_annotate() {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::AnnotateNotAllowed,
                "document permissions",
            ))
        }
    }

    fn can_copy(&self) -> bool {
        self.inner
            .permissions()
            .can_extract_text_and_graphics()
            .unwrap_or(true)
    }

    /// The document's table of contents (bookmarks). Empty if it has none.
    pub fn outline(&self) -> Vec<OutlineItem> {
        outline::read(&self.inner)
    }

    /// The clickable links on a page.
    pub fn links(&self, index: u32) -> Result<Vec<PageLink>> {
        let page = self.load_page(index)?;
        Ok(links::read(&page))
    }

    /// Every character of a page with its box (for selecting text).
    pub fn page_text(&self, index: u32) -> Result<crate::PageText> {
        let page = self.load_page(index)?;
        Ok(crate::text::read(self.pdfium, &page))
    }

    /// The text of a range of characters, possibly across pages (for copying).
    /// Pages are separated by a line break.
    /// Fails with [`ErrorCode::CopyNotAllowed`] if the document forbids copying.
    pub fn text(&self, range: crate::TextRange) -> Result<String> {
        if !self.can_copy() {
            return Err(Error::new(
                ErrorCode::CopyNotAllowed,
                "document permissions",
            ));
        }
        let last = range.end_page.min(self.page_count().saturating_sub(1));
        let mut out = String::new();
        for index in range.start_page..=last {
            let page = self.load_page(index)?;
            let start = if index == range.start_page {
                range.start
            } else {
                0
            };
            let end = if index == range.end_page {
                range.end
            } else {
                u32::MAX
            };
            let text = crate::text::text_of(self.pdfium, &page, start, end);
            if !out.is_empty() && !out.ends_with('\n') && !text.is_empty() {
                out.push('\n');
            }
            out.push_str(&text);
        }
        Ok(out)
    }

    /// Searches pages from `first` on, a batch at a time (see [`crate::SearchBatch`]).
    pub fn search(&self, query: &crate::SearchQuery, first: u32) -> crate::SearchBatch {
        crate::search::search_pages(self.page_count(), first, query, |index| {
            self.load_page(index)
                .map(|page| crate::search::page_chars(self.pdfium, &page))
                .unwrap_or_default()
        })
    }

    /// Everything shown in the Document properties dialog.
    pub fn properties(&self) -> crate::metadata::DocProperties {
        use crate::metadata::{DocProperties, Metadata, Permissions, pdf_date_to_iso};
        let tag = |t| {
            self.inner
                .metadata()
                .get(t)
                .map(|m| m.value().trim().to_owned())
                .unwrap_or_default()
        };
        let stored = Metadata {
            title: tag(PdfDocumentMetadataTagType::Title),
            author: tag(PdfDocumentMetadataTagType::Author),
            subject: tag(PdfDocumentMetadataTagType::Subject),
            keywords: tag(PdfDocumentMetadataTagType::Keywords),
        };
        let permissions = self.inner.permissions();
        // Newer encryption (AES-256, revisions 5–6) isn't in pdfium-render's list and
        // comes back as an error, so anything but a clear "unprotected" counts as encrypted.
        let encrypted = !matches!(
            permissions.security_handler_revision(),
            Ok(PdfSecurityHandlerRevision::Unprotected)
        );
        let version = format!("{:?}", self.inner.version());
        DocProperties {
            metadata: self.new_metadata.clone().unwrap_or(stored),
            creator: tag(PdfDocumentMetadataTagType::Creator),
            producer: tag(PdfDocumentMetadataTagType::Producer),
            created: pdf_date_to_iso(&tag(PdfDocumentMetadataTagType::CreationDate)),
            modified: pdf_date_to_iso(&tag(PdfDocumentMetadataTagType::ModificationDate)),
            pdf_version: version.trim_start_matches("Pdf").replace('_', "."),
            page_count: self.page_count(),
            file_name: self
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            folder: self
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            file_size: std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0),
            tagged: self.inner.catalog().is_tagged(),
            encrypted,
            permissions: Permissions {
                print: permissions.can_print_high_quality().unwrap_or(true)
                    || permissions.can_print_only_low_quality().unwrap_or(false),
                copy: self.can_copy(),
                modify: permissions.can_modify_document_content().unwrap_or(true),
                fill_forms: permissions
                    .can_fill_existing_interactive_form_fields()
                    .unwrap_or(true),
                annotate: self.can_annotate(),
            },
            can_edit_metadata: !encrypted,
        }
    }

    /// Changes the title, author, subject and keywords (written on the next save).
    pub fn set_metadata(&mut self, meta: crate::metadata::Metadata) -> Result<()> {
        if !self.properties().can_edit_metadata {
            return Err(Error::new(
                ErrorCode::ReadOnlyField,
                "password-protected PDF",
            ));
        }
        self.new_metadata = Some(meta);
        Ok(())
    }

    /// The interactive form fields on a page (empty if it has none).
    pub fn form_fields(&self, index: u32) -> Result<Vec<FormField>> {
        let page = self.load_page(index)?;
        Ok(forms::read(&page))
    }

    /// Changes a form field (see [`FieldChange`]).
    pub fn change_field(&self, page: u32, field: u32, change: &FieldChange) -> Result<()> {
        let pdf_page = self.load_page(page)?;
        forms::apply(self.pdfium, &self.inner, &pdf_page, field, change)?;
        self.unsaved_changes.set(true);
        Ok(())
    }

    /// Saves the document (including filled-in forms) to `path`.
    ///
    /// The new file is written next to the target first and then moved into
    /// place, so a crash or full disk never leaves a half-written PDF behind.
    pub fn save(&mut self, path: &Path) -> Result<()> {
        let failed = |e: &dyn std::fmt::Display| Error::new(ErrorCode::SaveFailed, e.to_string());
        let mut bytes = self
            .inner
            .save_to_bytes()
            .map_err(|e| failed(&format!("{e:?}")))?;
        // PDFium can't write metadata; changed title/author/… are added here.
        if let Some(meta) = &self.new_metadata {
            bytes = crate::metadata::apply(bytes, meta)?;
        }
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
        })?;
        // The saved bytes now hold every change, so reopening is safe again.
        if let Source::Memory(_) = self.source {
            self.source = Source::Memory(Arc::new(bytes));
            self.unsaved_changes.set(false);
        }
        Ok(())
    }

    /// Whether [`Document::release_memory`] would free a worthwhile amount.
    pub fn should_release_memory(&self) -> bool {
        self.pages_loaded.get() >= RELEASE_AFTER_PAGES && !self.unsaved_changes.get()
    }

    /// Opens the document again from the same bytes, freeing what PDFium has
    /// kept from pages shown so far. Does nothing while form fields or
    /// annotations have unsaved changes. If reopening fails, the current document stays open.
    pub fn release_memory(&mut self) -> Result<()> {
        if self.unsaved_changes.get() {
            return Ok(());
        }
        let fresh = load(
            self.pdfium,
            &self.source,
            &self.path,
            self.password.as_deref(),
        )?;
        // A big file is read from disk again; if it was changed by another program
        // meanwhile, keep showing the version that is open.
        if fresh.pages().len() != self.inner.pages().len() {
            return Err(Error::new(ErrorCode::Io, "the file changed on disk"));
        }
        self.inner = fresh;
        self.pages_loaded.set(0);
        Ok(())
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
        let page = self.load_page(index)?;
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

    fn load_page(&self, index: u32) -> Result<PdfPage<'_>> {
        self.check_index(index)?;
        self.pages_loaded
            .set(self.pages_loaded.get().saturating_add(1));
        Ok(self.inner.pages().get(index as PdfPageIndex)?)
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
