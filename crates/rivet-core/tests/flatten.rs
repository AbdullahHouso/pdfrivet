//! Flattening annotations and form fields into the pages.

mod common;

use common::{fixture, pdf, pdfium_dir, serial, temp_dir};
use rivet_core::{Annotation, AnnotationKind, Color, Engine, PageRect, Rotation};

/// How different two renders of the same page are (mean difference per channel).
fn difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64)
        .sum::<u64>() as f64
        / a.len() as f64
}

#[test]
fn flattens_fields_and_annotations_without_changing_the_look() {
    let _serial = serial();
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (doc, _) = engine.open(&fixture("forms.pdf"), None).unwrap();
    let square = Annotation {
        id: String::new(),
        kind: AnnotationKind::Square {
            fill: Some(Color {
                r: 30,
                g: 90,
                b: 220,
            }),
        },
        rect: PageRect {
            left: 0.1,
            top: 0.7,
            right: 0.4,
            bottom: 0.9,
        },
        color: Color {
            r: 200,
            g: 30,
            b: 30,
        },
        opacity: 1.0,
        width: 2.0,
        contents: String::new(),
        author: "Tester".into(),
        modified: None,
        editable: true,
        reply_to: None,
    };
    engine.add_annotation(doc, 0, square).unwrap();
    let before = engine.render(doc, 0, 1.0, Rotation::None).unwrap();
    assert!(!engine.form_fields(doc, 0).unwrap().is_empty());

    let (snapshot, _) = engine.flatten(doc).unwrap();
    assert!(
        engine.form_fields(doc, 0).unwrap().is_empty(),
        "no fields left"
    );
    assert!(
        engine.annotations(doc, 0).unwrap().is_empty(),
        "no annotations left"
    );
    let after = engine.render(doc, 0, 1.0, Rotation::None).unwrap();
    assert_eq!((before.width, before.height), (after.width, after.height));
    let diff = difference(&before.rgba, &after.rgba);
    assert!(diff < 1.0, "looks the same (difference {diff})");

    // Saved, the form's field list is gone too.
    let dir = temp_dir("flatten");
    let out = dir.join("flat.pdf");
    engine.save(doc, &out).unwrap();
    let file = lopdf::Document::load(&out).unwrap();
    assert!(file.catalog().unwrap().get(b"AcroForm").is_err());
    assert!(
        pdf()
            .open(&out, None)
            .unwrap()
            .form_fields(0)
            .unwrap()
            .is_empty()
    );

    // Undo brings the fields back.
    engine.swap_snapshot(doc, snapshot).unwrap();
    assert!(!engine.form_fields(doc, 0).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
