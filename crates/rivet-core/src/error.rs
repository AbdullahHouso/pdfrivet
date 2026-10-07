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
    DocumentNotOpen,
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
