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
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use crate::{
    DocInfo, Document, Error, ErrorCode, FieldChange, FormField, OutlineItem, PageLink, Pdf,
    RenderedPage, Result, Rotation,
    cache::{Key, RenderCache},
};

/// Identifies an open document inside an [`Engine`].
pub type DocId = u32;

/// Memory budget for already-rendered pages kept by the engine.
const CACHE_BUDGET_BYTES: usize = 48 * 1024 * 1024;

/// Pages this far outside the visible range are not rendered.
const VISIBLE_MARGIN: u32 = 2;

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
    Outline(DocId, Reply<Vec<OutlineItem>>),
    Links(DocId, u32, Reply<Vec<PageLink>>),
    FormFields(DocId, u32, Reply<Vec<FormField>>),
    ChangeField(DocId, u32, u32, FieldChange, Reply<()>),
    Save(DocId, PathBuf, Reply<()>),
    Render(RenderRequest),
    SetVisible(DocId, u32, u32),
    Close(DocId),
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

    pub fn outline(&self, doc: DocId) -> Result<Vec<OutlineItem>> {
        self.call(|reply| Request::Outline(doc, reply))
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

    /// Tells the engine which pages (inclusive, 0-based) are on screen.
    pub fn set_visible_pages(&self, doc: DocId, first: u32, last: u32) {
        let _ = self.tx.send(Request::SetVisible(doc, first, last));
    }

    pub fn close(&self, doc: DocId) {
        let _ = self.tx.send(Request::Close(doc));
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
}

impl Worker {
    fn new(pdf: Pdf) -> Self {
        Self {
            pdf,
            docs: HashMap::new(),
            visible: HashMap::new(),
            cache: RenderCache::new(CACHE_BUDGET_BYTES),
            next_id: 1,
        }
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
        }
    }

    fn handle(&mut self, request: Request) {
        match request {
            Request::Open(path, password, reply) => {
                let result = self.pdf.open(&path, password.as_deref()).and_then(|doc| {
                    let info = doc.info()?;
                    let id = self.next_id;
                    self.next_id += 1;
                    self.docs.insert(id, doc);
                    Ok((id, info))
                });
                let _ = reply.send(result);
            }
            Request::Outline(id, reply) => {
                let _ = reply.send(self.doc(id).map(Document::outline));
            }
            Request::Links(id, page, reply) => {
                let _ = reply.send(self.doc(id).and_then(|d| d.links(page)));
            }
            Request::FormFields(id, page, reply) => {
                let _ = reply.send(self.doc(id).and_then(|d| d.form_fields(page)));
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
                let _ = reply.send(self.doc(id).and_then(|d| d.save(&path)));
            }
            Request::SetVisible(id, first, last) => {
                self.visible.insert(id, (first.min(last), first.max(last)));
            }
            Request::Close(id) => {
                self.docs.remove(&id);
                self.visible.remove(&id);
                self.cache.remove_doc(id);
            }
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
        let page = self.doc(r.doc)?.render_page(r.page, r.scale, r.rotation)?;
        self.cache.insert(key, page.clone());
        Ok(page)
    }

    fn doc(&self, id: DocId) -> Result<&Document> {
        self.docs
            .get(&id)
            .ok_or_else(|| Error::new(ErrorCode::DocumentNotOpen, format!("document {id}")))
    }
}
