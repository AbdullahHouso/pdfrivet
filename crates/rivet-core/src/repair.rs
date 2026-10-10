//! Repairing damaged PDFs.
//!
//! A damaged file usually still has its pages; what breaks is the table that
//! says where each object is (the cross-reference table), or the file's end.
//! PDFium rebuilds that table by scanning the file when it's broken, so
//! opening and saving with it repairs most files. When PDFium can't make sense
//! of a file at all, lopdf's reader (which scans for objects in its own way)
//! rebuilds it, and PDFium then saves that.

use serde::Serialize;
use ts_rs::TS;

use crate::{Error, ErrorCode, Pdf, Result, document::write_atomically};

/// What a repair recovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct RepairReport {
    /// Pages in the repaired file.
    pub pages: u32,
    /// The file had to be rebuilt object by object (it was badly damaged).
    pub rebuilt: bool,
}

impl Pdf {
    /// Repairs the PDF at `from` and writes the result to `to`.
    pub fn repair(
        &self,
        from: &std::path::Path,
        to: &std::path::Path,
        password: Option<&str>,
    ) -> Result<RepairReport> {
        let bytes = std::fs::read(from).map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
        let (saved, rebuilt) = match self.resave(bytes.clone(), password) {
            Ok(saved) => (saved, false),
            Err(e)
                if matches!(
                    e.code,
                    ErrorCode::PasswordRequired | ErrorCode::WrongPassword
                ) =>
            {
                return Err(e);
            }
            Err(_) => {
                let mut doc = lopdf::Document::load_mem(&bytes)
                    .map_err(|e| Error::new(ErrorCode::NotRepairable, e.to_string()))?;
                let mut rebuilt = Vec::new();
                doc.save_to(&mut rebuilt)
                    .map_err(|e| Error::new(ErrorCode::NotRepairable, e.to_string()))?;
                let saved = self
                    .resave(rebuilt, password)
                    .map_err(|e| Error::new(ErrorCode::NotRepairable, e.detail))?;
                (saved, true)
            }
        };
        let pages = self.page_count_of(saved.clone(), password)?;
        if pages == 0 {
            return Err(Error::new(ErrorCode::NotRepairable, "no pages left"));
        }
        write_atomically(to, &saved)?;
        Ok(RepairReport { pages, rebuilt })
    }

    /// Opens bytes with PDFium (which mends what it can) and saves them again.
    fn resave(&self, bytes: Vec<u8>, password: Option<&str>) -> Result<Vec<u8>> {
        let doc = self
            .pdfium()
            .load_pdf_from_byte_vec(bytes, password)
            .map_err(|e| {
                let err = Error::from(e);
                if err.code == ErrorCode::PasswordRequired && password.is_some() {
                    Error::new(ErrorCode::WrongPassword, err.detail)
                } else {
                    err
                }
            })?;
        if doc.pages().is_empty() {
            return Err(Error::new(ErrorCode::NotRepairable, "no pages"));
        }
        Ok(doc.save_to_bytes()?)
    }

    fn page_count_of(&self, bytes: Vec<u8>, password: Option<&str>) -> Result<u32> {
        let doc = self.pdfium().load_pdf_from_byte_vec(bytes, password)?;
        Ok(doc.pages().len().max(0) as u32)
    }
}
