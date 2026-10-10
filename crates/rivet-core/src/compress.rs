//! Making PDFs smaller.
//!
//! Every level rewrites the file compactly: unused objects go, streams that
//! weren't compressed are, and objects are packed into compressed object
//! streams. The stronger levels also shrink pictures: ones with more pixels
//! than the page can show at the target resolution are scaled down, and
//! photos are stored as JPEG. Pictures this can't handle faithfully (CMYK,
//! 16-bit, masks, JPEG 2000…) are left exactly as they are.

use std::collections::{HashMap, HashSet};

use image::{DynamicImage, ImageBuffer, ImageFormat as Format, imageops::FilterType};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, ErrorCode, Result};

/// How hard to compress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CompressLevel {
    /// Only a more compact file: nothing changes in how it looks.
    Lossless,
    /// Pictures down to 150 dpi, photos as JPEG (quality 75): fine on screen and paper.
    Balanced,
    /// Pictures down to 100 dpi (quality 60): smallest, still readable.
    Strong,
}

impl CompressLevel {
    /// (Resolution, JPEG quality) for pictures, or None to leave them alone.
    fn pictures(self) -> Option<(f32, u8)> {
        match self {
            CompressLevel::Lossless => None,
            CompressLevel::Balanced => Some((150.0, 75)),
            CompressLevel::Strong => Some((100.0, 60)),
        }
    }
}

/// What compressing did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct CompressReport {
    #[ts(type = "number")]
    pub before: u64,
    #[ts(type = "number")]
    pub after: u64,
    /// Pictures made smaller.
    pub pictures: u32,
}

fn failed(e: &dyn std::fmt::Display) -> Error {
    Error::new(ErrorCode::SaveFailed, e.to_string())
}

/// Compresses an unprotected PDF's bytes. Returns the smaller bytes (the
/// original ones if nothing got smaller) and what was done.
pub fn compress(bytes: &[u8], level: CompressLevel) -> Result<(Vec<u8>, CompressReport)> {
    let mut doc = Document::load_mem(bytes).map_err(|e| failed(&e))?;
    if doc.trailer.get(b"Encrypt").is_ok() || doc.was_encrypted() {
        return Err(Error::new(
            ErrorCode::CompressProtected,
            "password-protected",
        ));
    }
    let mut pictures = 0;
    if let Some((dpi, quality)) = level.pictures() {
        let limits = picture_limits(&doc, dpi);
        let mut done = HashSet::new();
        for (id, max_side) in limits {
            if shrink_picture(&mut doc, id, max_side, quality, &mut done) {
                pictures += 1;
            }
        }
    }
    merge_duplicates(&mut doc);
    doc.prune_objects();
    doc.delete_zero_length_streams();
    doc.compress();
    let mut out = Vec::new();
    doc.save_modern(&mut out).map_err(|e| failed(&e))?;
    let before = bytes.len() as u64;
    if out.len() as u64 >= before {
        // Nothing to gain: the file stays as it was.
        return Ok((
            bytes.to_vec(),
            CompressReport {
                before,
                after: before,
                pictures: 0,
            },
        ));
    }
    let after = out.len() as u64;
    Ok((
        out,
        CompressReport {
            before,
            after,
            pictures,
        },
    ))
}

