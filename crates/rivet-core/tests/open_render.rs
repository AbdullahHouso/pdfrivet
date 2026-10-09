//! Integration tests against the PDFs in `tests/fixtures/`.
//! Run `cargo xtask fetch-pdfium` once before running these.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rivet_core::{
    Annotation, AnnotationKind, CHAR_GENERATED, CHAR_NO_BOX, Color, Engine, ErrorCode, FieldChange,
    FieldKind, FormField, LinkTarget, MarkupStyle, OutlineItem, PagePoint, PageRect, PageText, Pdf,
    Rotation, SearchQuery, StampImage, TextAlign, TextDirection, TextFont, TextRange, TextStyle,
    VerticalAlign,
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

const RED: Color = Color {
    r: 220,
    g: 40,
    b: 40,
};
const BLUE: Color = Color {
    r: 30,
    g: 90,
    b: 220,
};

fn annotation(kind: AnnotationKind, rect: PageRect) -> Annotation {
    Annotation {
        id: String::new(),
        kind,
        rect,
        color: RED,
        opacity: 1.0,
        width: 2.0,
        contents: String::new(),
        author: "Tester".into(),
        modified: None,
        editable: true,
        reply_to: None,
    }
}

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> PageRect {
    PageRect {
        left,
        top,
        right,
        bottom,
    }
}

fn point(x: f32, y: f32) -> PagePoint {
    PagePoint { x, y }
}

/// One of each kind PDFRivet can add, on page 2 of basic.pdf.
fn sample_annotations(doc: &rivet_core::Document) -> Vec<Annotation> {
    // Highlight "rivet-needle" using the characters' own boxes.
    let text = doc.page_text(1).unwrap();
    let all = page_string(&text);
    let start = all.find("rivet-needle").unwrap();
    let chars = &text.chars[start..start + 12];
    let line = rect(
        chars[0].left,
        chars[0].top,
        chars[11].right,
        chars[0].bottom,
    );
    let mut note = annotation(AnnotationKind::Note, rect(0.8, 0.1, 0.83, 0.13));
    note.contents = "ملاحظة: check this".into();
    let mut square = annotation(
        AnnotationKind::Square { fill: Some(BLUE) },
        rect(0.1, 0.5, 0.3, 0.6),
    );
    square.opacity = 0.5;
    vec![
        annotation(
            AnnotationKind::Markup {
                style: MarkupStyle::Highlight,
                quads: vec![line],
            },
            line,
        ),
        annotation(
            AnnotationKind::Ink {
                strokes: vec![vec![point(0.1, 0.3), point(0.2, 0.35), point(0.3, 0.3)]],
            },
            rect(0.0, 0.0, 0.0, 0.0),
        ),
        square,
        annotation(
            AnnotationKind::Circle { fill: None },
            rect(0.4, 0.5, 0.6, 0.6),
        ),
        annotation(
            AnnotationKind::Line {
                from: point(0.1, 0.7),
                to: point(0.5, 0.75),
                arrow: true,
            },
            rect(0.0, 0.0, 0.0, 0.0),
        ),
        note,
    ]
}

