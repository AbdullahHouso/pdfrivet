//! Pictures in and out: pages saved as image files, and image files made
//! into pages.

use std::{io::Cursor, path::Path};

use image::{DynamicImage, ImageDecoder, ImageFormat as Format, codecs::jpeg::JpegDecoder};
use pdfium_render::prelude::*;
use serde::Deserialize;
use ts_rs::TS;

use crate::{Document, Error, ErrorCode, RenderedPage, Result, document::write_atomically};

/// Pages of a document made from pictures are this size.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum PagePaper {
    /// The picture's own size (at 96 pixels per inch).
    Image,
    A4,
    Letter,
}

/// How pictures are placed on pages.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ImageLayout {
    pub paper: PagePaper,
    /// Space around the picture, in points.
    pub margin: f32,
}

/// The file type pages are saved as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

/// Bigger pictures are refused (a page that size would also be too big to show).
const MAX_PIXELS: u64 = 200_000_000;
/// How many pixels a saved page may have; bigger requests are rendered smaller.
const MAX_EXPORT_PIXELS: f32 = 150_000_000.0;

fn unsupported(e: impl std::fmt::Display) -> Error {
    Error::new(ErrorCode::UnsupportedImage, e.to_string())
}

/// A picture ready to go on a page: JPEG bytes as they are, or decoded pixels.
enum Picture {
    Jpeg {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    Pixels(DynamicImage),
}

impl Picture {
    fn size(&self) -> (u32, u32) {
        match self {
            Picture::Jpeg { width, height, .. } => (*width, *height),
            Picture::Pixels(image) => (image.width(), image.height()),
        }
    }
}

/// Reads a picture file. JPEGs are kept as they are (no quality lost, no size
/// gained) unless the camera stored them turned (phone photos): those are
/// turned upright, which means encoding them again.
fn read_picture(path: &Path) -> Result<Picture> {
    let bytes = std::fs::read(path).map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
    let format = image::guess_format(&bytes).map_err(unsupported)?;
    if format == Format::Jpeg {
        let mut decoder = JpegDecoder::new(Cursor::new(&bytes)).map_err(unsupported)?;
        let (width, height) = decoder.dimensions();
        check_size(width, height)?;
        let orientation = decoder.orientation().map_err(unsupported)?;
        if orientation == image::metadata::Orientation::NoTransforms {
            return Ok(Picture::Jpeg {
                bytes,
                width,
                height,
            });
        }
        let mut image = DynamicImage::from_decoder(decoder).map_err(unsupported)?;
        image.apply_orientation(orientation);
        let mut upright = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut upright, 92)
            .encode_image(&image.to_rgb8())
            .map_err(unsupported)?;
        let (width, height) = (image.width(), image.height());
        return Ok(Picture::Jpeg {
            bytes: upright,
            width,
            height,
        });
    }
    let image = image::load_from_memory_with_format(&bytes, format).map_err(unsupported)?;
    check_size(image.width(), image.height())?;
    Ok(Picture::Pixels(image))
}

fn check_size(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(unsupported(format!("{width}×{height} pixels")));
    }
    Ok(())
}

