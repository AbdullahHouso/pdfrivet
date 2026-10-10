use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::BTreeSet,
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

static RAW: OnceLock<&'static dyn PdfiumLibraryBindings> = OnceLock::new();

/// PDFium's C functions, for what pdfium-render doesn't wrap (forms, raw
/// annotation access, page moves…): `pdfium.bindings()`, and the raw handles
/// of documents, pages and forms through them. Only valid after [`Pdf::load`].
pub(crate) trait RawBindings {
    fn bindings(&self) -> &'static dyn PdfiumLibraryBindings;
}

impl RawBindings for Pdfium {
    fn bindings(&self) -> &'static dyn PdfiumLibraryBindings {
        *RAW.get()
            .expect("PDFium is loaded before any document exists")
    }
}

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
        let bind = || {
            Pdfium::bind_to_library(&path).map_err(|e| {
                Error::new(
                    ErrorCode::LibraryNotFound,
                    format!("{}: {e:?}", path.display()),
                )
            })
        };
        // pdfium-render keeps its bindings to itself; a second set (to the same
        // loaded library) is ours for the PDFium functions it doesn't wrap. Both
        // must be made before `Pdfium::new`, which refuses binding again.
        let bindings = bind()?;
        let raw = bind()?;
        RAW.get_or_init(|| Box::leak(raw));
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
        Ok(Document::new(
            self.pdfium,
            inner,
            path,
            source,
            password.map(str::to_owned),
        ))
    }

    /// A new, empty document that lives only in memory until it is saved
    /// (merging, splitting, images to PDF…).
    pub fn new_document(&self) -> Result<Document> {
        let inner = self.pdfium.create_new_pdf()?;
        let doc = Document::new(
            self.pdfium,
            inner,
            Path::new(""),
            Source::Memory(Arc::new(Vec::new())),
            None,
        );
        doc.unsaved_changes.set(true);
        Ok(doc)
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
pub(crate) enum Source {
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

pub(crate) fn load(
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
    /// Bookmarks can be added and changed (not in password-protected files yet).
    pub can_edit_outline: bool,
    /// Comments can be answered (replies need the same rewrite as bookmarks).
    pub can_reply: bool,
    /// Pages can be moved, rotated, deleted, inserted and taken out into other
    /// files (the author's permissions allow assembling the document).
    pub can_assemble: bool,
    /// The document's author allows printing it.
    pub can_print: bool,
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
    pub(crate) pdfium: &'static Pdfium,
    pub(crate) inner: PdfDocument<'static>,
    pub(crate) path: std::path::PathBuf,
    /// Title/author/… changed by the user, written into the file on save.
    pub(crate) new_metadata: Option<crate::metadata::Metadata>,
    /// When PDFRivet last saved the file (a PDF date). PDFium keeps showing the
    /// producer and date the file was opened with, so the properties use this.
    pub(crate) saved_at: Option<String>,
    pub(crate) source: Source,
    pub(crate) password: Option<String>,
    /// Pages PDFium has loaded since the document was (re)opened.
    pub(crate) pages_loaded: Cell<u32>,
    /// Filled-in form fields and annotations live only inside PDFium until
    /// they're saved, so the document must not be reopened while there are any.
    pub(crate) unsaved_changes: Cell<bool>,
    /// Pages with deleted annotations still waiting (hidden) to be removed on save.
    pub(crate) deleted_on: RefCell<BTreeSet<u32>>,
    /// Something was removed (deleted annotations, redactions): the next save
    /// rewrites the file without the leftovers (see `prune.rs`).
    pub(crate) needs_prune: Cell<bool>,
    /// Redacted pages: when saving, they keep only the resources still used.
    pub(crate) redacted: RefCell<BTreeSet<u32>>,
    /// Bookmarks as edited in PDFRivet (PDFium can't change them); written on save.
    pub(crate) edited_outline: Option<Vec<OutlineItem>>,
    /// Replies were added: saving links them to the annotations they answer.
    pub(crate) has_replies: Cell<bool>,
    /// The file's UTF-8 text strings, collected when one PDFium misread
    /// needs looking up (see `textstrings.rs`).
    pub(crate) stored_strings: OnceCell<Vec<Vec<u8>>>,
    /// Text boxes were added or changed: saving writes their `/IT` name.
    pub(crate) has_text_boxes: Cell<bool>,
    /// Passwords to add, change or take off on the next save (see `protect.rs`).
    pub(crate) protection: Option<crate::protect::Protection>,
    /// The owner password, once typed in to change a protected file's protection.
    pub(crate) owner_password: Option<String>,
}

impl Document {
    fn new(
        pdfium: &'static Pdfium,
        inner: PdfDocument<'static>,
        path: &Path,
        source: Source,
        password: Option<String>,
    ) -> Self {
        Document {
            pdfium,
            inner,
            path: path.to_path_buf(),
            new_metadata: None,
            saved_at: None,
            source,
            password,
            pages_loaded: Cell::new(0),
            unsaved_changes: Cell::new(false),
            deleted_on: RefCell::new(BTreeSet::new()),
            needs_prune: Cell::new(false),
            redacted: RefCell::new(BTreeSet::new()),
            edited_outline: None,
            has_replies: Cell::new(false),
            stored_strings: OnceCell::new(),
            has_text_boxes: Cell::new(false),
            protection: None,
            owner_password: None,
        }
    }

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
        let meta = |tag| Some(self.metadata_text(tag)).filter(|v| !v.is_empty());
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
            can_edit_outline: self.can_edit_outline(),
            can_reply: self.can_annotate() && self.can_edit_outline(),
            can_assemble: self.can_assemble(),
            can_print: self.rights().print,
        })
    }

    /// Every permission is granted: the file isn't restricted, it was opened
    /// with its owner password, or that password was typed in since.
    fn has_owner_rights(&self) -> bool {
        self.owner_password.is_some() || self.rights().full
    }

    /// Changes the protection on the next save (see `protect.rs`).
    pub fn set_protection(&mut self, protection: crate::protect::Protection) -> Result<()> {
        if self.properties().encrypted && !self.has_owner_rights() {
            return Err(Error::new(
                ErrorCode::OwnerPasswordRequired,
                "restricted file",
            ));
        }
        self.protection = Some(protection);
        self.unsaved_changes.set(true);
        Ok(())
    }

    /// Checks a protected file's owner password; once it is right, the
    /// protection can be changed. Returns whether it was right.
    pub fn unlock_owner(&mut self, password: &str) -> Result<bool> {
        let right = match load(self.pdfium, &self.source, &self.path, Some(password)) {
            Ok(doc) => crate::rights::read(self.pdfium, &doc).full,
            Err(_) => false,
        };
        if right {
            self.owner_password = Some(password.to_owned());
        }
        Ok(right)
    }

    /// The file's bytes as opened (or last saved), when they're kept in memory.
    pub(crate) fn file_bytes(&self) -> Option<Arc<Vec<u8>>> {
        match &self.source {
            Source::Memory(bytes) if !bytes.is_empty() => Some(bytes.clone()),
            _ => None,
        }
    }

    /// The author's permissions allow changing the pages (see [`DocInfo::can_assemble`]).
    pub(crate) fn can_assemble(&self) -> bool {
        self.rights().assemble
    }

    fn can_annotate(&self) -> bool {
        self.rights().annotate
    }

    /// What the document's author allows (see `rights.rs`).
    pub(crate) fn rights(&self) -> crate::rights::Rights {
        crate::rights::read(self.pdfium, &self.inner)
    }

    /// The annotations on a page (highlights, drawings, notes…), in drawing order.
    pub fn annotations(&self, index: u32) -> Result<Vec<crate::Annotation>> {
        let page = self.load_page(index)?;
        Ok(crate::annotations::read(self.pdfium, &page, &self.texts()))
    }

    /// The annotations of the pages from `first` on, a batch at a time (so
    /// rendering isn't held up), leaving out pages without any.
    pub fn annotations_from(&self, first: u32) -> crate::AnnotationBatch {
        let started = std::time::Instant::now();
        let count = self.page_count();
        let mut pages = Vec::new();
        let mut index = first;
        while index < count {
            if let Ok(annotations) = self.annotations(index)
                && !annotations.is_empty()
            {
                pages.push(crate::PageAnnotations {
                    page: index,
                    annotations,
                });
            }
            index += 1;
            if index - first >= crate::search::BATCH_PAGES
                || started.elapsed() >= crate::search::BATCH_TIME
            {
                break;
            }
        }
        crate::AnnotationBatch {
            pages,
            next_page: (index < count).then_some(index),
        }
    }

    /// Adds an annotation; returns its id.
    pub fn add_annotation(&self, index: u32, annotation: &crate::Annotation) -> Result<String> {
        self.check_can_annotate()?;
        let page = self.load_page(index)?;
        let id = crate::annotations::add(self.pdfium, &page, annotation)?;
        self.unsaved_changes.set(true);
        if annotation.reply_to.is_some() {
            self.has_replies.set(true);
        }
        self.note_text_box(annotation);
        Ok(id)
    }

    /// Changes an annotation (found by its id); returns its id, which is new
    /// for an annotation that had none.
    pub fn update_annotation(&self, index: u32, annotation: &crate::Annotation) -> Result<String> {
        self.check_can_annotate()?;
        let page = self.load_page(index)?;
        let id = crate::annotations::update(self.pdfium, &page, annotation, &self.texts())?;
        self.unsaved_changes.set(true);
        self.note_text_box(annotation);
        Ok(id)
    }

    fn note_text_box(&self, annotation: &crate::Annotation) {
        if matches!(annotation.kind, crate::AnnotationKind::FreeText { .. }) {
            self.has_text_boxes.set(true);
        }
    }

    /// Places a picture (a signature) as a stamp in `annotation.rect`; returns its id.
    pub fn add_image_stamp(
        &self,
        index: u32,
        annotation: &crate::Annotation,
        image: &crate::StampImage,
    ) -> Result<String> {
        self.check_can_annotate()?;
        let mut page = self.load_page(index)?;
        let id = crate::annotations::add_image_stamp(
            self.pdfium,
            &self.inner,
            &mut page,
            annotation,
            image,
        )?;
        self.unsaved_changes.set(true);
        Ok(id)
    }

    /// Deletes an annotation. Until the document is saved it can be brought
    /// back exactly as it was ([`Document::restore_annotation`]).
    pub fn delete_annotation(&self, index: u32, id: &str) -> Result<()> {
        self.check_can_annotate()?;
        let page = self.load_page(index)?;
        crate::annotations::delete(self.pdfium, &page, id)?;
        self.deleted_on.borrow_mut().insert(index);
        self.unsaved_changes.set(true);
        Ok(())
    }

    /// Brings back a deleted annotation (undo).
    pub fn restore_annotation(&self, index: u32, id: &str) -> Result<()> {
        self.check_can_annotate()?;
        let page = self.load_page(index)?;
        crate::annotations::restore(self.pdfium, &page, id)?;
        self.unsaved_changes.set(true);
        Ok(())
    }

    /// Permanently removes what is under `areas` (page fractions) of a page, and
    /// blacks them out. The removed content is gone from the file once saved.
    pub fn redact(&self, index: u32, areas: &[crate::PageRect]) -> Result<()> {
        self.check_can_annotate()?;
        if self.properties().encrypted {
            // The leftovers couldn't be removed from the saved file (see prune.rs).
            return Err(Error::new(
                ErrorCode::RedactProtected,
                "password-protected PDF",
            ));
        }
        let mut page = self.load_page(index)?;
        crate::redact::redact(self.pdfium, &self.inner, &mut page, areas)?;
        self.redacted.borrow_mut().insert(index);
        self.unsaved_changes.set(true);
        self.needs_prune.set(true);
        Ok(())
    }

    /// Hides or shows an annotation while it is dragged (not a change to the document).
    pub fn set_annotation_hidden(&self, index: u32, id: &str, hidden: bool) -> Result<()> {
        let page = self.load_page(index)?;
        crate::annotations::set_hidden(self.pdfium, &page, id, hidden)
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
        self.rights().copy
    }

    /// The document's table of contents (bookmarks), as edited. Empty if it has none.
    pub fn outline(&self) -> Vec<OutlineItem> {
        match &self.edited_outline {
            Some(items) => items.clone(),
            None => outline::read(&self.inner, &self.texts()),
        }
    }

    /// Replaces the bookmarks (written into the file on the next save).
    /// Entries from [`Document::outline`] keep their `origin`.
    pub fn set_outline(&mut self, items: Vec<OutlineItem>) -> Result<()> {
        if !self.can_edit_outline() {
            return Err(Error::new(
                ErrorCode::ReadOnlyField,
                "password-protected PDF",
            ));
        }
        self.edited_outline = Some(items);
        self.unsaved_changes.set(true);
        Ok(())
    }

    /// Bookmarks can be changed (lopdf, which writes them, can't write encrypted files).
    pub(crate) fn can_edit_outline(&self) -> bool {
        self.properties().can_edit_metadata
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

    /// One of the document's descriptive texts (title, author…), trimmed;
    /// empty if missing.
    fn metadata_text(&self, tag: PdfDocumentMetadataTagType) -> String {
        let Some(value) = self.inner.metadata().get(tag).map(|t| t.value().to_owned()) else {
            return String::new();
        };
        let key: &[u8] = match tag {
            PdfDocumentMetadataTagType::Title => b"Title",
            PdfDocumentMetadataTagType::Author => b"Author",
            PdfDocumentMetadataTagType::Subject => b"Subject",
            PdfDocumentMetadataTagType::Keywords => b"Keywords",
            PdfDocumentMetadataTagType::Creator => b"Creator",
            PdfDocumentMetadataTagType::Producer => b"Producer",
            PdfDocumentMetadataTagType::CreationDate => b"CreationDate",
            PdfDocumentMetadataTagType::ModificationDate => b"ModDate",
        };
        self.texts().repair_info(value, key).trim().to_owned()
    }

    /// Repairs text strings PDFium misreads (see `textstrings.rs`).
    fn texts(&self) -> crate::textstrings::Texts<'_> {
        crate::textstrings::Texts {
            stored: &self.stored_strings,
            bytes: match &self.source {
                Source::Memory(bytes) => Some(bytes),
                Source::File => None,
            },
            path: &self.path,
            password: self.password.as_deref(),
        }
    }

    /// Everything shown in the Document properties dialog.
    pub fn properties(&self) -> crate::metadata::DocProperties {
        use crate::metadata::{DocProperties, Metadata, Permissions, pdf_date_to_iso};
        let tag = |t| self.metadata_text(t);
        let stored = Metadata {
            title: tag(PdfDocumentMetadataTagType::Title),
            author: tag(PdfDocumentMetadataTagType::Author),
            subject: tag(PdfDocumentMetadataTagType::Subject),
            keywords: tag(PdfDocumentMetadataTagType::Keywords),
        };
        let rights = self.rights();
        let encrypted = rights.encrypted;
        let version = format!("{:?}", self.inner.version());
        DocProperties {
            metadata: self.new_metadata.clone().unwrap_or(stored),
            creator: tag(PdfDocumentMetadataTagType::Creator),
            producer: match self.saved_at {
                Some(_) => crate::metadata::PRODUCER.to_owned(),
                None => tag(PdfDocumentMetadataTagType::Producer),
            },
            created: pdf_date_to_iso(&tag(PdfDocumentMetadataTagType::CreationDate)),
            modified: pdf_date_to_iso(
                self.saved_at
                    .as_deref()
                    .unwrap_or(&tag(PdfDocumentMetadataTagType::ModificationDate)),
            ),
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
                print: rights.print,
                copy: rights.copy,
                modify: rights.modify,
                fill_forms: rights.fill_forms,
                annotate: rights.annotate,
            },
            can_edit_metadata: !encrypted,
            needs_open_password: encrypted && self.password.is_some(),
            can_change_protection: !encrypted || self.has_owner_rights(),
            protection_pending: self.protection.is_some(),
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
        Ok(forms::read(&page, &self.texts()))
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
        let (bytes, saved_at) = self.final_bytes()?;
        write_atomically(path, &bytes)?;
        // The saved bytes now hold every change, so reopening is safe again.
        // Redacted leftovers aren't in the saved file; later saves needn't prune
        // for them. (Deleted annotations are left out of every save until undone.)
        if saved_at.is_some() {
            self.saved_at = saved_at;
        }
        self.needs_prune.set(false);
        self.redacted.borrow_mut().clear();
        if let Source::Memory(_) = self.source {
            self.source = Source::Memory(Arc::new(bytes));
            self.unsaved_changes.set(false);
        }
        if let Some(protection) = self.protection.take() {
            // The saved file opens with its new password (or none).
            self.password = match protection {
                crate::protect::Protection::Set { open_password, .. }
                    if !open_password.is_empty() =>
                {
                    Some(open_password)
                }
                _ => None,
            };
            self.owner_password = None;
            // Show the document as saved, protected or not (unless deleted
            // annotations are kept hidden for undo: they're not in the file).
            if self.deleted_on.borrow().is_empty()
                && matches!(self.source, Source::Memory(_))
                && let Ok(fresh) = load(
                    self.pdfium,
                    &self.source,
                    &self.path,
                    self.password.as_deref(),
                )
            {
                self.inner = fresh;
                self.pages_loaded.set(0);
            }
        }
        Ok(())
    }

    /// The document as it would be saved, with every change: what [`Document::save`]
    /// writes, and what tools working on an open document (split, compress…) read.
    /// Also returns the modification date written into it, if any.
    pub fn final_bytes(&mut self) -> Result<(Vec<u8>, Option<String>)> {
        let failed = |e: &dyn std::fmt::Display| Error::new(ErrorCode::SaveFailed, e.to_string());
        let has_deleted = !self.deleted_on.borrow().is_empty();
        let mut bytes = self
            .inner
            .save_to_bytes()
            .map_err(|e| failed(&format!("{e:?}")))?;
        // Changing a protected file's protection: start from its unprotected bytes.
        if self.protection.is_some() && self.properties().encrypted {
            let password = self.owner_password.as_deref().or(self.password.as_deref());
            bytes = crate::protect::decrypt(&bytes, password.unwrap_or(""))?;
        }
        let outline = self.edited_outline.as_deref();
        let replies = self.has_replies.get();
        let text_boxes = self.has_text_boxes.get();
        if has_deleted || self.needs_prune.get() || outline.is_some() || replies || text_boxes {
            let redacted: Vec<u32> = self.redacted.borrow().iter().copied().collect();
            match crate::prune::prune(
                &bytes,
                crate::annotations::DELETED_KEY,
                &redacted,
                outline,
                replies,
                text_boxes,
            )? {
                Some(pruned) => bytes = pruned,
                // Password-protected: deleted annotations can't be left out of the
                // written file, so they are removed from the document for good.
                None if has_deleted => {
                    let pages: Vec<u32> = std::mem::take(&mut *self.deleted_on.borrow_mut())
                        .into_iter()
                        .collect();
                    for index in pages {
                        if let Ok(page) = self.load_page(index) {
                            crate::annotations::purge_deleted(self.pdfium, &page);
                        }
                    }
                    bytes = self
                        .inner
                        .save_to_bytes()
                        .map_err(|e| failed(&format!("{e:?}")))?;
                }
                None => {}
            }
        }
        // PDFium can't write metadata: PDFRivet as the producer, the date and
        // a changed title, author… are added here.
        let (mut bytes, saved_at) = crate::metadata::stamp(bytes, self.new_metadata.as_ref())?;
        if let Some(crate::protect::Protection::Set {
            open_password,
            owner_password,
            allowed,
        }) = &self.protection
        {
            bytes = crate::protect::encrypt(&bytes, open_password, owner_password, *allowed)?;
        }
        Ok((bytes, saved_at))
    }
}

