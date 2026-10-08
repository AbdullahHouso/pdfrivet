//! Integration tests against the PDFs in `tests/fixtures/`.
//! Run `cargo xtask fetch-pdfium` once before running these.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rivet_core::{
    CHAR_GENERATED, CHAR_NO_BOX, Engine, ErrorCode, FieldChange, FieldKind, FormField, LinkTarget,
    OutlineItem, PageText, Pdf, Rotation, SearchQuery, TextRange,
};

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

fn field<'a>(fields: &'a [FormField], name: &str) -> &'a FormField {
    fields
        .iter()
        .find(|f| f.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no field {name}"))
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rivet-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn reads_form_fields() {
    let _serial = serial();
    let doc = pdf().open(&fixture("forms.pdf"), None).unwrap();
    let fields = doc.form_fields(0).unwrap();

    assert!(
        matches!(field(&fields, "name").field, FieldKind::Text { ref value, .. } if value.is_empty())
    );
    assert_eq!(
        field(&fields, "agree").field,
        FieldKind::Checkbox { checked: false }
    );
    let radios: Vec<_> = fields
        .iter()
        .filter(|f| f.name.as_deref() == Some("colour"))
        .collect();
    assert_eq!(radios.len(), 2);
    assert_eq!(radios[0].field, FieldKind::Radio { checked: true });
    match &field(&fields, "country").field {
        FieldKind::Choice { options, selected } => {
            assert_eq!(options, &["Saudi Arabia", "Egypt", "Jordan"]);
            assert_eq!(*selected, Some(1));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(field(&fields, "locked").read_only);
    // Fields sit in the upper part of the page, left of centre.
    let name = field(&fields, "name");
    assert!(name.left < name.right && name.top < name.bottom && name.bottom < 0.25);
}

#[test]
fn fills_in_and_saves_a_form() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("forms.pdf"), None).unwrap();
    let fields = doc.form_fields(0).unwrap();
    let index = |name: &str| field(&fields, name).index;
    let blue = fields
        .iter()
        .filter(|f| f.name.as_deref() == Some("colour"))
        .nth(1)
        .unwrap()
        .index;

    let text = "Rivet ريفت 123";
    doc.change_field(0, index("name"), &FieldChange::Text { value: text.into() })
        .unwrap();
    doc.change_field(0, index("agree"), &FieldChange::Toggle)
        .unwrap();
    doc.change_field(0, blue, &FieldChange::Toggle).unwrap();
    doc.change_field(0, index("country"), &FieldChange::Select { option: 2 })
        .unwrap();
    let locked = doc.change_field(0, index("locked"), &FieldChange::Text { value: "x".into() });
    assert_eq!(locked.err().map(|e| e.code), Some(ErrorCode::ReadOnlyField));

    // Save, reopen, and check every value survived.
    let dir = temp_dir("forms");
    let out = dir.join("filled.pdf");
    doc.save(&out).unwrap();
    // RIVET_KEEP_OUTPUT=<path> keeps a copy for looking at it.
    if let Some(keep) = std::env::var_os("RIVET_KEEP_OUTPUT") {
        std::fs::copy(&out, keep).unwrap();
    }
    let reopened = pdf().open(&out, None).unwrap();
    let after = reopened.form_fields(0).unwrap();
    assert!(
        matches!(field(&after, "name").field, FieldKind::Text { ref value, .. } if value == text)
    );
    assert_eq!(
        field(&after, "agree").field,
        FieldKind::Checkbox { checked: true }
    );
    let colours: Vec<_> = after
        .iter()
        .filter(|f| f.name.as_deref() == Some("colour"))
        .map(|f| f.field.clone())
        .collect();
    assert_eq!(
        colours,
        vec![
            FieldKind::Radio { checked: false },
            FieldKind::Radio { checked: true }
        ]
    );
    assert!(matches!(
        field(&after, "country").field,
        FieldKind::Choice {
            selected: Some(2),
            ..
        }
    ));
    // No temporary file is left behind.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saves_over_the_open_file() {
    let _serial = serial();
    let dir = temp_dir("overwrite");
    let path = dir.join("form.pdf");
    std::fs::copy(fixture("forms.pdf"), &path).unwrap();
    let mut doc = pdf().open(&path, None).unwrap();
    let agree = field(&doc.form_fields(0).unwrap(), "agree").index;
    doc.change_field(0, agree, &FieldChange::Toggle).unwrap();
    // The file is open in Rivet and is replaced in place (this fails on
    // Windows if the file is still held open).
    doc.save(&path).unwrap();
    let reopened = pdf().open(&path, None).unwrap();
    assert_eq!(
        field(&reopened.form_fields(0).unwrap(), "agree").field,
        FieldKind::Checkbox { checked: true }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_reading_direction() {
    let _serial = serial();
    assert!(
        pdf()
            .open(&fixture("arabic.pdf"), None)
            .unwrap()
            .info()
            .unwrap()
            .rtl
    );
    assert!(
        !pdf()
            .open(&fixture("basic.pdf"), None)
            .unwrap()
            .info()
            .unwrap()
            .rtl
    );
}

#[test]
fn reads_and_changes_document_properties() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let props = doc.properties();
    assert_eq!(props.metadata.title, "Rivet fixture: basic");
    assert_eq!(props.page_count, 3);
    assert_eq!(props.file_name, "basic.pdf");
    assert!(props.file_size > 1000 && !props.encrypted && props.can_edit_metadata);
    assert!(props.created.is_some(), "Typst writes a creation date");

    let meta = rivet_core::metadata::Metadata {
        title: "عنوان جديد".into(),
        author: "Rivet".into(),
        subject: "Test".into(),
        keywords: "a, b".into(),
    };
    doc.set_metadata(meta.clone()).unwrap();
    assert_eq!(doc.info().unwrap().title.as_deref(), Some("عنوان جديد"));

    let dir = temp_dir("props");
    let out = dir.join("renamed.pdf");
    doc.save(&out).unwrap();
    if let Some(keep) = std::env::var_os("RIVET_KEEP_OUTPUT") {
        std::fs::copy(&out, keep).unwrap();
    }
    let reopened = pdf().open(&out, None).unwrap().properties();
    assert_eq!(reopened.metadata, meta);
    assert_eq!(reopened.page_count, 3);
    assert!(reopened.modified.is_some());
    let _ = std::fs::remove_dir_all(&dir);

    let protected = pdf().open(&fixture("password.pdf"), Some("rivet")).unwrap();
    let props = protected.properties();
    assert!(props.encrypted && !props.can_edit_metadata);
}

#[test]
fn reopens_to_release_memory_and_keeps_rendering() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let before = doc.render_page(1, 0.5, Rotation::None).unwrap();
    assert!(!doc.should_release_memory());
    // Showing many pages makes PDFium's caches worth releasing.
    for i in 0..120 {
        doc.render_page(i % 3, 0.2, Rotation::None).unwrap();
    }
    assert!(doc.should_release_memory());
    doc.release_memory().unwrap();
    assert!(!doc.should_release_memory());
    let after = doc.render_page(1, 0.5, Rotation::None).unwrap();
    assert_eq!((after.width, after.height), (before.width, before.height));
    assert_eq!(after.rgba, before.rgba);
}

#[test]
fn keeps_unsaved_form_changes_when_releasing_memory() {
    let _serial = serial();
    let dir = temp_dir("release");
    let mut doc = pdf().open(&fixture("forms.pdf"), None).unwrap();
    let agree = field(&doc.form_fields(0).unwrap(), "agree").index;
    doc.change_field(0, agree, &FieldChange::Toggle).unwrap();
    for _ in 0..120 {
        doc.render_page(0, 0.2, Rotation::None).unwrap();
    }
    // The ticked box exists only inside PDFium, so the document must stay open.
    assert!(!doc.should_release_memory());
    doc.release_memory().unwrap();
    let checked =
        |doc: &rivet_core::Document| field(&doc.form_fields(0).unwrap(), "agree").field.clone();
    assert_eq!(checked(&doc), FieldKind::Checkbox { checked: true });

    // Once saved, reopening is safe and the tick survives it.
    doc.save(&dir.join("saved.pdf")).unwrap();
    assert!(doc.should_release_memory());
    doc.release_memory().unwrap();
    assert_eq!(checked(&doc), FieldKind::Checkbox { checked: true });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engine_answers_cache_only_requests_without_rendering() {
    let _serial = serial();
    let engine = Engine::start(&pdfium_dir()).unwrap_or_else(|e| panic!("{e}"));
    let (doc, _) = engine.open(&fixture("basic.pdf"), None).unwrap();
    let missing = engine.cached(doc, 0, 0.5, Rotation::None).err().unwrap();
    assert_eq!(missing.code, ErrorCode::Cancelled);
    let rendered = engine.render(doc, 0, 0.5, Rotation::None).unwrap();
    let cached = engine.cached(doc, 0, 0.5, Rotation::None).unwrap();
    assert_eq!(cached.rgba, rendered.rgba);
    // A different size is a different render.
    assert!(engine.cached(doc, 0, 0.75, Rotation::None).is_err());
    engine.close(doc);
}

/// The characters of a page as a string (one `char` per character index).
fn page_string(text: &PageText) -> String {
    text.chars
        .iter()
        .map(|c| char::from_u32(c.code).unwrap_or('\u{fffd}'))
        .collect()
}

#[test]
fn reads_character_boxes() {
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let text = doc.page_text(1).unwrap();
    let all = page_string(&text);
    let start = all.find("rivet-needle").expect("needle on page 2");
    // `find` counts bytes; the text before the needle is ASCII, so it matches the char index.
    let needle = &text.chars[start..start + "rivet-needle".len()];

    // Boxes are fractions of the page, left to right on one line.
    for pair in needle.windows(2) {
        assert!(pair[0].left < pair[1].left, "{pair:?}");
        assert!((pair[0].top - pair[1].top).abs() < 0.01);
    }
    for c in needle {
        assert_eq!(c.flags & (CHAR_NO_BOX | CHAR_GENERATED), 0);
        assert!(c.left > 0.0 && c.right < 1.0 && c.top > 0.0 && c.bottom < 0.5);
        assert!(c.bottom - c.top > 0.005, "boxes are line-tall: {c:?}");
    }
    // PDFium adds line breaks between lines; they are marked as generated.
    assert!(
        text.chars
            .iter()
            .any(|c| c.code == '\n' as u32 && c.flags & CHAR_GENERATED != 0)
    );
}

#[test]
fn copies_text_ranges() {
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let all = page_string(&doc.page_text(1).unwrap());
    let start = all.find("rivet-needle").unwrap() as u32;
    let one_page = TextRange {
        start_page: 1,
        start,
        end_page: 1,
        end: start + 12,
    };
    assert_eq!(doc.text(one_page).unwrap(), "rivet-needle");

    // Across pages: the end of page 1, all of page 2, the start of page 3.
    let across = TextRange {
        start_page: 0,
        start: 0,
        end_page: 2,
        end: 4,
    };
    let copied = doc.text(across).unwrap();
    assert!(copied.starts_with("Page one"), "{copied}");
    assert!(copied.contains("lazy dog.\nPage two"), "{copied}");
    assert!(copied.contains("rivet-needle."), "{copied}");
    assert!(copied.ends_with("\nPage"), "{copied}");
    assert!(!copied.contains('\r'));

    // An end past the last character means "to the end of the page".
    let rest = TextRange {
        start_page: 1,
        start,
        end_page: 1,
        end: u32::MAX,
    };
    assert_eq!(doc.text(rest).unwrap(), "rivet-needle.");
}

#[test]
fn reads_arabic_text_in_reading_order() {
    let _serial = serial();
    let doc = pdf().open(&fixture("arabic.pdf"), None).unwrap();
    let text = doc.page_text(0).unwrap();
    let copied = doc
        .text(TextRange {
            start_page: 0,
            start: 0,
            end_page: 0,
            end: text.chars.len() as u32,
        })
        .unwrap();
    // Logical (typed) order, not the visual right-to-left order of the glyphs.
    assert!(copied.contains("مرحبا بكم"), "{copied}");
    assert!(copied.contains("Rivet"), "{copied}");
    // Right-to-left: in "مرحبا" each letter sits left of the one before it.
    let all = page_string(&text);
    let m = all.chars().position(|c| c == 'م').unwrap();
    assert!(text.chars[m + 1].right <= text.chars[m].left + 0.002);
}

#[test]
fn engine_returns_page_text() {
    let _serial = serial();
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (id, _) = engine.open(&fixture("basic.pdf"), None).unwrap();
    let text = engine.page_text(id, 0).unwrap();
    assert!(page_string(&text).contains("quick brown fox"));
    let bytes = text.to_bytes();
    assert_eq!(bytes.len(), 8 + text.chars.len() * PageText::BYTES_PER_CHAR);
}

fn search_query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        match_case: false,
        whole_word: false,
    }
}

