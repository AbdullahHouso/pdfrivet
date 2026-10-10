//! Removing what a saved file no longer uses.
//!
//! PDFium's save writes every object the document ever had, including ones
//! nothing points to any more: a deleted annotation (with its comment), or the
//! text and images removed by a redaction. Readers don't show them, but anyone
//! can still dig them out of the file. After a save that deleted or redacted
//! something, the file is rewritten with only the objects still in use.
//!
//! Annotations deleted in PDFRivet stay in the open document (hidden, so undo
//! can bring them back); they are left out of the written file here.

use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, Object, ObjectId, content::Content};

use crate::{Error, ErrorCode, OutlineItem, Result};

/// Rewrites a PDF without deleted annotations and unreferenced objects.
/// Returns `None` for password-protected files (lopdf can't write them).
/// `redacted` pages (0-based) also lose every resource their content no longer uses.
/// An edited `outline` replaces the file's bookmarks. With `replies`, replies
/// added in PDFRivet are linked to the annotations they answer. With
/// `text_boxes`, text boxes get the `/IT` name that tells readers they grow
/// with their text. With `drop_form`, the form's field list goes (its fields
/// were flattened into the pages).
pub(crate) fn prune(
    bytes: &[u8],
    deleted_key: &str,
    redacted: &[u32],
    outline: Option<&[OutlineItem]>,
    replies: bool,
    text_boxes: bool,
    drop_form: bool,
) -> Result<Option<Vec<u8>>> {
    let failed = |e: &dyn std::fmt::Display| Error::new(ErrorCode::SaveFailed, e.to_string());
    let mut doc = Document::load_mem(bytes).map_err(|e| failed(&e))?;
    if doc.is_encrypted() {
        return Ok(None);
    }
    strip_deleted_annotations(&mut doc, deleted_key.as_bytes());
    if replies {
        link_replies(&mut doc, crate::annotations::REPLY_KEY.as_bytes());
    }
    if text_boxes {
        mark_typewriters(&mut doc, crate::annotations::TYPEWRITER_KEY.as_bytes());
    }
    let pages = doc.get_pages();
    for index in redacted {
        if let Some(&page) = pages.get(&(index + 1)) {
            keep_used_resources(&mut doc, page).map_err(|e| failed(&e))?;
        }
    }
    if let Some(items) = outline {
        crate::outline::write(&mut doc, items).map_err(|e| failed(&e))?;
    }
    if drop_form && let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"AcroForm");
    }
    if doc.prune_objects().is_empty() && outline.is_none() && !replies && !text_boxes && !drop_form
    {
        return Ok(Some(bytes.to_vec()));
    }
    doc.renumber_objects();
    let mut out = Vec::with_capacity(bytes.len());
    doc.save_to(&mut out).map_err(|e| failed(&e))?;
    Ok(Some(out))
}

/// A page's `/Annots` array, and the object holding it if it isn't inline.
fn page_annots(doc: &Document, page_id: ObjectId) -> Option<(Vec<Object>, Option<ObjectId>)> {
    let annots = doc
        .get_dictionary(page_id)
        .and_then(|p| p.get(b"Annots"))
        .ok()?;
    match annots {
        Object::Array(a) => Some((a.clone(), None)),
        Object::Reference(id) => doc
            .get_object(*id)
            .and_then(Object::as_array)
            .ok()
            .map(|a| (a.clone(), Some(*id))),
        _ => None,
    }
}

fn set_page_annots(
    doc: &mut Document,
    page_id: ObjectId,
    holder: Option<ObjectId>,
    annots: Vec<Object>,
) {
    match holder {
        Some(id) => {
            doc.objects.insert(id, Object::Array(annots));
        }
        None => {
            if let Ok(page) = doc.get_dictionary_mut(page_id) {
                page.set("Annots", Object::Array(annots));
            }
        }
    }
}

