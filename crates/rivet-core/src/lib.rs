//! # rivet-core
//!
//! All of Rivet's PDF logic lives here, with no UI code. The desktop app and
//! `rivet-cli` are thin layers on top of this crate.
//!
//! - [`Pdf`] loads the PDFium library and opens documents.
//! - [`Document`] is one open PDF (page sizes, rendering, metadata).
//! - [`Engine`] runs PDFium on a dedicated worker thread, so it can be
//!   called safely from many threads (the app uses this).

mod document;
mod engine;
mod error;

pub use document::{DocInfo, Document, PageSize, Pdf, RenderedPage, pdfium_platform};
pub use engine::{DocId, Engine};
pub use error::{Error, ErrorCode, Result};
