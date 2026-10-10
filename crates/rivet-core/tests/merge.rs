//! Merging documents.

mod common;

use common::{fixture, page_text, pdf, pdfium_dir, serial, temp_dir};
use rivet_core::{Engine, MergePart};

/// Merges `files` (all their pages, or the ones given) with the engine, the way the app does.
fn merge(
    engine: &Engine,
    files: &[(&str, Option<Vec<u32>>)],
    bookmarks: bool,
    out: &std::path::Path,
) {
    let (merged, _) = engine.new_document().unwrap();
    let mut parts = Vec::new();
    let mut start = 0;
    for (name, pages) in files {
        let (doc, info) = engine.open(&fixture(name), None).unwrap();
        let pages = pages
            .clone()
            .unwrap_or_else(|| (0..info.page_count).collect());
        engine
            .import_pages(merged, doc, pages.clone(), start)
            .unwrap();
        parts.push(MergePart {
            doc,
            pages: pages.clone(),
            start,
            title: name.to_string(),
        });
        start += pages.len() as u32;
    }
    engine
        .finish_merge(merged, parts, bookmarks, out.to_path_buf())
        .unwrap();
}

#[test]
fn merges_pages_and_bookmarks() {
    let _serial = serial();
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let dir = temp_dir("merge");
    let out = dir.join("merged.pdf");
    merge(
        &engine,
        &[
            ("basic.pdf", None),
            ("arabic.pdf", None),
            ("basic.pdf", Some(vec![2, 0])),
        ],
        true,
        &out,
    );
    let doc = pdf().open(&out, None).unwrap();
    assert_eq!(doc.page_count(), 6);
    assert!(page_text(&doc, 0).contains("Page one"));
    assert!(page_text(&doc, 3).starts_with("مرحبا"));
    assert!(page_text(&doc, 4).contains("Page three"));
    assert!(page_text(&doc, 5).contains("Page one"));

    let outline = doc.outline();
    let files: Vec<_> = outline.iter().map(|i| (i.title.as_str(), i.page)).collect();
    assert_eq!(
        files,
        [
            ("basic.pdf", Some(0)),
            ("arabic.pdf", Some(3)),
            ("basic.pdf", Some(4))
        ]
    );
    // The first file's bookmarks are under its own, pointing at its pages.
    let children: Vec<_> = outline[0].children.iter().map(|i| i.page).collect();
    assert_eq!(children, [Some(0), Some(1), Some(2)]);
    // Only some pages of the last one: its bookmarks follow them, the rest go.
    let last: Vec<_> = outline[2].children.iter().map(|i| i.page).collect();
    assert_eq!(last, [Some(5), Some(4)]);

    // Without a bookmark per file, the files' own bookmarks follow each other.
    let flat = dir.join("flat.pdf");
    merge(
        &engine,
        &[("basic.pdf", None), ("basic.pdf", None)],
        false,
        &flat,
    );
    let doc = pdf().open(&flat, None).unwrap();
    let pages: Vec<_> = doc.outline().iter().map(|i| i.page).collect();
    assert_eq!(
        pages,
        [Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keeps_form_fields_working_and_apart() {
    let _serial = serial();
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let dir = temp_dir("merge-forms");
    let out = dir.join("forms.pdf");
    merge(
        &engine,
        &[("forms.pdf", None), ("forms.pdf", None)],
        false,
        &out,
    );

    let doc = pdf().open(&out, None).unwrap();
    let first = doc.form_fields(0).unwrap();
    let second = doc.form_fields(1).unwrap();
    assert!(!first.is_empty(), "the fields are still fields");
    assert_eq!(first.len(), second.len());
    // The same field in the two copies has different names, so they don't share a value.
    let names = |fields: &[rivet_core::FormField]| -> Vec<String> {
        fields.iter().filter_map(|f| f.name.clone()).collect()
    };
    let (a, b) = (names(&first), names(&second));
    assert!(a.iter().all(|n| !b.contains(n)), "{a:?} vs {b:?}");
    assert!(b.iter().any(|n| n.ends_with("_2")), "{b:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
