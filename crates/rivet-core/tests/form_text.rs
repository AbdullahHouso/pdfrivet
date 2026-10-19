//! Right-to-left values in form fields, drawn as joined outlines (`formtext.rs`).

mod common;

use std::path::{Path, PathBuf};

use common::{fixture, page_text, pdf, serial, temp_dir};
use lopdf::{Object, ObjectId, dictionary};
use rivet_core::{Document, FieldChange, Rotation};

const ARABIC: &str = "تجربة الكتابة بالعربي";

/// The object of the "name" field in `forms.pdf`.
fn name_field(doc: &lopdf::Document) -> ObjectId {
    doc.objects
        .iter()
        .find(|(_, o)| {
            o.as_dict()
                .ok()
                .and_then(|d| d.get(b"T").and_then(Object::as_str).ok())
                == Some(b"name".as_slice())
        })
        .map(|(id, _)| *id)
        .unwrap()
}

/// `forms.pdf` with its "name" field (or anything else) changed by `edit`.
fn variant(dir: &Path, file: &str, edit: impl FnOnce(&mut lopdf::Document, ObjectId)) -> PathBuf {
    let mut doc = lopdf::Document::load(fixture("forms.pdf")).unwrap();
    let id = name_field(&doc);
    edit(&mut doc, id);
    let out = dir.join(file);
    doc.save(&out).unwrap();
    out
}

fn set(doc: &mut lopdf::Document, id: ObjectId, key: &str, value: impl Into<Object>) {
    doc.get_object_mut(id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set(key, value);
}

/// Opens `path`, types `value` into "name" and shows the page, as the app does.
fn fill(path: &Path, value: &str) -> Document {
    let doc = pdf().open(path, None).unwrap();
    let fields = doc.form_fields(0).unwrap();
    let index = fields
        .iter()
        .find(|f| f.name.as_deref() == Some("name"))
        .unwrap()
        .index;
    doc.change_field(
        0,
        index,
        &FieldChange::Text {
            value: value.into(),
        },
    )
    .unwrap();
    doc.render_page(0, 1.0, Rotation::None).unwrap();
    doc
}

/// The normal appearance (decompressed) of every widget of the field `name`.
fn appearances(path: &Path, name: &str) -> Vec<String> {
    let doc = lopdf::Document::load(path).unwrap();
    let named =
        |d: &lopdf::Dictionary| d.get(b"T").and_then(Object::as_str).ok() == Some(name.as_bytes());
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.has(b"AP"))
        .filter(|d| {
            named(d)
                || d.get(b"Parent")
                    .and_then(Object::as_reference)
                    .and_then(|p| doc.get_dictionary(p))
                    .is_ok_and(named)
        })
        .map(|d| {
            let normal = d.get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap();
            let stream = doc
                .get_object(normal.as_reference().unwrap())
                .unwrap()
                .as_stream()
                .unwrap();
            let bytes = stream
                .decompressed_content()
                .unwrap_or(stream.content.clone());
            String::from_utf8_lossy(&bytes).into_owned()
        })
        .collect()
}

fn saved(doc: &mut Document, dir: &Path, file: &str) -> PathBuf {
    let out = dir.join(file);
    doc.save(&out).unwrap();
    out
}

/// Ours: outlines, no text left for PDFium's broken joins.
fn is_ours(content: &str) -> bool {
    !content.contains("Tj") && content.contains(" c ")
}

/// Left and right ends of the drawn letters (the path after the text colour).
fn text_extent(content: &str) -> (f32, f32) {
    let path = &content[content.rfind(" rg ").unwrap() + 4..];
    let mut numbers: Vec<f32> = Vec::new();
    let (mut min, mut max) = (f32::MAX, f32::MIN);
    for token in path.split_whitespace() {
        match token {
            "m" | "l" | "c" => {
                for pair in numbers.chunks(2) {
                    min = min.min(pair[0]);
                    max = max.max(pair[0]);
                }
                numbers.clear();
            }
            _ => numbers.extend(token.parse::<f32>().ok()),
        }
    }
    (min, max)
}