/// Streams stored more than once (the same font or drawing on every page, as
/// some apps write them) are kept once, and everything points at that one.
fn merge_duplicates(doc: &mut Document) {
    use std::hash::{Hash, Hasher};
    let mut first: HashMap<(u64, usize), Vec<ObjectId>> = HashMap::new();
    let mut same: HashMap<ObjectId, ObjectId> = HashMap::new();
    let key_of = |stream: &Stream| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        stream.content.hash(&mut hasher);
        for (k, v) in stream.dict.iter() {
            if k != b"Length" {
                k.hash(&mut hasher);
                format!("{v:?}").hash(&mut hasher);
            }
        }
        (hasher.finish(), stream.content.len())
    };
    for (&id, object) in &doc.objects {
        let Object::Stream(stream) = object else {
            continue;
        };
        let candidates = first.entry(key_of(stream)).or_default();
        // A hash match is checked byte by byte before two streams count as one.
        let original = candidates.iter().find(|&&c| match doc.objects.get(&c) {
            Some(Object::Stream(other)) => {
                other.content == stream.content && same_dict(&other.dict, &stream.dict)
            }
            _ => false,
        });
        match original {
            Some(&o) => {
                same.insert(id, o);
            }
            None => candidates.push(id),
        }
    }
    if same.is_empty() {
        return;
    }
    fn repoint(object: &mut Object, same: &HashMap<ObjectId, ObjectId>) {
        match object {
            Object::Reference(id) => {
                if let Some(o) = same.get(id) {
                    *id = *o;
                }
            }
            Object::Array(items) => items.iter_mut().for_each(|i| repoint(i, same)),
            Object::Dictionary(dict) => dict.iter_mut().for_each(|(_, v)| repoint(v, same)),
            Object::Stream(stream) => stream.dict.iter_mut().for_each(|(_, v)| repoint(v, same)),
            _ => {}
        }
    }
    for object in doc.objects.values_mut() {
        repoint(object, &same);
    }
    for value in doc.trailer.iter_mut() {
        repoint(value.1, &same);
    }
}

/// The same entries (in any order), not counting `/Length`.
fn same_dict(a: &Dictionary, b: &Dictionary) -> bool {
    let entries = |d: &Dictionary| d.iter().filter(|(k, _)| k.as_slice() != b"Length").count();
    entries(a) == entries(b)
        && a.iter()
            .filter(|(k, _)| k.as_slice() != b"Length")
            .all(|(k, v)| b.get(k).is_ok_and(|w| w == v))
}

/// The most pixels (longest side) each picture needs: the longest side of the
/// biggest page it's on, at `dpi`.
fn picture_limits(doc: &Document, dpi: f32) -> HashMap<ObjectId, u32> {
    let mut limits: HashMap<ObjectId, u32> = HashMap::new();
    for page in doc.get_pages().into_values() {
        let side = page_long_side(doc, page);
        let max_side = (side / 72.0 * dpi).ceil().max(64.0) as u32;
        let mut seen = HashSet::new();
        if let Ok((own, inherited)) = doc.get_page_resources(page) {
            let mut all: Vec<&Dictionary> = own.into_iter().collect();
            all.extend(
                inherited
                    .iter()
                    .filter_map(|id| doc.get_dictionary(*id).ok()),
            );
            for resources in all {
                collect_images(doc, resources, max_side, 0, &mut seen, &mut limits);
            }
        }
    }
    limits
}

/// Pictures in a resource dictionary, and in the forms (reusable drawings) it uses.
fn collect_images(
    doc: &Document,
    resources: &Dictionary,
    max_side: u32,
    depth: u32,
    seen: &mut HashSet<ObjectId>,
    limits: &mut HashMap<ObjectId, u32>,
) {
    if depth > 4 {
        return;
    }
    let Ok(xobjects) = resources.get(b"XObject").and_then(|x| resolve_dict(doc, x)) else {
        return;
    };
    for (_, value) in xobjects.iter() {
        let Ok(id) = value.as_reference() else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let Ok(Object::Stream(stream)) = doc.get_object(id) else {
            continue;
        };
        match stream.dict.get(b"Subtype").and_then(Object::as_name) {
            Ok(b"Image") => {
                let limit = limits.entry(id).or_insert(0);
                *limit = (*limit).max(max_side);
            }
            Ok(b"Form") => {
                if let Ok(inner) = stream
                    .dict
                    .get(b"Resources")
                    .and_then(|r| resolve_dict(doc, r))
                {
                    collect_images(doc, inner, max_side, depth + 1, seen, limits);
                }
            }
            _ => {}
        }
    }
}

