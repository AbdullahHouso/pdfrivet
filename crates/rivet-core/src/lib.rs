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
mod compress;
mod direction;
mod document;
mod engine;
mod error;
mod flatten;
pub mod fonts;
mod forms;
mod geometry;
mod images;
mod incremental;
mod links;
mod merge;
pub mod metadata;
mod outline;
mod pages;
pub mod print;
mod protect;
mod prune;
mod redact;
mod repair;
mod rights;
mod search;
mod text;
mod textlayout;
mod textstrings;

pub use annotations::{
    Annotation, AnnotationBatch, AnnotationKind, Color, MarkupStyle, PageAnnotations, PagePoint,
    PageRect, StampImage,
};
pub use compress::{CompressLevel, CompressReport, compress};
pub use document::{DocInfo, Document, PageSize, Pdf, RenderedPage, Rotation, pdfium_platform};
pub use engine::{DocId, Engine, SnapshotId};
pub use error::{Error, ErrorCode, Result};
pub use fonts::{FontInfo, TextFont};
pub use forms::{FieldChange, FieldKind, FormField};
pub use images::{ImageFormat, ImageLayout, PagePaper, save_image};
pub use links::{LinkTarget, PageLink};
pub use merge::MergePart;
pub use outline::OutlineItem;
pub use pages::{PageSlot, PageSource, Snapshot};
pub use protect::{Allowed, Protection};
pub use repair::RepairReport;
pub use search::{SearchBatch, SearchHit, SearchQuery};
pub use text::{CHAR_GENERATED, CHAR_NO_BOX, PageText, TextChar, TextRange};
pub use textlayout::{
    LINE_HEIGHT, TEXT_PADDING, TextAlign, TextBoxSize, TextDirection, TextStyle, VerticalAlign,
    measure as measure_text,
};