#[test]
fn adds_annotations_that_survive_saving() {
    let _serial = serial();
    let dir = temp_dir("annotations");
    let path = dir.join("annotated.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let before = doc.render_page(1, 0.5, Rotation::None).unwrap();
    let wanted = sample_annotations(&doc);
    let mut ids = Vec::new();
    for a in &wanted {
        ids.push(doc.add_annotation(1, a).unwrap());
    }
    let read = doc.annotations(1).unwrap();
    assert_eq!(read.len(), wanted.len());
    // PDFium draws them (it builds each appearance itself).
    let after = doc.render_page(1, 0.5, Rotation::None).unwrap();
    assert!(non_white_pixels(&after.rgba) > non_white_pixels(&before.rgba) + 500);

    doc.save(&path).unwrap();

    // Other readers need an appearance stream for every annotation.
    let saved = lopdf::Document::load(&path).unwrap();
    let mut with_ap = 0;
    for object in saved.objects.values() {
        if let Ok(dict) = object.as_dict()
            && dict
                .get(b"Subtype")
                .and_then(|s| s.as_name())
                .is_ok_and(|n| matches!(n, b"Highlight" | b"Ink" | b"Square" | b"Circle" | b"Text"))
        {
            assert!(
                dict.has(b"AP"),
                "{:?} has no appearance",
                dict.get(b"Subtype")
            );
            with_ap += 1;
        }
    }
    assert_eq!(with_ap, wanted.len());

    // Reading the saved file gives the same annotations back.
    let reopened = pdf().open(&path, None).unwrap();
    let read = reopened.annotations(1).unwrap();
    assert_eq!(read.len(), wanted.len());
    for (got, want) in read.iter().zip(&wanted) {
        assert!(ids.contains(&got.id), "keeps its id: {}", got.id);
        assert_eq!(got.color, want.color);
        assert_eq!(got.author, "Tester");
        assert!(got.modified.is_some());
        assert!(got.editable);
        match (&got.kind, &want.kind) {
            (AnnotationKind::Markup { quads: a, .. }, AnnotationKind::Markup { quads: b, .. }) => {
                assert_eq!(a.len(), 1);
                assert!(
                    (a[0].left - b[0].left).abs() < 0.002 && (a[0].top - b[0].top).abs() < 0.002
                );
            }
            (AnnotationKind::Ink { strokes: a }, AnnotationKind::Ink { strokes: b }) => {
                assert_eq!(a[0].len(), b[0].len());
                assert!((a[0][1].x - 0.2).abs() < 0.002 && (a[0][1].y - 0.35).abs() < 0.002);
            }
            (AnnotationKind::Line { from, to, arrow }, AnnotationKind::Line { .. }) => {
                assert!(*arrow);
                assert!((from.x - 0.1).abs() < 0.002 && (to.y - 0.75).abs() < 0.002);
            }
            (AnnotationKind::Square { fill }, AnnotationKind::Square { .. }) => {
                assert_eq!(*fill, Some(BLUE));
                assert!((got.opacity - 0.5).abs() < 0.01);
                assert!(
                    (got.rect.left - 0.1).abs() < 0.002 && (got.rect.bottom - 0.6).abs() < 0.002
                );
            }
            (AnnotationKind::Circle { fill }, AnnotationKind::Circle { .. }) => {
                assert_eq!(*fill, None);
            }
            (AnnotationKind::Note, AnnotationKind::Note) => {
                assert_eq!(got.contents, "ملاحظة: check this");
            }
            other => panic!("kind changed: {other:?}"),
        }
    }
}

#[test]
fn changes_and_deletes_annotations() {
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let mut square = annotation(
        AnnotationKind::Square { fill: None },
        rect(0.1, 0.1, 0.2, 0.2),
    );
    square.id = doc.add_annotation(0, &square).unwrap();
    // Render once, so PDFium has built its appearance (changing it must still work).
    doc.render_page(0, 0.3, Rotation::None).unwrap();

    square.color = BLUE;
    square.rect = rect(0.5, 0.5, 0.7, 0.6);
    square.kind = AnnotationKind::Square { fill: Some(RED) };
    assert_eq!(doc.update_annotation(0, &square).unwrap(), square.id);
    let read = &doc.annotations(0).unwrap()[0];
    assert_eq!(read.color, BLUE);
    assert_eq!(read.kind, AnnotationKind::Square { fill: Some(RED) });
    assert!((read.rect.left - 0.5).abs() < 0.002);

    // Removing the fill again.
    square.kind = AnnotationKind::Square { fill: None };
    doc.update_annotation(0, &square).unwrap();
    assert_eq!(
        doc.annotations(0).unwrap()[0].kind,
        AnnotationKind::Square { fill: None }
    );

    doc.delete_annotation(0, &square.id).unwrap();
    assert!(doc.annotations(0).unwrap().is_empty());
    let missing = doc.delete_annotation(0, &square.id).unwrap_err();
    assert_eq!(missing.code, ErrorCode::AnnotationNotFound);
    // Undo brings it back as it was.
    doc.restore_annotation(0, &square.id).unwrap();
    let back = doc.annotations(0).unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].id, square.id);
    assert_eq!(back[0].color, BLUE);
}

#[test]
fn deleted_annotations_leave_the_file_when_saved() {
    let _serial = serial();
    let dir = temp_dir("deleted");
    let path = dir.join("d.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let keep = annotation(AnnotationKind::Note, rect(0.1, 0.1, 0.13, 0.13));
    let gone = annotation(
        AnnotationKind::Square { fill: None },
        rect(0.3, 0.3, 0.5, 0.5),
    );
    doc.add_annotation(0, &keep).unwrap();
    let id = doc.add_annotation(0, &gone).unwrap();
    doc.delete_annotation(0, &id).unwrap();
    let blank = doc.render_page(0, 0.5, Rotation::None).unwrap();
    doc.save(&path).unwrap();
    let saved = lopdf::Document::load(&path).unwrap();
    let squares = saved
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| {
            d.get(b"Subtype")
                .and_then(|s| s.as_name())
                .is_ok_and(|n| n == b"Square")
        })
        .count();
    assert_eq!(squares, 0, "removed from the saved file");
    // Undo still works after saving (it was only left out of the file).
    doc.restore_annotation(0, &id).unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 2);
    doc.delete_annotation(0, &id).unwrap();
    // And it wasn't drawn while waiting to be saved either.
    let reopened = pdf().open(&path, None).unwrap();
    assert_eq!(reopened.annotations(0).unwrap().len(), 1);
    let after = reopened.render_page(0, 0.5, Rotation::None).unwrap();
    assert_eq!(non_white_pixels(&blank.rgba), non_white_pixels(&after.rgba));
}