fn resolve_dict<'a>(doc: &'a Document, object: &'a Object) -> lopdf::Result<&'a Dictionary> {
    match object {
        Object::Reference(id) => doc.get_dictionary(*id),
        other => other.as_dict(),
    }
}

/// A page's longest side in points (from its own or an inherited MediaBox).
fn page_long_side(doc: &Document, page: ObjectId) -> f32 {
    let mut current = Some(page);
    for _ in 0..32 {
        let Some(id) = current else { break };
        let Ok(dict) = doc.get_dictionary(id) else {
            break;
        };
        if let Ok(Object::Array(b)) = dict.get(b"MediaBox").and_then(|m| match m {
            Object::Reference(r) => doc.get_object(*r),
            other => Ok(other),
        }) {
            let v: Vec<f32> = b.iter().filter_map(|n| n.as_float().ok()).collect();
            if v.len() == 4 {
                return (v[2] - v[0]).abs().max((v[3] - v[1]).abs());
            }
        }
        current = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    842.0
}

/// The picture's pixels, if it's a kind that can be redone faithfully:
/// 8-bit gray or RGB, stored raw, Flate-compressed or as JPEG, without masks
/// of its own or remapped colours.
fn read_pixels(doc: &Document, stream: &Stream) -> Option<(DynamicImage, bool)> {
    let dict = &stream.dict;
    if dict
        .get(b"ImageMask")
        .and_then(Object::as_bool)
        .unwrap_or(false)
        || dict.has(b"Decode")
        || dict
            .get(b"BitsPerComponent")
            .and_then(Object::as_i64)
            .ok()?
            != 8
    {
        return None;
    }
    let width = u32::try_from(dict.get(b"Width").and_then(Object::as_i64).ok()?).ok()?;
    let height = u32::try_from(dict.get(b"Height").and_then(Object::as_i64).ok()?).ok()?;
    let colour_space = match dict.get(b"ColorSpace").ok()? {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    let components = match colour_space {
        Object::Name(n) if n == b"DeviceRGB" => 3,
        Object::Name(n) if n == b"DeviceGray" => 1,
        Object::Array(a) if a.first().and_then(|n| n.as_name().ok()) == Some(b"ICCBased") => {
            let profile = a.get(1)?.as_reference().ok()?;
            match doc
                .get_object(profile)
                .ok()?
                .as_stream()
                .ok()?
                .dict
                .get(b"N")
                .and_then(Object::as_i64)
            {
                Ok(1) => 1,
                Ok(3) => 3,
                _ => return None,
            }
        }
        _ => return None,
    };
    let filters: Vec<Vec<u8>> = match dict.get(b"Filter") {
        Err(_) => Vec::new(),
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a
            .iter()
            .filter_map(|f| f.as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        Ok(_) => return None,
    };
    match filters
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [b"DCTDecode"] => {
            let image = image::load_from_memory_with_format(&stream.content, Format::Jpeg).ok()?;
            let expected = if components == 3 {
                image.color().channel_count() == 3
            } else {
                image.color().channel_count() == 1
            };
            (expected && image.width() == width && image.height() == height)
                .then_some((image, true))
        }
        [] | [b"FlateDecode"] => {
            let raw = if filters.is_empty() {
                stream.content.clone()
            } else {
                stream.decompressed_content().ok()?
            };
            if raw.len() != (width * height * components) as usize {
                return None;
            }
            let image = if components == 3 {
                DynamicImage::ImageRgb8(ImageBuffer::from_raw(width, height, raw)?)
            } else {
                DynamicImage::ImageLuma8(ImageBuffer::from_raw(width, height, raw)?)
            };
            Some((image, false))
        }
        _ => None,
    }
}

/// A drawing or screenshot (few colours) rather than a photo: JPEG would blur it.
fn looks_like_a_drawing(image: &DynamicImage) -> bool {
    let rgb = image.to_rgb8();
    let step = ((rgb.width() as usize * rgb.height() as usize) / 20_000).max(1);
    let colours: HashSet<[u8; 3]> = rgb
        .pixels()
        .step_by(step)
        .map(|p| p.0)
        .take(20_000)
        .collect();
    colours.len() < 256
}

/// Makes one picture smaller if that's worth it; true if it changed.
fn shrink_picture(
    doc: &mut Document,
    id: ObjectId,
    max_side: u32,
    quality: u8,
    done: &mut HashSet<ObjectId>,
) -> bool {
    if !done.insert(id) {
        return false;
    }
    let Ok(Object::Stream(stream)) = doc.get_object(id) else {
        return false;
    };
    let stream = stream.clone();
    let Some((image, was_jpeg)) = read_pixels(doc, &stream) else {
        return false;
    };
    let longest = image.width().max(image.height());
    let resize = longest > max_side + max_side / 10;
    let drawing = !was_jpeg && looks_like_a_drawing(&image);
    if !resize && (drawing || was_jpeg && stream.content.len() < 200_000) {
        return false;
    }
    let image = if resize {
        let scale = max_side as f32 / longest as f32;
        let w = ((image.width() as f32 * scale).round() as u32).max(1);
        let h = ((image.height() as f32 * scale).round() as u32).max(1);
        image.resize_exact(w, h, FilterType::Triangle)
    } else {
        image
    };
    // A picture's soft mask (transparency) must keep its size.
    let smask = stream
        .dict
        .get(b"SMask")
        .and_then(Object::as_reference)
        .ok();
    let new_mask = match smask {
        Some(mask) if resize => match resized_mask(doc, mask, image.width(), image.height()) {
            Some(m) => Some((mask, m)),
            None => return false,
        },
        _ => None,
    };
    let (content, filter) = if drawing {
        let raw = image.as_bytes().to_vec();
        (raw, None)
    } else {
        let mut jpeg = Vec::new();
        let encoded = match &image {
            DynamicImage::ImageLuma8(gray) => {
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
                    .encode_image(gray)
            }
            other => image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
                .encode_image(&other.to_rgb8()),
        };
        if encoded.is_err() {
            return false;
        }
        (jpeg, Some(b"DCTDecode".to_vec()))
    };
    // Only worth it if it's clearly smaller.
    if !resize && content.len() as f32 > stream.content.len() as f32 * 0.9 {
        return false;
    }
    let mut dict = stream.dict.clone();
    dict.set("Width", i64::from(image.width()));
    dict.set("Height", i64::from(image.height()));
    dict.remove(b"DecodeParms");
    dict.remove(b"Filter");
    let mut new = Stream::new(dict, content);
    match filter {
        Some(f) => {
            new.dict.set("Filter", Object::Name(f));
            new.allows_compression = false;
        }
        None => {
            let _ = new.compress();
        }
    }
    doc.objects.insert(id, Object::Stream(new));
    if let Some((mask_id, mask)) = new_mask {
        doc.objects.insert(mask_id, Object::Stream(mask));
    }
    true
}

/// A picture's soft mask scaled to a new size (kept lossless).
fn resized_mask(doc: &Document, id: ObjectId, width: u32, height: u32) -> Option<Stream> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let (mask, _) = read_pixels(doc, stream).filter(|(m, _)| m.color().channel_count() == 1)?;
    let resized = mask.resize_exact(width, height, FilterType::Triangle);
    let mut dict = stream.dict.clone();
    dict.set("Width", i64::from(width));
    dict.set("Height", i64::from(height));
    dict.remove(b"DecodeParms");
    dict.remove(b"Filter");
    let mut new = Stream::new(dict, resized.as_bytes().to_vec());
    let _ = new.compress();
    Some(new)
}
