use serde::Serialize;
use ts_rs::TS;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Stable error codes. The UI turns these into translated messages, so no
/// user-facing English text ever comes from Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ErrorCode {
    LibraryNotFound,
    FileNotFound,
    PasswordRequired,
    WrongPassword,
    InvalidPdf,
    PageOutOfRange,
    /// The form field can't be changed.
    ReadOnlyField,
    /// Saving failed (disk full, no permission, file locked…).
    SaveFailed,
    /// The printer or print system reported a problem.
    PrintFailed,
    /// The document's author doesn't allow copying its text.
    CopyNotAllowed,
    /// The document's author doesn't allow adding or changing annotations.
    AnnotateNotAllowed,
    /// The annotation was removed (or never existed).
    AnnotationNotFound,
    /// The annotation can't be changed (another app's kind, or locked).
    ReadOnlyAnnotation,
    /// Redaction isn't possible in password-protected PDFs yet.
    RedactProtected,
    DocumentNotOpen,
    /// The document's author doesn't allow changing its pages (moving,
    /// deleting, inserting, rotating, taking them out into other files).
    AssembleNotAllowed,
    /// An earlier state of the document is no longer kept, so it can't be undone.
    UndoUnavailable,
    /// Changing a protected file's protection needs its owner (permissions) password.
    OwnerPasswordRequired,
    /// The file is too damaged to recover any pages from.
    NotRepairable,
    /// A picture file isn't a kind PDFRivet reads, is damaged, or is too big.
    UnsupportedImage,
    /// Compressing a password-protected file isn't possible (remove the protection first).
    CompressProtected,
    EngineStopped,
    /// A render was skipped because the page is no longer near the screen.
    Cancelled,
    Io,
    Internal,
}

/// An error from rivet-core: a [`ErrorCode`] for the UI plus technical
/// details for logs and bug reports.
#[derive(Debug, thiserror::Error, Serialize)]
#[error("{code:?}: {detail}")]
pub struct Error {
    pub code: ErrorCode,
    pub detail: String,
}

impl Error {
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl From<pdfium_render::prelude::PdfiumError> for Error {
    fn from(err: pdfium_render::prelude::PdfiumError) -> Self {
        use pdfium_render::prelude::{PdfiumError as E, PdfiumInternalError as I};
        let code = match &err {
            E::LoadLibraryError(_) => ErrorCode::LibraryNotFound,
            E::IoError(e) if e.kind() == std::io::ErrorKind::NotFound => ErrorCode::FileNotFound,
            E::IoError(_) => ErrorCode::Io,
            E::PdfiumLibraryInternalError(I::PasswordError) => ErrorCode::PasswordRequired,
            E::PdfiumLibraryInternalError(I::FormatError | I::FileError) => ErrorCode::InvalidPdf,
            _ => ErrorCode::Internal,
        };
        Self::new(code, format!("{err:?}"))
    }
}