#[test]
fn finds_text() {
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let batch = doc.search(&search_query("RIVET-needle"), 0);
    assert_eq!(batch.next_page, None);
    assert_eq!(batch.hits.len(), 1);
    let hit = &batch.hits[0];
    assert_eq!(hit.page, 1);
    assert_eq!(hit.text, "rivet-needle");
    assert!(hit.before.ends_with("text: "), "{hit:?}");
    // The match is a range of characters, like a selection.
    let copied = doc
        .text(TextRange {
            start_page: 1,
            start: hit.start,
            end_page: 1,
            end: hit.end,
        })
        .unwrap();
    assert_eq!(copied, "rivet-needle");

    // "Page" is on every page; whole words and case are honoured.
    assert_eq!(doc.search(&search_query("page"), 0).hits.len(), 3);
    let exact = SearchQuery {
        match_case: true,
        ..search_query("page")
    };
    assert!(doc.search(&exact, 0).hits.is_empty());
    let word = SearchQuery {
        whole_word: true,
        ..search_query("quic")
    };
    assert!(doc.search(&word, 0).hits.is_empty());
}

#[test]
fn finds_arabic_text_loosely() {
    let _serial = serial();
    let doc = pdf().open(&fixture("arabic.pdf"), None).unwrap();
    let count = |q: &str| doc.search(&search_query(q), 0).hits.len();
    assert_eq!(count("مرحبا"), 1);
    // Typed with a diacritic, without the hamza, or with Western digits.
    assert_eq!(count("مَرحبا"), 1);
    assert_eq!(count("وارقام"), 1);
    assert_eq!(count("123"), 1);
    // A word with the lam-alef ligature (PDFium reverses its two letters; we fix that).
    assert_eq!(count("للاختبار"), 1);
}

