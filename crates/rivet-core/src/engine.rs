//! PDFium is not thread-safe, so the app talks to it through an [`Engine`]:
//! one worker thread owns PDFium and every open document, and other threads
//! send it requests over a channel and wait for the reply.
//!
//! Two tricks keep scrolling smooth:
//! - The UI tells the engine which pages are visible ([`Engine::set_visible_pages`]).
//!   Render requests for pages that scrolled far away are answered with
//!   [`ErrorCode::Cancelled`] instead of being rendered.
//! - Waiting render requests are served newest first, so the page you are
//!   looking at now wins over pages you scrolled past.

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use crate::{
    Annotation, DocInfo, Document, Error, ErrorCode, FieldChange, FormField, MergePart,
    OutlineItem, PageLink, PageSlot, PageText, Pdf, RenderedPage, Result, Rotation, SearchBatch,
    SearchQuery, Snapshot, StampImage, TextRange,
    cache::{Key, RenderCache},
    metadata::{DocProperties, Metadata},
    print::PrintSettings,
};

/// Identifies an open document inside an [`Engine`].
pub type DocId = u32;

/// Memory budget for already-rendered pages kept by the engine.
const CACHE_BUDGET_BYTES: usize = 48 * 1024 * 1024;

/// Pages this far outside the visible range are not rendered.
const VISIBLE_MARGIN: u32 = 2;

/// Memory for earlier states of documents kept for undo (see [`Snapshot`]).
/// Beyond it the oldest are let go, and those changes can no longer be undone.
const SNAPSHOT_BUDGET_BYTES: usize = 1024 * 1024 * 1024;

/// Identifies a [`Snapshot`] kept by the engine.
pub type SnapshotId = u32;

type Reply<T> = mpsc::Sender<Result<T>>;

/// Why a page is rendered, which decides how the engine treats the request.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
    /// The main view: skipped if far from the visible range; cached.
    View,
    /// Sidebar thumbnails: always rendered; cached.
    Thumbnail,
    /// Printing: always rendered; not cached (big bitmaps would push out pages you're reading).
    Print,
    /// Only answered from the cache; never rendered.
    Cached,
}

struct RenderRequest {
    doc: DocId,
    page: u32,
    scale: f32,
    rotation: Rotation,
    purpose: Purpose,
    reply: Reply<RenderedPage>,
}

enum Request {
    Open(PathBuf, Option<String>, Reply<(DocId, DocInfo)>),
    Info(DocId, Reply<DocInfo>),
    Outline(DocId, Reply<Vec<OutlineItem>>),
    SetOutline(DocId, Vec<OutlineItem>, Reply<()>),
    RenderRegion(DocId, u32, f32, Rotation, [u32; 4], Reply<RenderedPage>),
    Links(DocId, u32, Reply<Vec<PageLink>>),
    FormFields(DocId, u32, Reply<Vec<FormField>>),
    PageText(DocId, u32, Reply<PageText>),
    Text(DocId, TextRange, Reply<String>),
    Search(DocId, SearchQuery, u32, Reply<SearchBatch>),
    Annotations(DocId, u32, Reply<Vec<Annotation>>),
    AnnotationsFrom(DocId, u32, Reply<crate::AnnotationBatch>),
    AddAnnotation(DocId, u32, Box<Annotation>, Reply<String>),
    UpdateAnnotation(DocId, u32, Box<Annotation>, Reply<String>),
    DeleteAnnotation(DocId, u32, String, Reply<()>),
    RestoreAnnotation(DocId, u32, String, Reply<()>),
    Redact(DocId, u32, Vec<crate::PageRect>, Reply<()>),
    SetAnnotationHidden(DocId, u32, String, bool, Reply<()>),
    AddImageStamp(DocId, u32, Box<(Annotation, StampImage)>, Reply<String>),
    ChangeField(DocId, u32, u32, FieldChange, Reply<()>),
    Save(DocId, PathBuf, Reply<()>),
    Print(DocId, Box<PrintSettings>, Option<Vec<u8>>, Reply<()>),
    Properties(DocId, Reply<DocProperties>),
    SetMetadata(DocId, Metadata, Reply<()>),
    Render(RenderRequest),
    SetVisible(DocId, u32, u32),
    Close(DocId),
    /// Any other work on the worker's documents (page tools and the like): a
    /// closure that replies itself, so each new operation needs no variant.
    Task(Box<dyn FnOnce(&mut Worker) + Send>),
}

