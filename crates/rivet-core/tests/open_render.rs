//! Integration tests against the PDFs in `tests/fixtures/`.
//! Run `cargo xtask fetch-pdfium` once before running these.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rivet_core::{Engine, ErrorCode, LinkTarget, OutlineItem, Pdf, Rotation};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(name)
}

/// PDFium keeps some state (like the last error) globally, so it must not be
/// used from several threads at once. Each test holds this lock while it runs.
/// (The app avoids the problem by only calling PDFium from the Engine thread.)
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// PDFium can only be bound once per process, so all tests share one [`Pdf`].
fn pdfium_dir() -> PathBuf {
    std::env::var_os("RIVET_PDFIUM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            repo_root()
                .join("vendor/pdfium")
                .join(rivet_core::pdfium_platform())
        })
}

fn pdf() -> &'static Pdf {
    static PDF: OnceLock<Pdf> = OnceLock::new();
    PDF.get_or_init(|| {
        Pdf::load(&pdfium_dir()).expect("PDFium not found: run `cargo xtask fetch-pdfium`")
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
    let _serial = serial();
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
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let page = doc.render_page(0, 2.0, Rotation::None).unwrap();
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
    let _serial = serial();
    let doc = pdf().open(&fixture("arabic.pdf"), None).unwrap();
    let info = doc.info().unwrap();
    assert_eq!(info.title.as_deref(), Some("ملف اختبار عربي"));
    let page = doc.render_page(0, 1.0, Rotation::None).unwrap();
    assert!(non_white_pixels(&page.rgba) > 100);
}

#[test]
fn reports_clear_errors() {
    let _serial = serial();
    let err = |name: &str| pdf().open(&fixture(name), None).err().unwrap().code;
    assert_eq!(err("does-not-exist.pdf"), ErrorCode::FileNotFound);
    assert_eq!(err("broken.pdf"), ErrorCode::InvalidPdf);

    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let e = doc.render_page(99, 1.0, Rotation::None).err().unwrap();
    assert_eq!(e.code, ErrorCode::PageOutOfRange);
}

#[test]
fn reads_outline() {
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let outline = doc.outline();
    let summary: Vec<(&str, Option<u32>)> = outline
        .iter()
        .map(|i: &OutlineItem| (i.title.as_str(), i.page))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("Page one (A4 portrait)", Some(0)),
            ("Page two (US Letter)", Some(1)),
            ("Page three (A4 landscape)", Some(2)),
        ]
    );
}

#[test]
fn rotation_swaps_width_and_height() {
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let upright = doc.render_page(0, 1.0, Rotation::None).unwrap();
    let turned = doc.render_page(0, 1.0, Rotation::Cw90).unwrap();
    assert_eq!(
        (upright.width, upright.height),
        (turned.height, turned.width)
    );
    assert_eq!(Rotation::from_degrees(270), Rotation::Cw270);
    assert_eq!(Rotation::from_degrees(-90), Rotation::Cw270);
    assert_eq!(Rotation::from_degrees(360), Rotation::None);
}

#[test]
fn opens_password_protected_documents() {
    let _serial = serial();
    let path = fixture("password.pdf");
    let code = |pw: Option<&str>| pdf().open(&path, pw).err().map(|e| e.code);
    assert_eq!(code(None), Some(ErrorCode::PasswordRequired));
    assert_eq!(code(Some("nope")), Some(ErrorCode::WrongPassword));
    let doc = pdf().open(&path, Some("rivet")).unwrap();
    assert_eq!(doc.page_count(), 3);
}

#[test]
fn opens_large_documents_quickly() {
    let _serial = serial();
    let dir = std::env::temp_dir().join(format!("rivet-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("big.pdf");
    pdf().write_test_document(&path, 2000).unwrap();

    let start = std::time::Instant::now();
    let doc = pdf().open(&path, None).unwrap();
    let info = doc.info().unwrap();
    let elapsed = start.elapsed();
    assert_eq!(info.page_count, 2000);
    // Generous limit so slow CI machines pass; locally this takes a few ms.
    assert!(elapsed.as_secs_f32() < 3.0, "opening took {elapsed:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engine_skips_pages_far_from_view() {
    let _serial = serial();
    // The engine uses its own PDFium binding; it shares the global library
    // with the other tests in this process, which PDFium allows.
    let engine = Engine::start(&pdfium_dir()).unwrap_or_else(|e| panic!("{e}"));
    let (doc, info) = engine.open(&fixture("basic.pdf"), None).unwrap();
    assert_eq!(info.page_count, 3);
    engine.set_visible_pages(doc, 0, 0);
    // Page 2 is within the margin, so it renders.
    assert!(engine.render(doc, 2, 0.5, Rotation::None).is_ok());
    assert_eq!(engine.outline(doc).unwrap().len(), 3);
    engine.close(doc);
    let err = engine.render(doc, 0, 0.5, Rotation::None).err().unwrap();
    assert_eq!(err.code, ErrorCode::DocumentNotOpen);
}

#[test]
fn reads_links() {
    let _serial = serial();
    let doc = pdf().open(&fixture("links.pdf"), None).unwrap();
    let links = doc.links(0).unwrap();
    let targets: Vec<&LinkTarget> = links.iter().map(|l| &l.target).collect();
    assert_eq!(
        targets,
        vec![
            &LinkTarget::Page { page: 1 },
            &LinkTarget::Uri {
                uri: "https://example.com".into()
            },
        ]
    );
    // Rectangles are fractions of the page, top-left origin, near the top of page 1.
    let first = &links[0];
    assert!(first.left < first.right && first.top < first.bottom);
    assert!(first.top > 0.0 && first.bottom < 0.3, "{first:?}");
    assert!(doc.links(1).unwrap().is_empty());
}