#[test]
fn copies_lam_alef_ligatures_in_order() {
    let _serial = serial();
    let doc = pdf().open(&fixture("arabic.pdf"), None).unwrap();
    let all = doc
        .text(TextRange {
            start_page: 0,
            start: 0,
            end_page: 0,
            end: u32::MAX,
        })
        .unwrap();
    assert!(all.contains("ملف PDF للاختبار يحتوي"), "{all}");
}

#[test]
fn engine_searches_in_batches() {
    let _serial = serial();
    let dir = temp_dir("search-batches");
    let path = dir.join("many.pdf");
    pdf().write_test_document(&path, 120).unwrap();
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (id, _) = engine.open(&path, None).unwrap();
    let query = search_query("of 120");
    let mut hits = Vec::new();
    let mut next = Some(0);
    let mut batches = 0;
    while let Some(first) = next {
        let batch = engine.search(id, query.clone(), first).unwrap();
        hits.extend(batch.hits);
        next = batch.next_page;
        batches += 1;
    }
    assert_eq!(hits.len(), 120);
    assert!(
        batches > 1,
        "long documents are searched in several batches"
    );
    let pages: Vec<u32> = hits.iter().map(|h| h.page).collect();
    assert_eq!(pages, (0..120).collect::<Vec<_>>());
}