/// A cheap, cloneable handle to the PDFium worker thread.
#[derive(Clone)]
pub struct Engine {
    tx: mpsc::Sender<Request>,
}

impl Engine {
    /// Starts the worker thread and loads PDFium from `lib_dir`.
    pub fn start(lib_dir: &Path) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Request>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        let lib_dir = lib_dir.to_path_buf();

        thread::Builder::new()
            .name("pdfium".into())
            .spawn(move || match Pdf::load(&lib_dir) {
                Ok(pdf) => {
                    let _ = ready_tx.send(Ok(()));
                    Worker::new(pdf).run(rx);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?;

        ready_rx
            .recv()
            .map_err(|_| Error::new(ErrorCode::EngineStopped, "worker exited during startup"))??;
        Ok(Self { tx })
    }

    /// Opens a document. Fails with [`ErrorCode::PasswordRequired`] or
    /// [`ErrorCode::WrongPassword`] for protected files.
    pub fn open(&self, path: &Path, password: Option<String>) -> Result<(DocId, DocInfo)> {
        self.call(|reply| Request::Open(path.to_path_buf(), password, reply))
    }

    /// Basic facts about an open document (as returned by [`Engine::open`]).
    pub fn info(&self, doc: DocId) -> Result<DocInfo> {
        self.call(|reply| Request::Info(doc, reply))
    }

    pub fn outline(&self, doc: DocId) -> Result<Vec<OutlineItem>> {
        self.call(|reply| Request::Outline(doc, reply))
    }

    /// Replaces the bookmarks; they are written into the file on the next save.
    pub fn set_outline(&self, doc: DocId, items: Vec<OutlineItem>) -> Result<()> {
        self.call(|reply| Request::SetOutline(doc, items, reply))
    }

    /// The clickable links on a page.
    pub fn links(&self, doc: DocId, page: u32) -> Result<Vec<PageLink>> {
        self.call(|reply| Request::Links(doc, page, reply))
    }

    /// Renders a page for the main view. Pages far from the visible range
    /// (see [`Engine::set_visible_pages`]) fail fast with [`ErrorCode::Cancelled`].
    pub fn render(
        &self,
        doc: DocId,
        page: u32,
        scale: f32,
        rotation: Rotation,
    ) -> Result<RenderedPage> {
        self.render_request(doc, page, scale, rotation, Purpose::View)
    }

    /// A page that is already rendered at this size, without rendering it.
    /// Fails with [`ErrorCode::Cancelled`] when it isn't in the cache, so the UI
    /// can show a quick preview first only when a real render is needed.
    pub fn cached(
        &self,
        doc: DocId,
        page: u32,
        scale: f32,
        rotation: Rotation,
    ) -> Result<RenderedPage> {
        self.render_request(doc, page, scale, rotation, Purpose::Cached)
    }

    /// Renders one area of a page (`[x, y, width, height]` in pixels of the
    /// page at `scale`), without the cache: a part of the page that just changed.
    pub fn render_region(
        &self,
        doc: DocId,
        page: u32,
        scale: f32,
        rotation: Rotation,
        area: [u32; 4],
    ) -> Result<RenderedPage> {
        self.call(|reply| Request::RenderRegion(doc, page, scale, rotation, area, reply))
    }

    /// Renders a page regardless of what is visible (used for thumbnails).
    pub fn render_thumbnail(
        &self,
        doc: DocId,
        page: u32,
        scale: f32,
        rotation: Rotation,
    ) -> Result<RenderedPage> {
        self.render_request(doc, page, scale, rotation, Purpose::Thumbnail)
    }

    /// Renders a page for printing (upright, not cached).
    pub fn render_for_print(&self, doc: DocId, page: u32, scale: f32) -> Result<RenderedPage> {
        self.render_request(doc, page, scale, Rotation::None, Purpose::Print)
    }

    fn render_request(
        &self,
        doc: DocId,
        page: u32,
        scale: f32,
        rotation: Rotation,
        purpose: Purpose,
    ) -> Result<RenderedPage> {
        self.call(|reply| {
            Request::Render(RenderRequest {
                doc,
                page,
                scale,
                rotation,
                purpose,
                reply,
            })
        })
    }

    /// The interactive form fields on a page.
    pub fn form_fields(&self, doc: DocId, page: u32) -> Result<Vec<FormField>> {
        self.call(|reply| Request::FormFields(doc, page, reply))
    }

    /// Every character of a page with its box (for selecting text).
    pub fn page_text(&self, doc: DocId, page: u32) -> Result<PageText> {
        self.call(|reply| Request::PageText(doc, page, reply))
    }

    /// The text of a range of characters (for copying).
    pub fn text(&self, doc: DocId, range: TextRange) -> Result<String> {
        self.call(|reply| Request::Text(doc, range, reply))
    }

    /// Searches the document from page `first` on. Returns after a batch of
    /// pages; continue from [`SearchBatch::next_page`].
    pub fn search(&self, doc: DocId, query: SearchQuery, first: u32) -> Result<SearchBatch> {
        self.call(|reply| Request::Search(doc, query, first, reply))
    }

    /// The annotations on a page.
    pub fn annotations(&self, doc: DocId, page: u32) -> Result<Vec<Annotation>> {
        self.call(|reply| Request::Annotations(doc, page, reply))
    }

    /// Annotations of every page from `first` on, a batch of pages at a time.
    pub fn annotations_from(&self, doc: DocId, first: u32) -> Result<crate::AnnotationBatch> {
        self.call(|reply| Request::AnnotationsFrom(doc, first, reply))
    }

    /// Adds an annotation; returns its id. The page's renders are refreshed.
    pub fn add_annotation(&self, doc: DocId, page: u32, annotation: Annotation) -> Result<String> {
        self.call(|reply| Request::AddAnnotation(doc, page, Box::new(annotation), reply))
    }

    /// Changes an annotation (found by its id); returns its id.
    pub fn update_annotation(
        &self,
        doc: DocId,
        page: u32,
        annotation: Annotation,
    ) -> Result<String> {
        self.call(|reply| Request::UpdateAnnotation(doc, page, Box::new(annotation), reply))
    }

    /// Places a picture (a signature) as a stamp; returns its id.
    pub fn add_image_stamp(
        &self,
        doc: DocId,
        page: u32,
        annotation: Annotation,
        image: StampImage,
    ) -> Result<String> {
        self.call(|reply| Request::AddImageStamp(doc, page, Box::new((annotation, image)), reply))
    }

    /// Deletes an annotation (it can be restored until the document is saved).
    pub fn delete_annotation(&self, doc: DocId, page: u32, id: String) -> Result<()> {
        self.call(|reply| Request::DeleteAnnotation(doc, page, id, reply))
    }

    /// Permanently removes what is under `areas` of a page (see `redact.rs`).
    pub fn redact(&self, doc: DocId, page: u32, areas: Vec<crate::PageRect>) -> Result<()> {
        self.call(|reply| Request::Redact(doc, page, areas, reply))
    }

    /// Brings back a deleted annotation (undo).
    pub fn restore_annotation(&self, doc: DocId, page: u32, id: String) -> Result<()> {
        self.call(|reply| Request::RestoreAnnotation(doc, page, id, reply))
    }

    /// Hides or shows an annotation while it is dragged; the page's renders are refreshed.
    pub fn set_annotation_hidden(
        &self,
        doc: DocId,
        page: u32,
        id: String,
        hidden: bool,
    ) -> Result<()> {
        self.call(|reply| Request::SetAnnotationHidden(doc, page, id, hidden, reply))
    }

    /// Changes a form field. Rendered pages of the document are refreshed.
    pub fn change_field(
        &self,
        doc: DocId,
        page: u32,
        field: u32,
        change: FieldChange,
    ) -> Result<()> {
        self.call(|reply| Request::ChangeField(doc, page, field, change, reply))
    }

    /// Saves the document (with its changes) to `path`.
    pub fn save(&self, doc: DocId, path: &Path) -> Result<()> {
        self.call(|reply| Request::Save(doc, path.to_path_buf(), reply))
    }

    /// Prints a document. Blocks until the job has been handed to the printer.
    pub fn print(
        &self,
        doc: DocId,
        settings: PrintSettings,
        printer_settings: Option<Vec<u8>>,
    ) -> Result<()> {
        self.call(|reply| Request::Print(doc, Box::new(settings), printer_settings, reply))
    }

    /// Everything shown in the Document properties dialog.
    pub fn properties(&self, doc: DocId) -> Result<DocProperties> {
        self.call(|reply| Request::Properties(doc, reply))
    }

    /// Changes the title, author, subject and keywords (written on the next save).
    pub fn set_metadata(&self, doc: DocId, metadata: Metadata) -> Result<()> {
        self.call(|reply| Request::SetMetadata(doc, metadata, reply))
    }

    /// Tells the engine which pages (inclusive, 0-based) are on screen.
    pub fn set_visible_pages(&self, doc: DocId, first: u32, last: u32) {
        let _ = self.tx.send(Request::SetVisible(doc, first, last));
    }

    pub fn close(&self, doc: DocId) {
        let _ = self.tx.send(Request::Close(doc));
    }

    /// A new, empty document in memory (for merging, splitting, images to PDF…).
    pub fn new_document(&self) -> Result<(DocId, DocInfo)> {
        self.task(|w| {
            let doc = w.pdf.new_document()?;
            let info = doc.info()?;
            Ok((w.add(doc), info))
        })
    }

    /// The document with all its changes, as saving would write it.
    pub fn final_bytes(&self, doc: DocId) -> Result<Vec<u8>> {
        self.task(move |w| w.doc_mut(doc)?.final_bytes().map(|(bytes, _)| bytes))
    }

    /// Rearranges a document's pages (see [`Document::arrange`]). Returns the
    /// state before, for undo (see [`Engine::swap_snapshot`]), and the new facts.
    /// If anything fails, the document is left as it was.
    pub fn arrange(&self, doc: DocId, slots: Vec<PageSlot>) -> Result<(SnapshotId, DocInfo)> {
        self.task(move |w| {
            let mut target = w.docs.remove(&doc).ok_or_else(|| not_open(doc))?;
            let others: Vec<(DocId, &Document)> = w.docs.iter().map(|(id, d)| (*id, d)).collect();
            let result = target.snapshot().and_then(|before| {
                match target.arrange(&slots, &others) {
                    Ok(()) => Ok(before),
                    Err(e) => {
                        // Put back what was done before the failure.
                        let _ = target.restore(before);
                        Err(e)
                    }
                }
            });
            w.docs.insert(doc, target);
            w.structure_changed(doc);
            let before = result?;
            Ok((w.keep_snapshot(doc, before), w.doc(doc)?.info()?))
        })
    }

    /// Turns pages by quarter turns (clockwise; negative: counter-clockwise).
    pub fn rotate_pages(&self, doc: DocId, pages: Vec<u32>, turns: i32) -> Result<DocInfo> {
        self.task(move |w| {
            w.doc_mut(doc)?.rotate_pages(&pages, turns)?;
            w.structure_changed(doc);
            w.doc(doc)?.info()
        })
    }

    /// Copies pages of document `source` into `doc`, starting at page `at`.
    pub fn import_pages(
        &self,
        doc: DocId,
        source: DocId,
        pages: Vec<u32>,
        at: u32,
    ) -> Result<DocInfo> {
        self.task(move |w| {
            if doc == source {
                return Err(Error::new(
                    ErrorCode::Internal,
                    "importing a document into itself",
                ));
            }
            let mut target = w.docs.remove(&doc).ok_or_else(|| not_open(doc))?;
            let result = w
                .doc(source)
                .and_then(|from| target.import_pages(from, &pages, at));
            w.docs.insert(doc, target);
            w.structure_changed(doc);
            result?;
            w.doc(doc)?.info()
        })
    }

    /// Finishes a merged document (pages already copied in with
    /// [`Engine::import_pages`]): bookmarks and form fields, then writes it to `path`.
    pub fn finish_merge(
        &self,
        doc: DocId,
        parts: Vec<MergePart>,
        bookmark_files: bool,
        path: PathBuf,
    ) -> Result<()> {
        self.task(move |w| {
            let mut merged = w.docs.remove(&doc).ok_or_else(|| not_open(doc))?;
            let result = parts
                .iter()
                .map(|part| Ok((part.clone(), w.doc(part.doc)?)))
                .collect::<Result<Vec<_>>>()
                .and_then(|sources| {
                    crate::merge::finish(&mut merged, &sources, bookmark_files, &path)
                });
            w.docs.insert(doc, merged);
            result
        })
    }

    /// Repairs a damaged PDF file into a new one (see `repair.rs`).
    pub fn repair(
        &self,
        from: PathBuf,
        to: PathBuf,
        password: Option<String>,
    ) -> Result<crate::RepairReport> {
        self.task(move |w| w.pdf.repair(&from, &to, password.as_deref()))
    }

    /// Adds, changes or removes password protection on the next save.
    pub fn set_protection(&self, doc: DocId, protection: crate::Protection) -> Result<()> {
        self.task(move |w| w.doc_mut(doc)?.set_protection(protection))
    }

    /// Checks a protected file's owner password (see [`Document::unlock_owner`]).
    pub fn unlock_owner(&self, doc: DocId, password: String) -> Result<bool> {
        self.task(move |w| w.doc_mut(doc)?.unlock_owner(&password))
    }

    /// Writes some pages of a document (as they are now, with unsaved changes)
    /// into a new PDF file at `path`.
    /// The pages' bookmarks and form fields come along (see `merge.rs`).
    pub fn extract_pages(&self, doc: DocId, pages: Vec<u32>, path: PathBuf) -> Result<()> {
        self.task(move |w| {
            let source = w.doc(doc)?;
            let mut out = w.pdf.new_document()?;
            out.import_pages(source, &pages, 0)?;
            // Part of the same document: same title, author, subject and keywords.
            out.new_metadata = Some(source.properties().metadata);
            let part = MergePart {
                doc,
                pages,
                start: 0,
                title: String::new(),
            };
            crate::merge::finish(&mut out, &[(part, source)], false, &path)
        })
    }

    /// Undo and redo of page changes: puts the document back to a kept state,
    /// and keeps the state it replaces instead (returned, for going back again).
    pub fn swap_snapshot(&self, doc: DocId, id: SnapshotId) -> Result<(SnapshotId, DocInfo)> {
        self.task(move |w| {
            let (owner, snapshot) = w
                .snapshots
                .take(id)
                .ok_or_else(|| Error::new(ErrorCode::UndoUnavailable, format!("snapshot {id}")))?;
            if owner != doc {
                return Err(Error::new(
                    ErrorCode::Internal,
                    "snapshot of another document",
                ));
            }
            let target = w.doc_mut(doc)?;
            let now = target.snapshot()?;
            target.restore(snapshot)?;
            w.structure_changed(doc);
            Ok((w.keep_snapshot(doc, now), w.doc(doc)?.info()?))
        })
    }

    /// Lets go of kept states (their changes can no longer be undone).
    pub fn drop_snapshots(&self, ids: Vec<SnapshotId>) {
        let _ = self.tx.send(Request::Task(Box::new(move |w| {
            for id in ids {
                w.snapshots.take(id);
            }
        })));
    }

    /// Runs `work` on the worker thread and waits for its result.
    fn task<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Worker) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.call(|reply| {
            Request::Task(Box::new(move |worker| {
                let _ = reply.send(work(worker));
            }))
        })
    }

    fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Request) -> Result<T> {
        let stopped = || Error::new(ErrorCode::EngineStopped, "PDFium worker is not running");
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx.send(make(reply_tx)).map_err(|_| stopped())?;
        reply_rx.recv().map_err(|_| stopped())?
    }
}

struct Worker {
    pdf: Pdf,
    docs: HashMap<DocId, Document>,
    visible: HashMap<DocId, (u32, u32)>,
    cache: RenderCache,
    next_id: DocId,
    snapshots: Snapshots,
}

/// Earlier states of documents, oldest first, within [`SNAPSHOT_BUDGET_BYTES`].
#[derive(Default)]
struct Snapshots {
    kept: VecDeque<(SnapshotId, DocId, Snapshot)>,
    bytes: usize,
    next_id: SnapshotId,
}

impl Snapshots {
    fn add(&mut self, doc: DocId, snapshot: Snapshot) -> SnapshotId {
        self.next_id += 1;
        self.bytes += snapshot.size();
        self.kept.push_back((self.next_id, doc, snapshot));
        // Keep at least the newest one, even if it alone is over budget.
        while self.bytes > SNAPSHOT_BUDGET_BYTES && self.kept.len() > 1 {
            if let Some((_, _, old)) = self.kept.pop_front() {
                self.bytes -= old.size();
            }
        }
        self.next_id
    }

