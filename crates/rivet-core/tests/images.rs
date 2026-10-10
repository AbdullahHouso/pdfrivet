//! Pictures in and out.

mod common;

use common::{fixture, pdf, pdfium_dir, serial, temp_dir};
use rivet_core::{Engine, ErrorCode, ImageFormat, ImageLayout, PagePaper};

/// A small test picture in the given format.
fn picture(
    dir: &std::path::Path,
    name: &str,
    width: u32,
    height: u32,
    format: image::ImageFormat,
) -> std::path::PathBuf {
    let img = image::RgbaImage::from_fn(width, height, |x, y| {
        image::Rgba([
            (x * 7) as u8,
            (y * 5) as u8,
            128,
            if x < width / 2 { 255 } else { 128 },
        ])
    });
    let path = dir.join(name);
    match format {
        image::ImageFormat::Jpeg => image::DynamicImage::ImageRgba8(img)
            .to_rgb8()
            .save_with_format(&path, format),
        _ => img.save_with_format(&path, format),
    }
    .unwrap();
    path
}

fn size(doc: &rivet_core::Document, i: u32) -> (i32, i32) {
    let s = doc.page_size(i).unwrap();
    (s.width.round() as i32, s.height.round() as i32)
}

#[test]
fn makes_pages_from_pictures() {
    let _serial = serial();
    let dir = temp_dir("images-in");
    let jpeg = picture(&dir, "photo.jpg", 400, 300, image::ImageFormat::Jpeg);
    let png = picture(&dir, "drawing.png", 100, 200, image::ImageFormat::Png);
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (doc, _) = engine.new_document().unwrap();
    let a4 = ImageLayout {
        paper: PagePaper::A4,
        margin: 18.0,
    };
    engine.add_image_page(doc, jpeg.clone(), a4).unwrap();
    let own = ImageLayout {
        paper: PagePaper::Image,
        margin: 0.0,
    };
    engine.add_image_page(doc, png, own).unwrap();
    let out = dir.join("pictures.pdf");
    engine.save(doc, &out).unwrap();

    let saved = pdf().open(&out, None).unwrap();
    assert_eq!(saved.page_count(), 2);
    assert_eq!(
        size(&saved, 0),
        (842, 595),
        "a wide picture gets a landscape A4 page"
    );
    assert_eq!(
        size(&saved, 1),
        (75, 150),
        "its own size at 96 pixels per inch"
    );
    assert_eq!(saved.properties().creator, rivet_core::metadata::PRODUCER);

    // The JPEG is in the file exactly as it was: no quality lost.
    let original = std::fs::read(&jpeg).unwrap();
    let file = lopdf::Document::load(&out).unwrap();
    let embedded = file
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .any(|s| s.content == original);
    assert!(embedded);
}

#[test]
fn refuses_what_isnt_a_picture() {
    let _serial = serial();
    let dir = temp_dir("images-bad");
    let path = dir.join("notes.png");
    std::fs::write(&path, b"just text").unwrap();
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (doc, _) = engine.new_document().unwrap();
    let layout = ImageLayout {
        paper: PagePaper::A4,
        margin: 0.0,
    };
    let err = engine.add_image_page(doc, path, layout).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedImage);
}

#[test]
fn saves_pages_as_pictures() {
    let _serial = serial();
    let dir = temp_dir("images-out");
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (doc, _) = engine.open(&fixture("basic.pdf"), None).unwrap();
    let png = dir.join("page.png");
    engine
        .export_page_image(doc, 0, 150.0, ImageFormat::Png, 90, &png)
        .unwrap();
    let (w, h) = image::image_dimensions(&png).unwrap();
    assert_eq!((w, h), (1240, 1754), "A4 at 150 dpi");
    let jpg = dir.join("page.jpg");
    engine
        .export_page_image(doc, 2, 72.0, ImageFormat::Jpeg, 80, &jpg)
        .unwrap();
    let (w, h) = image::image_dimensions(&jpg).unwrap();
    assert_eq!((w, h), (842, 595), "the landscape page at 72 dpi");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keeps_the_colours_of_opaque_pictures() {
    let _serial = serial();
    let dir = temp_dir("images-colours");
    // Left half red, right half blue: swapped channels would show at once.
    let picture = image::RgbImage::from_fn(200, 100, |x, _| {
        if x < 100 {
            image::Rgb([220, 20, 20])
        } else {
            image::Rgb([20, 20, 220])
        }
    });
    let path = dir.join("flag.png");
    picture.save(&path).unwrap();
    let engine = Engine::start(&pdfium_dir()).unwrap();
    let (doc, _) = engine.new_document().unwrap();
    let own = ImageLayout {
        paper: PagePaper::Image,
        margin: 0.0,
    };
    engine.add_image_page(doc, path, own).unwrap();
    let out = dir.join("flag.pdf");
    engine.save(doc, &out).unwrap();

    let saved = pdf().open(&out, None).unwrap();
    let page = saved
        .render_page(0, 1.0, rivet_core::Rotation::None)
        .unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * page.width + x) * 4) as usize;
        [page.rgba[i], page.rgba[i + 1], page.rgba[i + 2]]
    };
    let (w, h) = (page.width, page.height);
    let left = at(w / 4, h / 2);
    let right = at(w * 3 / 4, h / 2);
    assert!(left[0] > 180 && left[2] < 60, "left is red: {left:?}");
    assert!(right[2] > 180 && right[0] < 60, "right is blue: {right:?}");
    // And no transparency mask was stored for it.
    let file = lopdf::Document::load(&out).unwrap();
    assert!(
        !file
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .any(|s| s.dict.has(b"SMask"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}