fn annot_dict<'a>(doc: &'a Document, item: &'a Object) -> Option<&'a Dictionary> {
    match item {
        Object::Reference(id) => doc.get_dictionary(*id).ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

/// Takes annotations marked as deleted out of every page's `/Annots`.
fn strip_deleted_annotations(doc: &mut Document, key: &[u8]) {
    let is_deleted = |doc: &Document, item: &Object| {
        annot_dict(doc, item)
            .and_then(|d| d.get(key).ok())
            .is_some_and(|v| matches!(v, Object::String(s, _) if s.as_slice() == b"1"))
    };
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    for page_id in pages {
        let Some((array, holder)) = page_annots(doc, page_id) else {
            continue;
        };
        let kept: Vec<Object> = array
            .iter()
            .filter(|a| !is_deleted(doc, a))
            .cloned()
            .collect();
        if kept.len() != array.len() {
            set_page_annots(doc, page_id, holder, kept);
        }
    }
}

/// Turns PDFRivet's replies into standard ones: the private key naming the
/// annotation a reply answers becomes an `/IRT` reference to it. A reply whose
/// annotation is gone is left out.
fn link_replies(doc: &mut Document, key: &[u8]) {
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    for page_id in pages {
        let Some((mut array, holder)) = page_annots(doc, page_id) else {
            continue;
        };
        // A reply can only point to an annotation that is an object of its own;
        // annotations written inside the array become objects.
        if array.iter().any(|a| matches!(a, Object::Dictionary(_))) {
            for item in &mut array {
                if let Object::Dictionary(d) = item {
                    *item = Object::Reference(doc.add_object(d.clone()));
                }
            }
            set_page_annots(doc, page_id, holder, array.clone());
        }
        let name_of = |dict: &Dictionary, k: &[u8]| {
            dict.get(k)
                .ok()
                .and_then(|v| v.as_str().ok())
                .map(decode_text)
                .filter(|n| !n.is_empty())
        };
        let mut named = std::collections::HashMap::new();
        let mut replies = Vec::new();
        for item in &array {
            let (Object::Reference(id), Some(dict)) = (item, annot_dict(doc, item)) else {
                continue;
            };
            if let Some(name) = name_of(dict, b"NM") {
                named.insert(name, *id);
            }
            if let Some(parent) = name_of(dict, key) {
                replies.push((*id, parent));
            }
        }
        let mut orphans = Vec::new();
        for (id, parent) in replies {
            let Ok(dict) = doc.get_dictionary_mut(id) else {
                continue;
            };
            dict.remove(key);
            match named.get(&parent) {
                Some(parent) => {
                    dict.set("IRT", Object::Reference(*parent));
                    dict.set("RT", Object::Name(b"R".to_vec()));
                }
                None => orphans.push(id),
            }
        }
        if !orphans.is_empty() {
            let kept = array
                .into_iter()
                .filter(|a| !matches!(a, Object::Reference(id) if orphans.contains(id)))
                .collect();
            set_page_annots(doc, page_id, holder, kept);
        }
    }
}

/// Text boxes that grow with their text get `/IT /FreeTextTypeWriter` (a
/// name, which PDFium can't write); ones that don't lose it. The private key
/// saying which is removed.
fn mark_typewriters(doc: &mut Document, key: &[u8]) {
    for object in doc.objects.values_mut() {
        let dict = match object {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &mut s.dict,
            _ => continue,
        };
        let Ok(value) = dict.get(key).and_then(Object::as_str).map(<[u8]>::to_vec) else {
            continue;
        };
        dict.remove(key);
        if value == b"1" {
            dict.set("IT", Object::Name(b"FreeTextTypeWriter".to_vec()));
        } else {
            dict.remove(b"IT");
        }
    }
}

/// A PDF text string as text: UTF-16 with a byte order mark, or single bytes.
fn decode_text(bytes: &[u8]) -> String {
    match bytes.strip_prefix(&[0xFE, 0xFF]) {
        Some(utf16) => {
            let units: Vec<u16> = utf16
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect();
            String::from_utf16_lossy(&units)
        }
        None => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Resource kinds whose entries are named in content streams.
const RESOURCE_KINDS: [&[u8]; 7] = [
    b"XObject",
    b"Font",
    b"ExtGState",
    b"ColorSpace",
    b"Pattern",
    b"Shading",
    b"Properties",
];

/// Gives a page its own resources holding only what its content names, so
/// removed images, forms and fonts aren't referenced (or kept) any more.
fn keep_used_resources(doc: &mut Document, page: ObjectId) -> lopdf::Result<()> {
    let content = Content::decode(&doc.get_page_content(page))?;
    let used: BTreeSet<Vec<u8>> = content
        .operations
        .iter()
        .flat_map(|op| op.operands.iter())
        .filter_map(|o| o.as_name().ok().map(<[u8]>::to_vec))
        .collect();
    let (resources, _) = doc.get_page_resources(page)?;
    let Some(resources) = resources.cloned() else {
        return Ok(());
    };
    let mut kept = Dictionary::new();
    for (kind, value) in resources.iter() {
        if !RESOURCE_KINDS.contains(&kind.as_slice()) {
            kept.set(kind.clone(), value.clone());
            continue;
        }
        let entries = match value {
            Object::Reference(id) => doc.get_dictionary(*id)?.clone(),
            Object::Dictionary(d) => d.clone(),
            _ => continue,
        };
        let mut filtered = Dictionary::new();
        for (name, entry) in entries.iter() {
            if used.contains(name) {
                filtered.set(name.clone(), entry.clone());
            }
        }
        kept.set(kind.clone(), Object::Dictionary(filtered));
    }
    // Its own copy: other pages may share the old resources.
    doc.get_dictionary_mut(page)?
        .set("Resources", Object::Dictionary(kept));
    Ok(())
}