/// Writes `bytes` next to `path` first and then moves them into place, so a
/// crash or full disk never leaves a half-written file behind.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
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
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        Error::new(ErrorCode::SaveFailed, e.to_string())
    })
}

impl Document {
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

    /// Renders one area of a page (pixels `x, y, width, height` of the page
    /// rendered at `scale` and `rotation`), exactly as that area looks in the
    /// whole render. Used to update a small part of a page at once (an
    /// annotation picked up to move) without waiting for the whole page.
    #[allow(clippy::too_many_arguments)]
    pub fn render_region(
        &self,
        index: u32,
        scale: f32,
        rotation: Rotation,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<RenderedPage> {
        let page = self.load_page(index)?;
        let (width, height) = (width.clamp(1, 8192), height.clamp(1, 8192));
        let config = PdfRenderConfig::new()
            .scale_page_by_factor(scale.clamp(0.05, 16.0))
            .rotate(rotation.to_pdfium(), true)
            .set_format(PdfBitmapFormat::BGRA)
            .set_reverse_byte_order(true)
            .set_clear_color(PdfColor::WHITE)
            .render_annotations(true)
            .render_form_data(true)
            // The page is drawn shifted so the area lands in the small bitmap.
            .set_origin(
                -(x.min(i32::MAX as u32) as i32),
                -(y.min(i32::MAX as u32) as i32),
            );
        let mut bitmap = PdfBitmap::empty(width as i32, height as i32, PdfBitmapFormat::BGRA)?;
        page.render_into_bitmap_with_config(&mut bitmap, &config)?;
        Ok(RenderedPage {
            width,
            height,
            rgba: bitmap.as_rgba_bytes().into(),
        })
    }

    // `'static` like the document itself (pdfium-render hands pages out that way);
    // pages are only used within one call, never kept.
    pub(crate) fn load_page(&self, index: u32) -> Result<PdfPage<'static>> {
        self.check_index(index)?;
        self.pages_loaded
            .set(self.pages_loaded.get().saturating_add(1));
        Ok(self.inner.pages().get(index as PdfPageIndex)?)
    }

    pub(crate) fn check_index(&self, index: u32) -> Result<()> {
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
