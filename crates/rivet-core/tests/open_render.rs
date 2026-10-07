//! Integration tests against the PDFs in `tests/fixtures/`.
//! Run `cargo xtask fetch-pdfium` once before running these.

use std::path::PathBuf;
use std::sync::OnceLock;

use rivet_core::{ErrorCode, Pdf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(name)
}

/// PDFium can only be bound once per process, so all tests share one [`Pdf`].
fn pdf() -> &'static Pdf {
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

/// Counts pixels that are not pure white (i.e. something was drawn).
fn non_white_pixels(rgba: &[u8]) -> usize {
    let (pixels, _) = rgba.as_chunks::<4>();
    pixels
        .iter()
        .filter(|px| px[..3] != [255, 255, 255])
        .count()
}

#[test]
fn opens_and_reads_info() {
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let info = doc.info().unwrap();
    assert_eq!(info.page_count, 3);
    assert_eq!(info.title.as_deref(), Some("Rivet fixture: basic"));

    // A4 portrait, US Letter, A4 landscape (in points, rounded).
    let sizes: Vec<_> = info
        .page_sizes
        .iter()
        .map(|s| (s.width.round() as i32, s.height.round() as i32))
        .collect();
    assert_eq!(sizes, vec![(595, 842), (612, 792), (842, 595)]);
}

#[test]
fn renders_page_at_scale() {
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let page = doc.render_page(0, 2.0).unwrap();
    assert_eq!((page.width, page.height), (1191, 1684));
    assert_eq!(page.rgba.len(), (page.width * page.height * 4) as usize);

    // The page has text on it, so it must not be all white.
    let non_white = non_white_pixels(&page.rgba);
    assert!(
        non_white > 100,
        "page looks blank ({non_white} non-white pixels)"
    );
}

#[test]
fn opens_arabic_document() {
    let doc = pdf().open(&fixture("arabic.pdf"), None).unwrap();
    let info = doc.info().unwrap();
    assert_eq!(info.title.as_deref(), Some("ملف اختبار عربي"));
    let page = doc.render_page(0, 1.0).unwrap();
    assert!(non_white_pixels(&page.rgba) > 100);
}

#[test]
fn reports_clear_errors() {
    let err = |name: &str| pdf().open(&fixture(name), None).err().unwrap().code;
    assert_eq!(err("does-not-exist.pdf"), ErrorCode::FileNotFound);
    assert_eq!(err("broken.pdf"), ErrorCode::InvalidPdf);

    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let e = doc.render_page(99, 1.0).err().unwrap();
    assert_eq!(e.code, ErrorCode::PageOutOfRange);
}