#[test]
fn draws_fields_of_forms_that_ask_readers_to_draw_them() {
    let _serial = serial();
    let dir = temp_dir("formtext-needap");
    let source = variant(&dir, "need.pdf", |doc, _| {
        let catalog = doc.catalog().unwrap();
        let form = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        set(doc, form, "NeedAppearances", true);
    });
    // PDFium draws every field again when the page is shown; ours wins.
    let mut doc = fill(&source, ARABIC);
    let first = saved(&mut doc, &dir, "filled.pdf");
    assert!(appearances(&first, "name").iter().all(|a| is_ours(a)));
    drop(doc);
    // And when the saved file is opened again (nothing typed this time).
    let mut again = pdf().open(&first, None).unwrap();
    again.render_page(0, 1.0, Rotation::None).unwrap();
    let second = saved(&mut again, &dir, "again.pdf");
    let shown = appearances(&second, "name");
    assert!(
        !shown.is_empty() && shown.iter().all(|a| is_ours(a)),
        "{shown:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn draws_every_widget_of_a_field() {
    let _serial = serial();
    let dir = temp_dir("formtext-widgets");
    // "name" becomes a field with two widgets: the old one and one lower down.
    let source = variant(&dir, "two.pdf", |doc, widget| {
        let page = doc.get_pages()[&1];
        let parent = doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("name"),
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            "V" => Object::string_literal(""),
        });
        let dict = doc.get_object_mut(widget).unwrap().as_dict_mut().unwrap();
        let mut second = dict.clone();
        for key in [b"T".as_slice(), b"FT", b"V", b"DV", b"MaxLen", b"Ff"] {
            dict.remove(key);
            second.remove(key);
        }
        dict.set("Parent", parent);
        second.set("Parent", parent);
        second.set("Rect", vec![160.into(), 400.into(), 410.into(), 422.into()]);
        second.remove(b"AP");
        let second = doc.add_object(second);
        set(doc, parent, "Kids", vec![widget.into(), second.into()]);
        let annots = doc
            .get_dictionary_mut(page)
            .unwrap()
            .get_mut(b"Annots")
            .unwrap();
        annots.as_array_mut().unwrap().push(second.into());
        let catalog = doc.catalog().unwrap();
        let form = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        let fields = doc
            .get_dictionary_mut(form)
            .unwrap()
            .get_mut(b"Fields")
            .unwrap();
        for f in fields.as_array_mut().unwrap() {
            if f.as_reference().ok() == Some(widget) {
                *f = parent.into();
            }
        }
    });
    let mut doc = fill(&source, ARABIC);
    let out = saved(&mut doc, &dir, "filled.pdf");
    let shown = appearances(&out, "name");
    assert_eq!(shown.len(), 2);
    assert!(shown.iter().all(|a| is_ours(a)), "{shown:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keeps_a_centred_field_centred() {
    let _serial = serial();
    let dir = temp_dir("formtext-centred");
    let source = variant(&dir, "centred.pdf", |doc, id| set(doc, id, "Q", 1));
    let mut doc = fill(&source, "تجربة");
    let content = &appearances(&saved(&mut doc, &dir, "filled.pdf"), "name")[0];
    let (left, right) = text_extent(content);
    // The field is 250 points wide.
    assert!((left - (250.0 - right)).abs() < 3.0, "{left}..{right}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shrinks_long_text_in_an_auto_sized_field() {
    let _serial = serial();
    let dir = temp_dir("formtext-auto");
    let source = variant(&dir, "auto.pdf", |doc, id| {
        set(doc, id, "DA", Object::string_literal("/Helv 0 Tf 0 g"));
    });
    let long = ARABIC.repeat(4);
    let mut doc = fill(&source, &long);
    let content = &appearances(&saved(&mut doc, &dir, "filled.pdf"), "name")[0];
    let (left, right) = text_extent(content);
    assert!(left >= 0.0 && right <= 250.0, "{left}..{right}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn turns_text_with_a_turned_field() {
    let _serial = serial();
    let dir = temp_dir("formtext-turned");
    let source = variant(&dir, "turned.pdf", |doc, id| {
        // A tall field whose text runs upwards.
        set(
            doc,
            id,
            "Rect",
            vec![160.into(), 450.into(), 182.into(), 700.into()],
        );
        let mk = doc
            .get_dictionary_mut(id)
            .unwrap()
            .get_mut(b"MK")
            .unwrap()
            .as_dict_mut()
            .unwrap();
        mk.set("R", 90);
        doc.get_dictionary_mut(id).unwrap().remove(b"AP");
    });
    let mut doc = fill(&source, "تجربة");
    let content = &appearances(&saved(&mut doc, &dir, "filled.pdf"), "name")[0];
    assert!(is_ours(content), "{content}");
    assert!(
        content.contains(" 0 1 -1 0 22.000 0.000 cm"),
        "not turned: {content}"
    );
    // Laid out along the field's 250-point height.
    let (_, right) = text_extent(content);
    assert!(right > 30.0 && right <= 250.0, "{right}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keeps_dashed_borders_and_leaves_comb_fields_to_pdfium() {
    let _serial = serial();
    let dir = temp_dir("formtext-borders");
    let dashed = variant(&dir, "dashed.pdf", |doc, id| {
        set(
            doc,
            id,
            "BS",
            dictionary! { "S" => "D", "W" => 1, "D" => vec![3.into(), 2.into()] },
        );
        doc.get_dictionary_mut(id).unwrap().remove(b"AP");
    });
    let mut doc = fill(&dashed, "تجربة");
    let content = &appearances(&saved(&mut doc, &dir, "dashed-filled.pdf"), "name")[0];
    assert!(
        is_ours(content) && content.contains("] 0.000 d "),
        "{content}"
    );

    // Comb fields put each character in its own box: PDFium does that.
    let comb = variant(&dir, "comb.pdf", |doc, id| {
        set(doc, id, "Ff", 1 << 24);
        set(doc, id, "MaxLen", 10);
    });
    let mut doc = fill(&comb, "تجربة");
    let content = &appearances(&saved(&mut doc, &dir, "comb-filled.pdf"), "name")[0];
    assert!(content.contains("Tj"), "{content}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn flattened_arabic_stays_searchable() {
    let _serial = serial();
    let dir = temp_dir("formtext-flatten");
    let mut doc = fill(&fixture("forms.pdf"), ARABIC);
    // Saved and opened again first: the field is ours already, not just typed.
    let filled = saved(&mut doc, &dir, "filled.pdf");
    drop(doc);
    let mut doc = pdf().open(&filled, None).unwrap();
    doc.render_page(0, 1.0, Rotation::None).unwrap();
    assert!(doc.flatten().unwrap() > 0);
    let flat = saved(&mut doc, &dir, "flat.pdf");
    drop(doc);
    let doc = pdf().open(&flat, None).unwrap();
    assert!(doc.form_fields(0).unwrap().is_empty());
    let text: String = page_text(&doc, 0)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for word in ARABIC.split(' ') {
        assert!(text.contains(word), "{word} not in {text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
