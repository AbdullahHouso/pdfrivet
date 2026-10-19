//! Invisible text under flattened right-to-left field values.
//!
//! Those values are drawn as outlines (see `formtext`). Once a form is
//! flattened the field (and its value) is gone, and outlines can't be
//! searched, selected or copied. So the same characters are written on the
//! page as invisible text (render mode 3, as OCR tools do), each where its
//! glyph is drawn. The font is the standard Helvetica every reader has, with
//! a `ToUnicode` map that says which characters each code stands for. Only
//! codes that have a glyph in Helvetica are used (PDFium leaves out text
//! whose glyphs take no room): 189 per font, and more fonts when needed.

use std::collections::HashMap;

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::formtext::DrawnText;

/// The fonts' names in each page's resources (numbered).
const FONT_NAME: &str = "RivetText";
/// The codes a font uses, in order: those with a glyph in Helvetica
/// (`WinAnsiEncoding`).
fn codes() -> impl Iterator<Item = u8> {
    (0x21..=0x7E).chain(0xA1..=0xFF)
}
/// How many codes a font has.
const PER_FONT: usize = (0x7E - 0x21 + 1) + (0xFF - 0xA1 + 1);

/// Adds the invisible text of `drawn` (page index, values drawn on it) to
/// the document `bytes`. `None` if the document can't be changed this way.
pub(crate) fn add(bytes: &[u8], drawn: &[(u32, Vec<DrawnText>)]) -> Option<Vec<u8>> {
    let mut doc = lopdf::Document::load_mem(bytes).ok()?;
    if doc.trailer.has(b"Encrypt") {
        return None;
    }
    let pages = doc.get_pages();

    // One code per distinct piece of text, shared by every page: piece `i`
    // is code `i % PER_FONT` of font `i / PER_FONT`.
    let mut pieces: HashMap<String, usize> = HashMap::new();
    let mut widths: Vec<f32> = Vec::new();
    let mut contents: Vec<(ObjectId, String)> = Vec::new();
    for (index, texts) in drawn {
        let Some(&page_id) = pages.get(&(index + 1)) else {
            continue;
        };
        let mut content = String::new();
        for text in texts {
            let m = text.matrix;
            let size = text.size;
            for glyph in text.glyphs.iter().filter(|g| !g.text.is_empty()) {
                let next = pieces.len();
                let piece = *pieces.entry(glyph.text.clone()).or_insert(next);
                if piece == widths.len() {
                    widths.push(0.0);
                }
                widths[piece] = widths[piece].max(glyph.advance / size * 1000.0);
                let font = piece / PER_FONT;
                let code = codes().nth(piece % PER_FONT).unwrap_or(b'!');
                let (x, y) = (glyph.x, glyph.y);
                content.push_str(&format!(
                    "/{FONT_NAME}{font} 1 Tf {:.4} {:.4} {:.4} {:.4} {:.3} {:.3} Tm <{code:02X}> Tj\n",
                    m[0] * size,
                    m[1] * size,
                    m[2] * size,
                    m[3] * size,
                    m[0] * x + m[2] * y + m[4],
                    m[1] * x + m[3] * y + m[5],
                ));
            }
        }
        if !content.is_empty() {
            contents.push((page_id, content));
        }
    }
    if contents.is_empty() {
        return None;
    }

    let mut by_piece: Vec<(usize, &str)> = pieces.iter().map(|(t, &i)| (i, t.as_str())).collect();
    by_piece.sort_unstable();
    let fonts: Vec<ObjectId> = by_piece
        .chunks(PER_FONT)
        .zip(widths.chunks(PER_FONT))
        .map(|(texts, widths)| add_font(&mut doc, texts, widths))
        .collect();
    for (page_id, text) in contents {
        for (i, &font) in fonts.iter().enumerate() {
            add_to_resources(&mut doc, page_id, &format!("{FONT_NAME}{i}"), font)?;
        }
        let content = format!("Q\nq BT 3 Tr\n{text}ET Q\n");
        wrap_contents(&mut doc, page_id, content.into_bytes())?;
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).ok()?;
    Some(out)
}

