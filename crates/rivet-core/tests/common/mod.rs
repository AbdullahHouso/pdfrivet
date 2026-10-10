//! Helpers shared by the integration tests of the page tools.
//! Run `cargo xtask fetch-pdfium` once before running these.

#![allow(dead_code)] // Each test file uses a different part.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rivet_core::{Document, Pdf, TextRange};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fixture(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(name)
}

/// PDFium must not be used from several threads at once; each test holds this.
pub fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// PDFium can only be bound once per process, so all tests share one [`Pdf`].
pub fn pdf() -> &'static Pdf {
    static PDF: OnceLock<Pdf> = OnceLock::new();
    PDF.get_or_init(|| {
        let dir = std::env::var_os("RIVET_PDFIUM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                repo_root()
                    .join("vendor/pdfium")
                    .join(rivet_core::pdfium_platform())
            });
        Pdf::load(&dir).expect("PDFium not found: run `cargo xtask fetch-pdfium`")
    })
}

/// The text of one page.
pub fn page_text(doc: &Document, page: u32) -> String {
    doc.text(TextRange {
        start_page: page,
        start: 0,
        end_page: page,
        end: u32::MAX,
    })
    .unwrap()
}

/// A fresh folder for a test's output files.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rivet-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
