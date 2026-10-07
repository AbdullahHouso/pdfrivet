//! PDFium is not thread-safe, so the app talks to it through an [`Engine`]:
//! one worker thread owns PDFium and every open document, and other threads
//! send it requests over a channel and wait for the reply.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use crate::{DocInfo, Document, Error, ErrorCode, Pdf, RenderedPage, Result};

/// Identifies an open document inside an [`Engine`].
pub type DocId = u32;

type Reply<T> = mpsc::Sender<Result<T>>;

enum Request {
    Open(PathBuf, Option<String>, Reply<(DocId, DocInfo)>),
    Render(DocId, u32, f32, Reply<RenderedPage>),
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
            .spawn(move || {
                let pdf = match Pdf::load(&lib_dir) {
                    Ok(pdf) => {
                        let _ = ready_tx.send(Ok(()));
                        pdf
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                worker(pdf, rx);
            })
            .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?;

        ready_rx
            .recv()
            .map_err(|_| Error::new(ErrorCode::EngineStopped, "worker exited during startup"))??;
        Ok(Self { tx })
    }

    pub fn open(&self, path: &Path, password: Option<String>) -> Result<(DocId, DocInfo)> {
        self.call(|reply| Request::Open(path.to_path_buf(), password, reply))
    }

    pub fn render(&self, doc: DocId, page: u32, scale: f32) -> Result<RenderedPage> {
        self.call(|reply| Request::Render(doc, page, scale, reply))
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

fn worker(pdf: Pdf, rx: mpsc::Receiver<Request>) {
    let mut docs: HashMap<DocId, Document> = HashMap::new();
    let mut next_id: DocId = 1;
    let not_open = |id| Error::new(ErrorCode::DocumentNotOpen, format!("document {id}"));

    // The loop ends when every Engine handle has been dropped.
    for request in rx {
        match request {
            Request::Open(path, password, reply) => {
                let result = pdf.open(&path, password.as_deref()).and_then(|doc| {
                    let info = doc.info()?;
                    let id = next_id;
                    next_id += 1;
                    docs.insert(id, doc);
                    Ok((id, info))
                });
                let _ = reply.send(result);
            }
            Request::Render(id, page, scale, reply) => {
                let result = docs
                    .get(&id)
                    .ok_or_else(|| not_open(id))
                    .and_then(|doc| doc.render_page(page, scale));
                let _ = reply.send(result);
            }
            Request::Close(id) => {
                docs.remove(&id);
            }
        }
    }
}
