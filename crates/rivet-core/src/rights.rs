//! What a document's author allows (its permissions), read straight from
//! PDFium. pdfium-render's own checks don't know the AES-256 security handler
//! (revisions 5 and 6) and fail for those files, which would let everything
//! through; PDFium itself handles every revision.

#![allow(unsafe_code)]

use pdfium_render::prelude::*;

use crate::document::RawBindings;

/// The permissions in effect for the open document (all of them when it isn't
/// protected, or was opened with its owner password).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rights {
    /// The file is encrypted (it has a security handler).
    pub encrypted: bool,
    /// Every permission is granted.
    pub full: bool,
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    /// Insert, delete and rotate pages, create bookmarks.
    pub assemble: bool,
}

/// Bits of the permissions value (PDF 32000-1, table 22), numbered from 1.
fn bit(value: u32, n: u32) -> bool {
    value & (1 << (n - 1)) != 0
}

pub(crate) fn read(pdfium: &Pdfium, doc: &PdfDocument) -> Rights {
    let bindings = pdfium.bindings();
    let handle = bindings.get_handle_from_document(doc);
    // SAFETY: `handle` belongs to the live document `doc`, borrowed for the
    // whole call; we're on the PDFium thread.
    let (revision, value) = unsafe {
        (
            bindings.FPDF_GetSecurityHandlerRevision(handle),
            bindings.FPDF_GetDocPermissions(handle) as u32,
        )
    };
    if revision < 0 {
        return Rights {
            encrypted: false,
            full: true,
            print: true,
            modify: true,
            copy: true,
            annotate: true,
            fill_forms: true,
            assemble: true,
        };
    }
    let modify = bit(value, 4);
    let annotate = bit(value, 6);
    // Revision 2 has no separate bits for filling forms and assembling.
    let (fill_forms, assemble) = if revision == 2 {
        (annotate, modify)
    } else {
        (annotate || bit(value, 9), modify || bit(value, 11))
    };
    Rights {
        encrypted: true,
        // PDFium grants everything when the owner password was given (all bits
        // but the two lowest, which are reserved and always clear).
        full: value | 0b11 == u32::MAX,
        print: bit(value, 3),
        modify,
        copy: bit(value, 5),
        annotate,
        fill_forms,
        assemble,
    }
}

#[cfg(test)]
mod tests {
    use super::bit;

    #[test]
    fn reads_bits_numbered_from_one() {
        // -3376 as written by lopdf for "copy only": bits 5, 7, 8, 10 and up.
        let value = -3376_i32 as u32;
        assert!(bit(value, 5) && bit(value, 10));
        assert!(
            !bit(value, 3) && !bit(value, 4) && !bit(value, 6) && !bit(value, 9) && !bit(value, 11)
        );
    }
}
