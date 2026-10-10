//! Flattening: annotations and form fields become part of the pages' drawing,
//! so they look the same everywhere and can no longer be changed.
//!
//! PDFium's own flattening adds each one's appearance to the page content
//! without rewriting what's already there (rewriting it would lose colours;
//! see AGENTS.md). Hidden annotations, like ones deleted in PDFRivet that are
//! kept for undo, are left out.

#![allow(unsafe_code)]

use crate::{Document, Error, ErrorCode, Result, document::RawBindings};

/// PDFium's `FLAT_NORMALDISPLAY`: flatten what shows on screen.
const FLAT_NORMAL_DISPLAY: i32 = 0;
const FLATTEN_FAIL: i32 = 0;
const FLATTEN_SUCCESS: i32 = 1;

impl Document {
    /// Flattens every page's annotations and form fields. Returns how many
    /// pages had something to flatten.
    pub fn flatten(&mut self) -> Result<u32> {
        let rights = self.rights();
        if !rights.modify && !rights.annotate {
            return Err(Error::new(
                ErrorCode::AnnotateNotAllowed,
                "document permissions",
            ));
        }
        let bindings = self.pdfium.bindings();
        let mut changed = 0;
        for index in 0..self.page_count() {
            let page = self.load_page(index)?;
            let handle = bindings.get_handle_from_page(&page);
            // SAFETY: `handle` belongs to `page`, which lives until the end of
            // this iteration; we're on the PDFium thread.
            match unsafe { bindings.FPDFPage_Flatten(handle, FLAT_NORMAL_DISPLAY) } {
                FLATTEN_FAIL => {
                    return Err(Error::new(
                        ErrorCode::Internal,
                        format!("flattening page {index}"),
                    ));
                }
                FLATTEN_SUCCESS => changed += 1,
                _ => {}
            }
        }
        if changed > 0 {
            // The form's field list now points at nothing: saving drops it.
            self.forms_flattened.set(true);
            self.needs_prune.set(true);
            self.unsaved_changes.set(true);
            // PDFium asks for pages to be loaded again after flattening; reopening
            // the document from its current bytes does that for every page.
            let now = self.snapshot()?;
            self.restore(now)?;
        }
        Ok(changed)
    }
}