    fn take(&mut self, id: SnapshotId) -> Option<(DocId, Snapshot)> {
        let at = self.kept.iter().position(|(i, _, _)| *i == id)?;
        let (_, doc, snapshot) = self.kept.remove(at)?;
        self.bytes -= snapshot.size();
        Some((doc, snapshot))
    }

    fn forget_doc(&mut self, doc: DocId) {
        self.kept.retain(|(_, d, _)| *d != doc);
        self.bytes = self.kept.iter().map(|(_, _, s)| s.size()).sum();
    }
}

fn not_open(id: DocId) -> Error {
    Error::new(ErrorCode::DocumentNotOpen, format!("document {id}"))
}

impl Worker {
    fn new(pdf: Pdf) -> Self {
        Self {
            pdf,
            docs: HashMap::new(),
            visible: HashMap::new(),
            cache: RenderCache::new(CACHE_BUDGET_BYTES),
            next_id: 1,
            snapshots: Snapshots::default(),
        }
    }

    fn add(&mut self, doc: Document) -> DocId {
        let id = self.next_id;
        self.next_id += 1;
        self.docs.insert(id, doc);
        id
    }

    fn keep_snapshot(&mut self, doc: DocId, snapshot: Snapshot) -> SnapshotId {
        self.snapshots.add(doc, snapshot)
    }

