//! # rivet-core
//!
//! All of Rivet's PDF logic lives here, with no UI code. The desktop app and
//! `rivet-cli` are thin layers on top of this crate.
//!
//! - [`Pdf`] loads the PDFium library and opens documents.
//! - [`Document`] is one open PDF (page sizes, rendering, outline, metadata).
//! - [`Engine`] runs PDFium on a dedicated worker thread, so it can be
//!   called safely from many threads (the app uses this).

mod annotations;
mod cache;
mod direction;
mod document;
mod engine;
mod error;
mod forms;
mod geometry;
mod links;
pub mod metadata;
mod outline;
pub mod print;
mod search;
mod text;

pub use annotations::{
    Annotation, AnnotationKind, Color, MarkupStyle, PagePoint, PageRect, StampImage,
};
pub use document::{DocInfo, Document, PageSize, Pdf, RenderedPage, Rotation, pdfium_platform};
pub use engine::{DocId, Engine};
pub use error::{Error, ErrorCode, Result};
pub use forms::{FieldChange, FieldKind, FormField};
pub use links::{LinkTarget, PageLink};
pub use outline::OutlineItem;
pub use search::{SearchBatch, SearchHit, SearchQuery};
pub use text::{CHAR_GENERATED, CHAR_NO_BOX, PageText, TextChar, TextRange};