#[test]
fn moving_keeps_the_original_appearance() {
    let _serial = serial();
    let dir = temp_dir("keep-ap");
    let path = dir.join("k.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let mut ink = annotation(
        AnnotationKind::Ink {
            strokes: vec![vec![point(0.1, 0.5), point(0.3, 0.52), point(0.4, 0.5)]],
        },
        rect(0.0, 0.0, 0.0, 0.0),
    );
    ink.opacity = 0.5;
    ink.width = 10.0;
    ink.id = doc.add_annotation(0, &ink).unwrap();
    let appearance = |doc: &mut rivet_core::Document, path: &std::path::Path| {
        doc.save(path).unwrap();
        let saved = lopdf::Document::load(path).unwrap();
        saved
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter(|s| s.dict.get(b"BBox").is_ok())
            .map(|s| s.decompressed_content().unwrap_or(s.content.clone()))
            .next()
            .unwrap()
    };
    let before = appearance(&mut doc, &path);
    let text = String::from_utf8_lossy(&before);
    assert!(text.contains("1 J 1 j"), "round ends: {text}");
    assert!(text.contains("/GS gs"), "half transparent: {text}");

    // Moved down: same appearance (fitted into the new Rect), new place on the page.
    let mut read = doc.annotations(0).unwrap()[0].clone();
    let dy = 0.2;
    read.rect = PageRect {
        top: read.rect.top + dy,
        bottom: read.rect.bottom + dy,
        ..read.rect
    };
    if let AnnotationKind::Ink { strokes } = &mut read.kind {
        for p in strokes.iter_mut().flatten() {
            p.y += dy;
        }
    }
    doc.update_annotation(0, &read).unwrap();
    assert_eq!(appearance(&mut doc, &path), before, "appearance kept");
    let page = doc.render_page(0, 1.0, Rotation::None).unwrap();
    assert!(
        pixel_at(&page, 0.3, 0.72) != [255, 255, 255],
        "drawn at the new place"
    );
    assert_eq!(
        pixel_at(&page, 0.3, 0.52),
        [255, 255, 255],
        "not at the old one"
    );
}

#[test]
fn keeps_unsaved_annotations_when_releasing_memory() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let note = annotation(AnnotationKind::Note, rect(0.1, 0.1, 0.13, 0.13));
    doc.add_annotation(0, &note).unwrap();
    for _ in 0..120 {
        doc.render_page(2, 0.1, Rotation::None).unwrap();
    }
    assert!(!doc.should_release_memory());
    doc.release_memory().unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
}

#[test]
fn engine_refreshes_annotated_pages() {
    let _serial = serial();
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (id, info) = engine.open(&fixture("basic.pdf"), None).unwrap();
    assert!(info.can_annotate);
    let blank = engine.render(id, 2, 0.5, Rotation::None).unwrap();
    let square = annotation(
        AnnotationKind::Square { fill: Some(RED) },
        rect(0.3, 0.3, 0.7, 0.7),
    );
    engine.add_annotation(id, 2, square).unwrap();
    // Not served from the cache: the new square is drawn.
    let drawn = engine.render(id, 2, 0.5, Rotation::None).unwrap();
    assert!(non_white_pixels(&drawn.rgba) > non_white_pixels(&blank.rgba) + 1000);
}

/// The colour of the pixel at a fraction of a rendered page.
fn pixel_at(page: &rivet_core::RenderedPage, x: f32, y: f32) -> [u8; 3] {
    let px = (x * page.width as f32) as usize;
    let py = (y * page.height as f32) as usize;
    let i = (py * page.width as usize + px) * 4;
    [page.rgba[i], page.rgba[i + 1], page.rgba[i + 2]]
}