    /// Pages moved, appeared or went: every render of the document is stale.
    fn structure_changed(&mut self, doc: DocId) {
        self.cache.remove_doc(doc);
        self.visible.remove(&doc);
    }

    /// Runs until every [`Engine`] handle has been dropped.
    fn run(mut self, rx: mpsc::Receiver<Request>) {
        while let Ok(first) = rx.recv() {
            // Take everything that is already waiting. Other requests run in
            // order; renders run newest first.
            let mut renders = Vec::new();
            for request in std::iter::once(first).chain(rx.try_iter()) {
                match request {
                    Request::Render(r) => renders.push(r),
                    other => self.handle(other),
                }
            }
            for render in renders.into_iter().rev() {
                let result = self.render(&render);
                let _ = render.reply.send(result);
            }
            self.release_memory();
        }
    }

    /// Reopens documents that have shown many pages, so PDFium's per-document
    /// caches don't keep growing (see `Document::release_memory`). Runs between
    /// batches of requests, so it never delays a page that is waiting.
    fn release_memory(&mut self) {
        for (id, doc) in &mut self.docs {
            if doc.should_release_memory()
                && let Err(e) = doc.release_memory()
            {
                eprintln!("Couldn't reopen document {id} to free memory: {e}");
            }
        }
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Open(path, password, reply) => {
                let result = self.pdf.open(&path, password.as_deref()).and_then(|doc| {
                    let info = doc.info()?;
                    Ok((self.add(doc), info))
                });
                let _ = reply.send(result);
            }
            Request::Info(id, reply) => {
                let _ = reply.send(self.doc(id).and_then(Document::info));
            }
            Request::Outline(id, reply) => {
                let _ = reply.send(self.doc(id).map(Document::outline));
            }
            Request::RenderRegion(id, page, scale, rotation, [x, y, w, h], reply) => {
                let _ = reply.send(
                    self.doc(id)
                        .and_then(|d| d.render_region(page, scale, rotation, x, y, w, h)),
                );
            }
            Request::SetOutline(id, items, reply) => {
                let result = match self.docs.get_mut(&id) {
                    Some(doc) => doc.set_outline(items),
                    None => Err(Error::new(
                        ErrorCode::DocumentNotOpen,
                        format!("document {id}"),
                    )),
                };
                let _ = reply.send(result);
            }
            Request::Links(id, page, reply) => {
                let _ = reply.send(self.doc(id).and_then(|d| d.links(page)));
            }
            Request::FormFields(id, page, reply) => {
                let _ = reply.send(self.doc(id).and_then(|d| d.form_fields(page)));
            }
            Request::PageText(id, page, reply) => {
                let _ = reply.send(self.doc(id).and_then(|d| d.page_text(page)));
            }
            Request::Text(id, range, reply) => {
                let _ = reply.send(self.doc(id).and_then(|d| d.text(range)));
            }
            Request::Search(id, query, first, reply) => {
                let _ = reply.send(self.doc(id).map(|d| d.search(&query, first)));
            }
            Request::Annotations(id, page, reply) => {
                let _ = reply.send(self.doc(id).and_then(|d| d.annotations(page)));
            }
            Request::AnnotationsFrom(id, first, reply) => {
                let _ = reply.send(self.doc(id).map(|d| d.annotations_from(first)));
            }
            Request::AddAnnotation(id, page, annotation, reply) => {
                let result = self
                    .doc(id)
                    .and_then(|d| d.add_annotation(page, &annotation));
                self.page_changed(id, page, result.is_ok());
                let _ = reply.send(result);
            }
            Request::UpdateAnnotation(id, page, annotation, reply) => {
                let result = self
                    .doc(id)
                    .and_then(|d| d.update_annotation(page, &annotation));
                self.page_changed(id, page, result.is_ok());
                let _ = reply.send(result);
            }
            Request::AddImageStamp(id, page, stamp, reply) => {
                let (annotation, image) = *stamp;
                let result = self
                    .doc(id)
                    .and_then(|d| d.add_image_stamp(page, &annotation, &image));
                self.page_changed(id, page, result.is_ok());
                let _ = reply.send(result);
            }
            Request::Redact(id, page, areas, reply) => {
                let result = self.doc(id).and_then(|d| d.redact(page, &areas));
                self.page_changed(id, page, result.is_ok());
                let _ = reply.send(result);
            }
            Request::RestoreAnnotation(id, page, annotation, reply) => {
                let result = self
                    .doc(id)
                    .and_then(|d| d.restore_annotation(page, &annotation));
                self.page_changed(id, page, result.is_ok());
                let _ = reply.send(result);
            }
            Request::SetAnnotationHidden(id, page, annotation, hidden, reply) => {
                let result = self
                    .doc(id)
                    .and_then(|d| d.set_annotation_hidden(page, &annotation, hidden));
                self.page_changed(id, page, result.is_ok());
                let _ = reply.send(result);
            }
            Request::DeleteAnnotation(id, page, annotation, reply) => {
                let result = self
                    .doc(id)
                    .and_then(|d| d.delete_annotation(page, &annotation));
                self.page_changed(id, page, result.is_ok());
                let _ = reply.send(result);
            }
            Request::ChangeField(id, page, field, change, reply) => {
                let result = self
                    .doc(id)
                    .and_then(|d| d.change_field(page, field, &change));
                if result.is_ok() {
                    // Field appearances changed; drop stale renders of this document.
                    self.cache.remove_doc(id);
                }
                let _ = reply.send(result);
            }
            Request::Save(id, path, reply) => {
                let result = match self.docs.get_mut(&id) {
                    Some(doc) => doc.save(&path),
                    None => Err(Error::new(
                        ErrorCode::DocumentNotOpen,
                        format!("document {id}"),
                    )),
                };
                let _ = reply.send(result);
            }
            Request::Print(id, settings, printer_settings, reply) => {
                let result = self
                    .doc(id)
                    .and_then(|d| d.print(&settings, printer_settings.as_deref()));
                let _ = reply.send(result);
            }
            Request::Properties(id, reply) => {
                let _ = reply.send(self.doc(id).map(Document::properties));
            }
            Request::SetMetadata(id, metadata, reply) => {
                let result = match self.docs.get_mut(&id) {
                    Some(doc) => doc.set_metadata(metadata),
                    None => Err(Error::new(
                        ErrorCode::DocumentNotOpen,
                        format!("document {id}"),
                    )),
                };
                let _ = reply.send(result);
            }
            Request::SetVisible(id, first, last) => {
                self.visible.insert(id, (first.min(last), first.max(last)));
            }
            Request::Close(id) => {
                self.docs.remove(&id);
                self.visible.remove(&id);
                self.cache.remove_doc(id);
                self.snapshots.forget_doc(id);
            }
            Request::Task(work) => work(self),
            Request::Render(_) => unreachable!("renders are handled separately"),
        }
    }

    fn render(&mut self, r: &RenderRequest) -> Result<RenderedPage> {
        if let Some(&(first, last)) = self
            .visible
            .get(&r.doc)
            .filter(|_| r.purpose == Purpose::View)
        {
            let near = r.page + VISIBLE_MARGIN >= first && r.page <= last + VISIBLE_MARGIN;
            if !near {
                return Err(Error::new(
                    ErrorCode::Cancelled,
                    "page scrolled out of view",
                ));
            }
        }
        if r.purpose == Purpose::Print {
            return self.doc(r.doc)?.render_page(r.page, r.scale, r.rotation);
        }
        let key = Key::new(r.doc, r.page, r.scale, r.rotation);
        if let Some(page) = self.cache.get(&key) {
            return Ok(page);
        }
        if r.purpose == Purpose::Cached {
            return Err(Error::new(ErrorCode::Cancelled, "not rendered yet"));
        }
        let page = self.doc(r.doc)?.render_page(r.page, r.scale, r.rotation)?;
        self.cache.insert(key, page.clone());
        Ok(page)
    }

    /// Drops stale renders of a page whose annotations changed.
    fn page_changed(&mut self, doc: DocId, page: u32, changed: bool) {
        if changed {
            self.cache.remove_page(doc, page);
        }
    }

    fn doc(&self, id: DocId) -> Result<&Document> {
        self.docs.get(&id).ok_or_else(|| not_open(id))
    }

    fn doc_mut(&mut self, id: DocId) -> Result<&mut Document> {
        self.docs.get_mut(&id).ok_or_else(|| not_open(id))
    }
}