/// Helvetica with its codes (see [`codes`]) standing for `texts` (in
/// order), each as wide as its glyph was drawn.
fn add_font(doc: &mut lopdf::Document, texts: &[(usize, &str)], widths: &[f32]) -> ObjectId {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    let mapped: Vec<(u8, &str)> = codes().zip(texts.iter().map(|(_, t)| *t)).collect();
    for chunk in mapped.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (code, text) in chunk {
            let utf16: String = text.encode_utf16().map(|u| format!("{u:04X}")).collect();
            cmap.push_str(&format!("<{code:02X}> <{utf16}>\n"));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let to_unicode = doc.add_object(Stream::new(Dictionary::new(), cmap.into_bytes()));
    // Widths of every code from the first to the last (unused ones 0).
    let mut all = vec![0.0f32; 0xFF - 0x21 + 1];
    for (code, &width) in codes().zip(widths) {
        all[usize::from(code) - 0x21] = width.max(1.0);
    }
    let widths: Vec<Object> = all.into_iter().map(Object::Real).collect();
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
        "FirstChar" => 0x21,
        "LastChar" => 0xFF,
        "Widths" => widths,
        "ToUnicode" => to_unicode,
    })
}

/// Adds `font` to a page's fonts as `name`. A page that takes its resources
/// from the page tree gets its own copy of them first.
fn add_to_resources(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    name: &str,
    font: ObjectId,
) -> Option<()> {
    let resources_id = match doc.get_dictionary(page_id).ok()?.get(b"Resources") {
        Ok(Object::Reference(id)) => Some(*id),
        Ok(Object::Dictionary(_)) => None,
        _ => {
            let inherited = inherited_resources(doc, page_id).unwrap_or_default();
            doc.get_dictionary_mut(page_id)
                .ok()?
                .set("Resources", inherited);
            None
        }
    };
    // The fonts may be the page's own dictionary, or one shared with other pages.
    let fonts_id = match resources(doc, page_id, resources_id)?.get(b"Font") {
        Ok(Object::Reference(id)) => Some(*id),
        Ok(Object::Dictionary(_)) => None,
        _ => {
            resources(doc, page_id, resources_id)?.set("Font", Dictionary::new());
            None
        }
    };
    let fonts = match fonts_id {
        Some(id) => doc.get_dictionary_mut(id).ok()?,
        None => resources(doc, page_id, resources_id)?
            .get_mut(b"Font")
            .ok()?
            .as_dict_mut()
            .ok()?,
    };
    fonts.set(name.as_bytes(), font);
    Some(())
}

/// A page's resource dictionary: the object `id`, or the page's own.
fn resources(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    id: Option<ObjectId>,
) -> Option<&mut Dictionary> {
    match id {
        Some(id) => doc.get_dictionary_mut(id).ok(),
        None => doc
            .get_dictionary_mut(page_id)
            .ok()?
            .get_mut(b"Resources")
            .ok()?
            .as_dict_mut()
            .ok(),
    }
}

/// The resources a page takes from its parents in the page tree.
fn inherited_resources(doc: &lopdf::Document, page_id: ObjectId) -> Option<Dictionary> {
    let mut node = doc.get_dictionary(page_id).ok()?;
    for _ in 0..64 {
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = doc.get_dictionary(parent).ok()?;
        if let Ok(resources) = node.get(b"Resources") {
            return doc.dereference(resources).ok()?.1.as_dict().ok().cloned();
        }
    }
    None
}

/// Adds `content` after the page's own content, which is put between `q`
/// and `Q` first so whatever state it leaves doesn't move the new text. (The
/// new content starts with that `Q`.)
fn wrap_contents(doc: &mut lopdf::Document, page_id: ObjectId, content: Vec<u8>) -> Option<()> {
    // One stream, or a list of them (which may itself be a separate object).
    let existing: Vec<Object> = match doc.get_dictionary(page_id).ok()?.get(b"Contents") {
        Ok(Object::Reference(id)) => match doc.get_object(*id).ok()? {
            Object::Array(items) => items.clone(),
            _ => vec![Object::Reference(*id)],
        },
        Ok(Object::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let close = doc.add_object(Stream::new(Dictionary::new(), content));
    let mut all = vec![Object::Reference(open)];
    all.extend(existing);
    all.push(Object::Reference(close));
    doc.get_dictionary_mut(page_id).ok()?.set("Contents", all);
    Some(())
}
