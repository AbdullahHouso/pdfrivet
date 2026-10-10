//! Rearranging pages: order, rotation, deleting, inserting, undo.

mod common;

use common::{fixture, page_text, pdf, serial, temp_dir};
use rivet_core::{Document, ErrorCode, PageSlot, PageSource};

fn page(index: u32) -> PageSlot {
    PageSlot {
        source: PageSource::Page { index },
        turns: 0,
    }
}

/// (width, height) rounded to whole points.
fn size(doc: &Document, index: u32) -> (i32, i32) {
    let s = doc.page_size(index).unwrap();
    (s.width.round() as i32, s.height.round() as i32)
}

const A4: (i32, i32) = (595, 842);
const LETTER: (i32, i32) = (612, 792);
const A4_LANDSCAPE: (i32, i32) = (842, 595);

#[test]
fn reorders_rotates_deletes_and_inserts() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    assert_eq!(
        [size(&doc, 0), size(&doc, 1), size(&doc, 2)],
        [A4, LETTER, A4_LANDSCAPE]
    );

    // Landscape page first, turned upright; then page one; a blank page; page one again.
    // Page two (US Letter) is left out, so it's deleted.
    let slots = vec![
        PageSlot {
            source: PageSource::Page { index: 2 },
            turns: 1,
        },
        page(0),
        PageSlot {
            source: PageSource::Blank {
                width: 200.0,
                height: 100.0,
            },
            turns: 0,
        },
        page(0),
    ];
    doc.arrange(&slots, &[]).unwrap();

    assert_eq!(doc.page_count(), 4);
    assert_eq!(size(&doc, 0), A4, "the landscape page turned upright");
    assert!(page_text(&doc, 0).contains("Page three"));
    assert!(page_text(&doc, 1).contains("Page one"));
    assert_eq!(size(&doc, 2), (200, 100));
    assert!(page_text(&doc, 2).trim().is_empty());
    assert!(page_text(&doc, 3).contains("Page one"), "duplicated page");

    // Saved and opened again, it's the same, and the deleted page is gone from the file.
    let dir = temp_dir("arrange");
    let out = dir.join("arranged.pdf");
    doc.save(&out).unwrap();
    let saved = pdf().open(&out, None).unwrap();
    assert_eq!(saved.page_count(), 4);
    assert!(page_text(&saved, 0).contains("Page three"));
    assert!(page_text(&saved, 3).contains("Page one"));
    let file = lopdf::Document::load(&out).unwrap();
    let letter_pages = file
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Type").and_then(|t| t.as_name()).ok() == Some(b"Page"))
        .filter(|d| {
            let media = d.get(b"MediaBox").and_then(|m| m.as_array());
            media.is_ok_and(|m| m.get(2).and_then(|w| w.as_float().ok()) == Some(612.0))
        })
        .count();
    assert_eq!(letter_pages, 0, "the deleted page isn't left in the file");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bookmarks_follow_moved_pages_and_lose_deleted_ones() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let before: Vec<_> = doc
        .outline()
        .iter()
        .map(|i| (i.title.clone(), i.page))
        .collect();
    assert_eq!(
        before.len(),
        3,
        "the fixture has a bookmark per page: {before:?}"
    );

    // Reverse the pages and delete the middle one.
    doc.arrange(&[page(2), page(0)], &[]).unwrap();
    let after: Vec<_> = doc
        .outline()
        .iter()
        .map(|i| (i.title.clone(), i.page))
        .collect();
    assert_eq!(after.len(), 2);
    assert!(
        after[0].0.contains("one") && after[0].1 == Some(1),
        "{after:?}"
    );
    assert!(
        after[1].0.contains("three") && after[1].1 == Some(0),
        "{after:?}"
    );
}

#[test]
fn rotates_pages() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    doc.rotate_pages(&[0, 2], 1).unwrap();
    assert_eq!(size(&doc, 0), (842, 595));
    assert_eq!(size(&doc, 1), LETTER, "untouched");
    assert_eq!(size(&doc, 2), A4);
    doc.rotate_pages(&[0], -1).unwrap();
    assert_eq!(size(&doc, 0), A4, "turned back");
}

#[test]
fn inserts_pages_from_another_document() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let other = pdf().open(&fixture("arabic.pdf"), None).unwrap();
    doc.import_pages(&other, &[0], 1).unwrap();
    assert_eq!(doc.page_count(), 4);
    assert!(page_text(&doc, 0).contains("Page one"));
    assert!(page_text(&doc, 2).contains("Page two"));
    // (Compared by its first line: PDFium may order a mixed Arabic/English line differently.)
    assert!(page_text(&doc, 1).starts_with("مرحبا بكم"));

    // The same through arrange, placed between pages.
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let slots = vec![
        page(0),
        PageSlot {
            source: PageSource::Other { doc: 7, index: 0 },
            turns: 0,
        },
        page(1),
        page(2),
    ];
    doc.arrange(&slots, &[(7, &other)]).unwrap();
    assert_eq!(doc.page_count(), 4);
    assert!(page_text(&doc, 1).starts_with("مرحبا بكم"));
    assert!(page_text(&doc, 3).contains("Page three"));
}

#[test]
fn goes_back_to_a_snapshot() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let before = doc.snapshot().unwrap();
    doc.arrange(&[page(1)], &[]).unwrap();
    assert_eq!(doc.page_count(), 1);
    let after = doc.snapshot().unwrap();

    doc.restore(before).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert!(page_text(&doc, 0).contains("Page one"));
    doc.restore(after).unwrap();
    assert_eq!(doc.page_count(), 1);
    assert!(page_text(&doc, 0).contains("Page two"));
}

#[test]
fn refuses_nonsense() {
    let _serial = serial();
    let mut doc = pdf().open(&fixture("basic.pdf"), None).unwrap();
    let err = doc.arrange(&[], &[]).unwrap_err();
    assert_eq!(err.code, ErrorCode::PageOutOfRange);
    let err = doc.arrange(&[page(9)], &[]).unwrap_err();
    assert_eq!(err.code, ErrorCode::PageOutOfRange);
    let other = PageSlot {
        source: PageSource::Other { doc: 42, index: 0 },
        turns: 0,
    };
    let err = doc.arrange(&[other], &[]).unwrap_err();
    assert_eq!(err.code, ErrorCode::DocumentNotOpen);
    assert_eq!(doc.page_count(), 3, "nothing changed");
}

#[test]
fn extracts_pages_into_a_new_file() {
    let _serial = serial();
    let engine = rivet_core::Engine::start(&common::pdfium_dir()).unwrap();
    let (doc, _) = engine.open(&fixture("basic.pdf"), None).unwrap();
    let dir = temp_dir("extract");
    let out = dir.join("extracted.pdf");
    engine.extract_pages(doc, vec![2, 0], out.clone()).unwrap();
    let extracted = pdf().open(&out, None).unwrap();
    assert_eq!(extracted.page_count(), 2);
    assert!(page_text(&extracted, 0).contains("Page three"));
    assert!(page_text(&extracted, 1).contains("Page one"));
    let _ = std::fs::remove_dir_all(&dir);
}