impl Document {
    /// Adds a page at the end with the picture at `path` on it.
    pub fn add_image_page(&mut self, path: &Path, layout: &ImageLayout) -> Result<()> {
        let picture = read_picture(path)?;
        let (px_w, px_h) = picture.size();
        // The picture at 96 pixels per inch, in points.
        let (img_w, img_h) = (px_w as f32 * 0.75, px_h as f32 * 0.75);
        let margin = layout.margin.max(0.0);
        let (page_w, page_h) = match layout.paper {
            PagePaper::Image => (img_w + 2.0 * margin, img_h + 2.0 * margin),
            PagePaper::A4 | PagePaper::Letter => {
                let (w, h) = if layout.paper == PagePaper::A4 {
                    (595.28, 841.89)
                } else {
                    (612.0, 792.0)
                };
                // Wide pictures get a landscape page.
                if img_w > img_h { (h, w) } else { (w, h) }
            }
        };
        // PDF pages can't be bigger than 200 inches.
        let shrink = (14_400.0 / page_w.max(page_h)).min(1.0);
        let (page_w, page_h) = (page_w * shrink, page_h * shrink);
        let room_w = (page_w - 2.0 * margin * shrink).max(1.0);
        let room_h = (page_h - 2.0 * margin * shrink).max(1.0);
        let scale = (room_w / img_w).min(room_h / img_h);
        let (w, h) = (img_w * scale, img_h * scale);
        let (x, y) = ((page_w - w) / 2.0, (page_h - h) / 2.0);

        let at = self.inner.pages().len();
        let size = PdfPagePaperSize::Custom(PdfPoints::new(page_w), PdfPoints::new(page_h));
        let mut page = self.inner.pages_mut().create_page_at_index(size, at)?;
        let mut object = match picture {
            Picture::Jpeg { bytes, .. } => {
                PdfPageImageObject::new_from_jpeg_reader(&self.inner, Cursor::new(bytes))?
            }
            Picture::Pixels(image) if image.color().has_alpha() => {
                PdfPageImageObject::new(&self.inner, &image)?
            }
            Picture::Pixels(image) => opaque_image(&self.inner, &image)?,
        };
        object.apply_matrix(PdfMatrix::new(w, 0.0, 0.0, h, x, y))?;
        page.objects_mut().add_image_object(object)?;
        self.unsaved_changes.set(true);
        Ok(())
    }

    /// Renders a page for saving as a picture at `dpi` (upright, as printed).
    pub fn render_for_export(&self, index: u32, dpi: f32) -> Result<RenderedPage> {
        let size = self.page_size(index)?;
        let mut scale = dpi.clamp(18.0, 1200.0) / 72.0;
        let pixels = size.width * size.height * scale * scale;
        if pixels > MAX_EXPORT_PIXELS {
            scale *= (MAX_EXPORT_PIXELS / pixels).sqrt();
        }
        self.render_page(index, scale, crate::Rotation::None)
    }
}

/// An image object for a picture without transparency. pdfium-render always
/// hands pictures over with an alpha channel, and PDFium then stores a
/// transparency mask with them, which only makes the file bigger; given
/// pixels without alpha (BGRx), it stores just the colours.
fn opaque_image<'a>(
    document: &PdfDocument<'a>,
    image: &DynamicImage,
) -> Result<PdfPageImageObject<'a>> {
    let rgb = image.to_rgb8();
    let (width, height) = (rgb.width(), rgb.height());
    let mut bgrx: Vec<u8> = rgb.pixels().flat_map(|p| [p[2], p[1], p[0], 255]).collect();
    let too_big = || Error::new(ErrorCode::UnsupportedImage, "picture too big");
    let bitmap = PdfBitmap::from_bytes(
        width.try_into().map_err(|_| too_big())?,
        height.try_into().map_err(|_| too_big())?,
        PdfBitmapFormat::BGRx,
        &mut bgrx,
    )?;
    // Made from a 1×1 picture, then given the real pixels.
    let mut object = PdfPageImageObject::new(document, &DynamicImage::new_rgb8(1, 1))?;
    object.set_bitmap(&bitmap)?;
    Ok(object)
}

/// Saves rendered pixels as a PNG or JPEG file (`quality` 1–100, for JPEG).
pub fn save_image(
    page: &RenderedPage,
    format: ImageFormat,
    quality: u8,
    path: &Path,
) -> Result<()> {
    let failed = |e: image::ImageError| Error::new(ErrorCode::SaveFailed, e.to_string());
    let rgba = image::RgbaImage::from_raw(page.width, page.height, page.rgba.to_vec())
        .ok_or_else(|| Error::new(ErrorCode::Internal, "pixel buffer size"))?;
    let mut bytes = Vec::new();
    match format {
        ImageFormat::Png => {
            // Pages are opaque: RGB makes smaller files.
            DynamicImage::ImageRgba8(rgba)
                .to_rgb8()
                .write_to(&mut Cursor::new(&mut bytes), Format::Png)
                .map_err(failed)?;
        }
        ImageFormat::Jpeg => {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality.clamp(1, 100))
                .encode_image(&DynamicImage::ImageRgba8(rgba).to_rgb8())
                .map_err(failed)?;
        }
    }
    write_atomically(path, &bytes)
}