#[test]
fn places_signature_pictures_with_transparency() {
    let _serial = serial();
    let dir = temp_dir("signature");
    let path = dir.join("signed.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    // 20 × 10 pixels: the left half dark blue and opaque, the right half transparent.
    let mut rgba = Vec::new();
    for _ in 0..10 {
        for x in 0..20 {
            rgba.extend_from_slice(if x < 10 {
                &[20, 30, 120, 255]
            } else {
                &[255, 0, 0, 0]
            });
        }
    }
    let image = StampImage {
        width: 20,
        height: 10,
        rgba,
    };
    let mut stamp = annotation(AnnotationKind::Stamp, rect(0.2, 0.6, 0.6, 0.7));
    stamp.id = doc.add_image_stamp(2, &stamp, &image).unwrap();

    let page = doc.render_page(2, 1.0, Rotation::None).unwrap();
    let dark = pixel_at(&page, 0.3, 0.65);
    assert!(
        dark[2] > 80 && dark[0] < 80,
        "opaque half is drawn: {dark:?}"
    );
    assert_eq!(
        pixel_at(&page, 0.5, 0.65),
        [255, 255, 255],
        "transparent half shows the page"
    );

    let read = &doc.annotations(2).unwrap()[0];
    assert_eq!(read.kind, AnnotationKind::Stamp);
    assert!(read.editable, "our signatures can be moved");
    assert!((read.rect.left - 0.2).abs() < 0.002 && (read.rect.bottom - 0.7).abs() < 0.002);

    // Move it down.
    stamp.rect = rect(0.2, 0.8, 0.6, 0.9);
    doc.update_annotation(2, &stamp).unwrap();
    let page = doc.render_page(2, 1.0, Rotation::None).unwrap();
    assert_eq!(
        pixel_at(&page, 0.3, 0.65),
        [255, 255, 255],
        "gone from the old place"
    );
    assert!(pixel_at(&page, 0.3, 0.85)[2] > 80, "drawn at the new place");

    doc.save(&path).unwrap();
    let reopened = pdf().open(&path, None).unwrap();
    let page = reopened.render_page(2, 1.0, Rotation::None).unwrap();
    assert!(
        pixel_at(&page, 0.35, 0.85)[2] > 80,
        "saved with its picture"
    );
    assert_eq!(pixel_at(&page, 0.55, 0.85), [255, 255, 255]);
}

/// All stream contents of a PDF file, decompressed, as one string.
fn all_stream_text(path: &std::path::Path) -> String {
    let doc = lopdf::Document::load(path).unwrap();
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .map(|s| {
            String::from_utf8_lossy(&s.decompressed_content().unwrap_or(s.content.clone()))
                .into_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// How PDFium writes a Latin string in a content stream (hex, upper case).
fn hex(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02X}")).collect()
}

#[test]
fn redaction_removes_text_from_the_file() {
    let _serial = serial();
    let dir = temp_dir("redact");
    let source = dir.join("source.pdf");
    let saved = dir.join("redacted.pdf");
    // Helvetica text is stored as plain characters, so the file can be searched.
    pdf().write_test_document(&source, 2).unwrap();
    assert!(all_stream_text(&source).contains(&hex("Rivet test page 1 of 2")));

    let mut doc = pdf().open(&source, None).unwrap();
    let text = doc.page_text(0).unwrap();
    let line = text
        .chars
        .iter()
        .find(|c| c.flags & CHAR_NO_BOX == 0)
        .unwrap();
    let area = rect(0.0, line.top - 0.01, 1.0, line.bottom + 0.01);
    doc.redact(0, &[area]).unwrap();

    // Gone from the page's text, and drawn black.
    assert!(page_string(&doc.page_text(0).unwrap()).trim().is_empty());
    let page = doc.render_page(0, 1.0, Rotation::None).unwrap();
    assert_eq!(
        pixel_at(&page, 0.3, (area.top + area.bottom) / 2.0),
        [0, 0, 0]
    );

    doc.save(&saved).unwrap();
    let streams = all_stream_text(&saved);
    assert!(
        !streams.contains(&hex("test page 1 of 2")),
        "redacted text is not in the file"
    );
    assert!(
        streams.contains(&hex("Rivet test page 2 of 2")),
        "other pages keep their text"
    );
    let reopened = pdf().open(&saved, None).unwrap();
    assert!(
        reopened
            .search(&search_query("page 1 of"), 0)
            .hits
            .is_empty()
    );
    assert_eq!(reopened.search(&search_query("page 2 of"), 0).hits.len(), 1);
}

#[test]
fn redaction_keeps_what_is_outside_the_area() {
    let _serial = serial();
    let dir = temp_dir("redact-keep");
    let saved = dir.join("redacted.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let before = doc.render_page(1, 1.0, Rotation::None).unwrap();
    let text = doc.page_text(1).unwrap();
    let all = page_string(&text);
    let start = all.find("rivet-needle").unwrap();
    let needle = &text.chars[start..start + 12];
    let area = rect(
        needle[0].left,
        needle[0].top,
        needle[11].right,
        needle[0].bottom,
    );
    doc.redact(1, &[area]).unwrap();

    // The page looks the same outside the area (it's now a picture, so letter
    // edges are anti-aliased a little differently).
    let after = doc.render_page(1, 1.0, Rotation::None).unwrap();
    let heading = text
        .chars
        .iter()
        .find(|c| c.flags & CHAR_NO_BOX == 0)
        .unwrap();
    let y = (heading.top + heading.bottom) / 2.0;
    let differing = (0..100)
        .map(|i| heading.left + i as f32 * 0.003)
        .filter(|&x| {
            let (a, b) = (pixel_at(&before, x, y), pixel_at(&after, x, y));
            a.iter().zip(b).any(|(p, q)| p.abs_diff(q) > 100)
        })
        .count();
    assert!(differing < 10, "heading changed in {differing} places");
    // Text clear of the area can still be found; the redacted text can't.
    let page = page_string(&doc.page_text(1).unwrap());
    assert!(page.contains("Page two"), "{page}");
    assert!(!page.contains("needle"), "{page}");

    doc.save(&saved).unwrap();
    let reopened = pdf().open(&saved, None).unwrap();
    assert_eq!(reopened.search(&search_query("Page two"), 0).hits.len(), 1);
    assert!(reopened.search(&search_query("needle"), 0).hits.is_empty());
    // Only the new picture of the page is in the file as an image.
    let file = lopdf::Document::load(&saved).unwrap();
    let images = file
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| {
            s.dict
                .get(b"Subtype")
                .and_then(|t| t.as_name())
                .is_ok_and(|n| n == b"Image")
        })
        .count();
    assert_eq!(images, 1);
}

#[test]
fn redaction_refuses_password_protected_files() {
    let _serial = serial();
    let doc = pdf().open(&fixture("password.pdf"), Some("rivet")).unwrap();
    let err = doc.redact(0, &[rect(0.1, 0.1, 0.2, 0.2)]).unwrap_err();
    assert_eq!(err.code, ErrorCode::RedactProtected);
}

/// Titles and pages of an outline, nested, for comparing.
fn outline_summary(items: &[OutlineItem]) -> Vec<(String, Option<u32>, usize)> {
    items
        .iter()
        .map(|i| (i.title.clone(), i.page, i.children.len()))
        .collect()
}

#[test]
fn edits_bookmarks_and_saves_them_into_the_file() {
    let _serial = serial();
    let dir = temp_dir("bookmarks");
    let path = dir.join("out.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    assert!(doc.info().unwrap().can_edit_outline);
    let mut items = doc.outline();
    // Rename the first, put a new Arabic bookmark under the second, drop the third.
    items[0].title = "Start here".into();
    items[1].children.push(OutlineItem {
        title: "الصفحة الثالثة".into(),
        page: Some(2),
        children: Vec::new(),
        origin: None,
    });
    items.truncate(2);
    doc.set_outline(items.clone()).unwrap();
    // Shown as edited before saving.
    assert_eq!(doc.outline(), items);
    doc.save(&path).unwrap();

    let saved = pdf().open(&path, None).unwrap();
    let outline = saved.outline();
    assert_eq!(
        outline_summary(&outline),
        vec![
            ("Start here".to_owned(), Some(0), 0),
            ("Page two (US Letter)".to_owned(), Some(1), 1),
        ]
    );
    assert_eq!(outline[1].children[0].title, "الصفحة الثالثة");
    assert_eq!(outline[1].children[0].page, Some(2));

    // The removed entry is gone from the file, not just unlinked.
    let file = lopdf::Document::load(&path).unwrap();
    let has_title = |dict: &lopdf::Dictionary| {
        dict.get(b"Title")
            .and_then(lopdf::Object::as_str)
            .is_ok_and(|t| t.starts_with(b"Page three"))
    };
    assert!(
        !file
            .objects
            .values()
            .any(|o| o.as_dict().is_ok_and(has_title))
    );

    // Saving again (the outline is still the edited one) gives the same bookmarks.
    let mut saved = saved;
    let again = dir.join("again.pdf");
    saved.set_outline(saved.outline()).unwrap();
    saved.save(&again).unwrap();
    assert_eq!(pdf().open(&again, None).unwrap().outline(), outline);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bookmarks_from_the_file_keep_their_actions() {
    let _serial = serial();
    let dir = temp_dir("bookmark-actions");
    // A copy of basic.pdf whose first bookmark opens a web page.
    let source = dir.join("web.pdf");
    let mut file = lopdf::Document::load(fixture("basic.pdf")).unwrap();
    let outlines = file
        .catalog()
        .unwrap()
        .get(b"Outlines")
        .unwrap()
        .as_reference()
        .unwrap();
    let first = file
        .get_dictionary(outlines)
        .unwrap()
        .get(b"First")
        .unwrap()
        .as_reference()
        .unwrap();
    let entry = file.get_dictionary_mut(first).unwrap();
    entry.remove(b"Dest");
    let mut action = lopdf::Dictionary::new();
    action.set("S", lopdf::Object::Name(b"URI".to_vec()));
    action.set("URI", lopdf::Object::string_literal("https://example.com/"));
    entry.set("A", action);
    file.save(&source).unwrap();

    // Move it to the end and save.
    let mut doc = pdf().open(&source, None).unwrap();
    let mut items = doc.outline();
    let web = items.remove(0);
    assert_eq!(web.page, None);
    items.push(web);
    doc.set_outline(items).unwrap();
    let out = dir.join("out.pdf");
    doc.save(&out).unwrap();

    let saved = lopdf::Document::load(&out).unwrap();
    let root = saved
        .get_dictionary(
            saved
                .catalog()
                .unwrap()
                .get(b"Outlines")
                .unwrap()
                .as_reference()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(root.get(b"Count").unwrap().as_i64().unwrap(), 3);
    let last = saved
        .get_dictionary(root.get(b"Last").unwrap().as_reference().unwrap())
        .unwrap();
    let action = last.get(b"A").unwrap().as_dict().unwrap();
    assert_eq!(
        action.get(b"URI").unwrap().as_str().unwrap(),
        b"https://example.com/"
    );
    // The others still go to their pages.
    let reopened = pdf().open(&out, None).unwrap().outline();
    assert_eq!(reopened[0].page, Some(1));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn removing_every_bookmark_removes_the_outline() {
    let _serial = serial();
    let dir = temp_dir("no-bookmarks");
    let path = dir.join("out.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    doc.set_outline(Vec::new()).unwrap();
    doc.save(&path).unwrap();
    assert!(pdf().open(&path, None).unwrap().outline().is_empty());
    let file = lopdf::Document::load(&path).unwrap();
    assert!(file.catalog().unwrap().get(b"Outlines").is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn password_protected_files_cant_edit_bookmarks_yet() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("password.pdf"), Some("rivet")).unwrap();
    assert!(!doc.info().unwrap().can_edit_outline);
    assert!(doc.set_outline(Vec::new()).is_err());
}

#[test]
fn replies_are_linked_to_their_comment_when_saved() {
    let _serial = serial();
    let dir = temp_dir("replies");
    let path = dir.join("r.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    assert!(doc.info().unwrap().can_reply);
    let mut square = annotation(
        AnnotationKind::Square { fill: None },
        rect(0.3, 0.3, 0.5, 0.5),
    );
    square.contents = "Is this right?".into();
    square.id = doc.add_annotation(0, &square).unwrap();
    let before = doc.render_page(0, 0.5, Rotation::None).unwrap();

    let mut reply = annotation(AnnotationKind::Note, square.rect);
    reply.contents = "نعم، صحيح".into();
    reply.reply_to = Some(square.id.clone());
    let reply_id = doc.add_annotation(0, &reply).unwrap();
    // A reply to something that isn't there is refused.
    let mut stray = reply.clone();
    stray.reply_to = Some("nothing".into());
    assert!(doc.add_annotation(0, &stray).is_err());

    // Listed, pointing to the square, and not drawn.
    let listed = doc.annotations(0).unwrap();
    let found = listed.iter().find(|a| a.id == reply_id).unwrap();
    assert_eq!(found.reply_to.as_deref(), Some(square.id.as_str()));
    let after = doc.render_page(0, 0.5, Rotation::None).unwrap();
    assert_eq!(before.rgba, after.rgba, "replies draw nothing");

    doc.save(&path).unwrap();
    // A standard reply in the file: /IRT to the square, no private key.
    let file = lopdf::Document::load(&path).unwrap();
    let dicts: Vec<&lopdf::Dictionary> = file
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .collect();
    let square_ref = file
        .objects
        .iter()
        .find(|(_, o)| {
            o.as_dict().is_ok_and(|d| {
                d.get(b"Subtype")
                    .and_then(|s| s.as_name())
                    .is_ok_and(|n| n == b"Square")
            })
        })
        .map(|(id, _)| *id)
        .unwrap();
    let saved_reply = dicts
        .iter()
        .find(|d| d.has(b"IRT"))
        .expect("a reply with /IRT");
    assert_eq!(
        saved_reply.get(b"IRT").unwrap().as_reference().unwrap(),
        square_ref
    );
    assert!(!dicts.iter().any(|d| d.has(b"PDFRivetReplyTo")));

    // Read back from the saved file as a reply.
    let reopened = pdf().open(&path, None).unwrap();
    let back = reopened.annotations(0).unwrap();
    let square_back = back.iter().find(|a| a.reply_to.is_none()).unwrap();
    let reply_back = back.iter().find(|a| a.reply_to.is_some()).unwrap();
    assert_eq!(reply_back.reply_to.as_ref(), Some(&square_back.id));
    assert_eq!(reply_back.contents, "نعم، صحيح");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn replies_to_a_deleted_comment_are_left_out() {
    let _serial = serial();
    let dir = temp_dir("orphan-replies");
    let path = dir.join("o.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let mut note = annotation(AnnotationKind::Note, rect(0.1, 0.1, 0.13, 0.13));
    note.id = doc.add_annotation(0, &note).unwrap();
    let mut reply = annotation(AnnotationKind::Note, note.rect);
    reply.reply_to = Some(note.id.clone());
    doc.add_annotation(0, &reply).unwrap();
    doc.delete_annotation(0, &note.id).unwrap();
    doc.save(&path).unwrap();
    let left = pdf().open(&path, None).unwrap().annotations(0).unwrap();
    assert!(left.is_empty(), "{left:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reads_every_pages_annotations_in_batches() {
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    doc.add_annotation(
        2,
        &annotation(AnnotationKind::Note, rect(0.1, 0.1, 0.13, 0.13)),
    )
    .unwrap();
    let batch = doc.annotations_from(0);
    assert_eq!(batch.next_page, None);
    assert_eq!(batch.pages.len(), 1);
    assert_eq!(batch.pages[0].page, 2);
    assert!(doc.annotations_from(3).pages.is_empty());
}

fn text_style(family: &str, size: f32) -> TextStyle {
    TextStyle {
        font: TextFont {
            family: family.into(),
            bundled: true,
        },
        size,
        bold: false,
        align: TextAlign::Auto,
        valign: VerticalAlign::Top,
        direction: TextDirection::Auto,
        width: None,
        height: None,
    }
}

/// Non-white pixels inside a page area (fractions) of a render.
fn ink_in(page: &rivet_core::RenderedPage, r: PageRect) -> usize {
    let (w, h) = (page.width as f32, page.height as f32);
    let mut count = 0;
    for y in (r.top * h) as u32..(r.bottom * h).min(h) as u32 {
        for x in (r.left * w) as u32..(r.right * w).min(w) as u32 {
            let i = ((y * page.width + x) * 4) as usize;
            if page.rgba[i..i + 3] != [255, 255, 255] {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn text_boxes_are_drawn_saved_and_read_back() {
    let _serial = serial();
    let dir = temp_dir("text-box");
    let path = dir.join("t.pdf");
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let style = text_style("Amiri", 18.0);
    let mut text_box = annotation(
        AnnotationKind::FreeText {
            text: "مرحبا بالعالم\nPDFRivet 2026".into(),
            style: style.clone(),
        },
        rect(0.5, 0.5, 0.9, 0.6),
    );
    text_box.color = BLUE;
    let area = rect(0.2, 0.45, 0.95, 0.75);
    let before = doc.render_page(0, 1.0, Rotation::None).unwrap();
    let id = doc.add_annotation(0, &text_box).unwrap();
    let after = doc.render_page(0, 1.0, Rotation::None).unwrap();
    assert!(
        ink_in(&after, area) > ink_in(&before, area) + 200,
        "the text is drawn"
    );

    let read = doc.annotations(0).unwrap();
    let back = read.iter().find(|a| a.id == id).unwrap();
    match &back.kind {
        AnnotationKind::FreeText { text, style: s } => {
            assert_eq!(text, "مرحبا بالعالم\nPDFRivet 2026");
            assert_eq!(s, &style);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(back.color, BLUE);
    // It grew with its text from its top-right corner (the first line is Arabic).
    assert!((back.rect.right - 0.9).abs() < 1e-3 && back.rect.left > 0.5);
    assert!((back.rect.top - 0.5).abs() < 1e-3);

    doc.save(&path).unwrap();
    let file = lopdf::Document::load(&path).unwrap();
    let saved = file
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| {
            d.get(b"Subtype")
                .and_then(|s| s.as_name())
                .is_ok_and(|n| n == b"FreeText")
        })
        .unwrap();
    assert_eq!(
        saved.get(b"IT").unwrap().as_name().unwrap(),
        b"FreeTextTypeWriter"
    );
    assert!(!saved.has(b"PDFRivetTypewriter"));
    let utf16 = |o: &lopdf::Object| {
        let b = o.as_str().unwrap();
        let units: Vec<u16> = b[2..]
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    };
    let rc = utf16(saved.get(b"RC").unwrap());
    assert!(
        rc.contains(r#"<p dir="rtl" style="text-align:right">مرحبا بالعالم</p>"#),
        "{rc}"
    );
    assert!(
        rc.contains(r#"<p dir="ltr" style="text-align:left">PDFRivet 2026</p>"#),
        "{rc}"
    );
    let ds = saved.get(b"DS").unwrap().as_str().unwrap();
    assert!(String::from_utf8_lossy(ds).contains("18pt 'Amiri'"));

    let reopened = pdf().open(&path, None).unwrap();
    let again = reopened.annotations(0).unwrap();
    assert!(matches!(&again[0].kind, AnnotationKind::FreeText { style: s, .. } if s == &style));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn text_boxes_move_and_change() {
    let _serial = serial();
    let doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let mut style = text_style("Rubik", 14.0);
    let mut a = annotation(
        AnnotationKind::FreeText {
            text: "Hello".into(),
            style: style.clone(),
        },
        rect(0.1, 0.5, 0.2, 0.55),
    );
    a.id = doc.add_annotation(0, &a).unwrap();
    let placed = doc.annotations(0).unwrap().remove(0);
    // Longer text: the box grows to the right.
    let mut longer = placed.clone();
    longer.kind = AnnotationKind::FreeText {
        text: "Hello there, world".into(),
        style: style.clone(),
    };
    doc.update_annotation(0, &longer).unwrap();
    let grown = doc.annotations(0).unwrap().remove(0);
    assert!((grown.rect.left - placed.rect.left).abs() < 1e-3);
    assert!(grown.rect.right > placed.rect.right + 0.05);
    // A set width: lines wrap and the box gets taller.
    style.width = Some(60.0);
    let mut wrapped = grown.clone();
    wrapped.kind = AnnotationKind::FreeText {
        text: "Hello there, world".into(),
        style,
    };
    doc.update_annotation(0, &wrapped).unwrap();
    let tall = doc.annotations(0).unwrap().remove(0);
    assert!(tall.rect.bottom - tall.rect.top > (grown.rect.bottom - grown.rect.top) * 2.0);
    // Moving keeps its look.
    let mut moved = tall.clone();
    let dy = 0.2;
    moved.rect = rect(
        tall.rect.left,
        tall.rect.top + dy,
        tall.rect.right,
        tall.rect.bottom + dy,
    );
    doc.update_annotation(0, &moved).unwrap();
    let page = doc.render_page(0, 1.0, Rotation::None).unwrap();
    assert!(ink_in(&page, moved.rect) > 50);
    assert!(ink_in(&page, tall.rect) == 0);
}

#[test]
fn reads_text_boxes_from_other_apps_and_redraws_them() {
    let _serial = serial();
    let dir = temp_dir("text-box-other");
    let source = dir.join("other.pdf");
    // A text box like PDFgear writes: typewriter, Arial, red, no PDFRivet keys.
    let mut file = lopdf::Document::load(fixture("basic.pdf")).unwrap();
    let page = *file.get_pages().get(&1).unwrap();
    let utf16 = |s: &str| {
        let mut b = vec![0xFE, 0xFF];
        b.extend(s.encode_utf16().flat_map(u16::to_be_bytes));
        lopdf::Object::String(b, lopdf::StringFormat::Hexadecimal)
    };
    let mut annot = lopdf::Dictionary::new();
    annot.set("Type", lopdf::Object::Name(b"Annot".to_vec()));
    annot.set("Subtype", lopdf::Object::Name(b"FreeText".to_vec()));
    annot.set("Rect", vec![100.into(), 600.into(), 200.into(), 620.into()]);
    annot.set("Contents", utf16("مرحبا بالعالم"));
    annot.set("DA", lopdf::Object::string_literal("/Arial 12 Tf 0 0 0 rg"));
    annot.set(
        "DS",
        lopdf::Object::string_literal("font: 12pt Arial; text-align:left; color:#ff0000"),
    );
    annot.set("IT", lopdf::Object::Name(b"FreeTextTypeWriter".to_vec()));
    let id = file.add_object(annot);
    file.get_dictionary_mut(page)
        .unwrap()
        .set("Annots", vec![lopdf::Object::Reference(id)]);
    file.save(&source).unwrap();

    let doc = pdf().open(&source, None).unwrap();
    let found = doc.annotations(0).unwrap().remove(0);
    let AnnotationKind::FreeText { text, style } = &found.kind else {
        panic!("{:?}", found.kind);
    };
    assert_eq!(text, "مرحبا بالعالم");
    assert_eq!(style.size, 12.0);
    assert_eq!(style.font.family, "Arial");
    assert!(!style.font.bundled);
    assert_eq!(style.align, TextAlign::Left);
    assert_eq!(style.width, None);
    assert_eq!(found.color, Color { r: 255, g: 0, b: 0 });
    assert!(found.editable);
    // Editing it draws it again, with joined Arabic (Arial may be missing: a bundled font stands in).
    let before = doc.render_page(0, 1.0, Rotation::None).unwrap();
    let mut edited = found.clone();
    edited.kind = AnnotationKind::FreeText {
        text: "مرحبا بالعالم كله".into(),
        style: style.clone(),
    };
    doc.update_annotation(0, &edited).unwrap();
    let after = doc.render_page(0, 1.0, Rotation::None).unwrap();
    let r = doc.annotations(0).unwrap().remove(0).rect;
    assert!(ink_in(&after, r) > ink_in(&before, r));
    let _ = std::fs::remove_dir_all(dir);
}
