//! Making PDFs smaller.

mod common;

use common::{fixture, page_text, pdf, pdfium_dir, serial, temp_dir};
use rivet_core::{CompressLevel, Engine, ErrorCode, ImageLayout, PagePaper};

/// A PDF with a big photo-like picture (noise, so it doesn't compress by itself)
/// and a drawing with few colours.
fn heavy_pdf(dir: &std::path::Path, engine: &Engine) -> std::path::PathBuf {
    let mut seed = 12345u32;
    let photo = image::RgbImage::from_fn(3000, 2000, |x, y| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let n = (seed >> 24) as u8 / 4;
        image::Rgb([
            (x / 12) as u8 ^ n,
            (y / 8) as u8 ^ n,
            ((x + y) / 20) as u8 ^ n,
        ])
    });
    let photo_path = dir.join("photo.jpg");
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
        .encode_image(&photo)
        .unwrap();
    std::fs::write(&photo_path, jpeg).unwrap();
    let drawing = image::RgbImage::from_fn(1200, 900, |x, _| {
        if x < 600 {
            image::Rgb([200, 30, 30])
        } else {
            image::Rgb([255, 255, 255])
        }
    });
    let drawing_path = dir.join("drawing.png");
    drawing.save(&drawing_path).unwrap();

    let (doc, _) = engine.new_document().unwrap();
    let a4 = ImageLayout {
        paper: PagePaper::A4,
        margin: 0.0,
    };
    engine.add_image_page(doc, photo_path, a4).unwrap();
    engine.add_image_page(doc, drawing_path, a4).unwrap();
    let out = dir.join("heavy.pdf");
    engine.save(doc, &out).unwrap();
    out
}

/// The sizes of the pictures in a PDF.
fn picture_sizes(path: &std::path::Path) -> Vec<(i64, i64, bool)> {
    let doc = lopdf::Document::load(path).unwrap();
    let mut sizes: Vec<_> = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").and_then(|t| t.as_name()).ok() == Some(b"Image"))
        .map(|s| {
            let jpeg = s.dict.get(b"Filter").and_then(|f| f.as_name()).ok() == Some(b"DCTDecode");
            (
                s.dict.get(b"Width").unwrap().as_i64().unwrap(),
                s.dict.get(b"Height").unwrap().as_i64().unwrap(),
                jpeg,
            )
        })
        .collect();
    sizes.sort();
    sizes
}

#[test]
fn shrinks_big_pictures() {
    let _serial = serial();
    let dir = temp_dir("compress");
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let heavy = heavy_pdf(&dir, &engine);
    let before = std::fs::metadata(&heavy).unwrap().len();

    let (doc, _) = engine.open(&heavy, None).unwrap();
    let out = dir.join("smaller.pdf");
    let report = engine.compress(doc, CompressLevel::Balanced, &out).unwrap();
    // Measured on the document as it would be saved (with its changes), so
    // about the file's size.
    assert!(
        report.before.abs_diff(before) < 1024,
        "{report:?} vs {before}"
    );
    assert!(report.after < before / 2, "{report:?}");
    assert_eq!(report.pictures, 1, "only the photo needed it");
    assert_eq!(std::fs::metadata(&out).unwrap().len(), report.after);

    // The photo now fits A4 (landscape, 842 pt wide) at 150 dpi: 1754 pixels;
    // the drawing (few colours, small enough) is untouched and still lossless.
    let sizes = picture_sizes(&out);
    assert_eq!(sizes, [(1200, 900, false), (1754, 1169, true)], "{sizes:?}");
    let opened = pdf().open(&out, None).unwrap();
    assert_eq!(opened.page_count(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lossless_keeps_text_and_never_grows() {
    let _serial = serial();
    let dir = temp_dir("compress-lossless");
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (doc, _) = engine.open(&fixture("basic.pdf"), None).unwrap();
    let out = dir.join("smaller.pdf");
    let report = engine.compress(doc, CompressLevel::Lossless, &out).unwrap();
    assert!(report.after <= report.before);
    let opened = pdf().open(&out, None).unwrap();
    assert!(page_text(&opened, 2).contains("Page three"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_protected_files() {
    let _serial = serial();
    let dir = temp_dir("compress-protected");
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (doc, _) = engine
        .open(&fixture("password.pdf"), Some("rivet".into()))
        .unwrap();
    let err = engine
        .compress(doc, CompressLevel::Balanced, &dir.join("x.pdf"))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::CompressProtected);
    let _ = std::fs::remove_dir_all(&dir);
}
