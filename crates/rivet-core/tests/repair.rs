//! Repairing damaged files.

mod common;

use common::{fixture, page_text, pdf, serial, temp_dir};
use rivet_core::ErrorCode;

fn damaged(name: &str, damage: impl FnOnce(Vec<u8>) -> Vec<u8>) -> std::path::PathBuf {
    let dir = temp_dir(&format!("repair-{name}"));
    let path = dir.join("damaged.pdf");
    std::fs::write(&path, damage(std::fs::read(fixture("basic.pdf")).unwrap())).unwrap();
    path
}

fn find(bytes: &[u8], needle: &[u8]) -> usize {
    bytes
        .windows(needle.len())
        .rposition(|w| w == needle)
        .unwrap()
}

#[test]
fn repairs_a_wrong_cross_reference_offset() {
    let _serial = serial();
    let path = damaged("offset", |mut b| {
        let at = find(&b, b"startxref") + 10;
        let end = at + b[at..].iter().position(|c| !c.is_ascii_digit()).unwrap();
        b.splice(at..end, b"12".iter().copied());
        b
    });
    let out = path.with_file_name("repaired.pdf");
    let report = pdf().repair(&path, &out, None).unwrap();
    assert_eq!(report.pages, 3);
    let doc = pdf().open(&out, None).unwrap();
    assert!(page_text(&doc, 2).contains("Page three"));
}

#[test]
fn repairs_a_file_cut_off_at_the_end() {
    let _serial = serial();
    let path = damaged("cut", |b| {
        let xref = find(&b, b"xref");
        b[..xref].to_vec()
    });
    let out = path.with_file_name("repaired.pdf");
    let report = pdf().repair(&path, &out, None).unwrap();
    assert_eq!(report.pages, 3);
    assert_eq!(pdf().open(&out, None).unwrap().page_count(), 3);
}

#[test]
fn rebuilds_what_pdfium_rejects() {
    let _serial = serial();
    // Junk before the header: PDFium gives up, lopdf finds the file in it.
    let path = damaged("junk", |b| [vec![b'x'; 4096], b].concat());
    assert!(
        pdf().open(&path, None).is_err(),
        "PDFium can't open it as it is"
    );
    let out = path.with_file_name("repaired.pdf");
    let report = pdf().repair(&path, &out, None).unwrap();
    assert!(report.rebuilt);
    assert_eq!(report.pages, 3);
    assert!(page_text(&pdf().open(&out, None).unwrap(), 0).contains("Page one"));
}

#[test]
fn says_when_nothing_can_be_saved() {
    let _serial = serial();
    let dir = temp_dir("repair-hopeless");
    let path = dir.join("noise.pdf");
    std::fs::write(&path, b"%PDF-1.7\nthis is not a pdf at all\n%%EOF").unwrap();
    let err = pdf().repair(&path, &dir.join("out.pdf"), None).unwrap_err();
    assert_eq!(err.code, ErrorCode::NotRepairable);
    assert!(!dir.join("out.pdf").exists());
}
