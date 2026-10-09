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
/// An edited `outline` replaces the file's bookmarks.
pub(crate) fn prune(
    bytes: &[u8],
    deleted_key: &str,
    redacted: &[u32],
    outline: Option<&[OutlineItem]>,
) -> Result<Option<Vec<u8>>> {
    let failed = |e: &dyn std::fmt::Display| Error::new(ErrorCode::SaveFailed, e.to_string());
    let mut doc = Document::load_mem(bytes).map_err(|e| failed(&e))?;
    if doc.is_encrypted() {
        return Ok(None);
    }
    strip_deleted_annotations(&mut doc, deleted_key.as_bytes());
    let pages = doc.get_pages();
    for index in redacted {
        if let Some(&page) = pages.get(&(index + 1)) {
            keep_used_resources(&mut doc, page).map_err(|e| failed(&e))?;
        }
    }
    if let Some(items) = outline {
        crate::outline::write(&mut doc, items).map_err(|e| failed(&e))?;
    }
    if doc.prune_objects().is_empty() && outline.is_none() {
        return Ok(Some(bytes.to_vec()));
    }
    doc.renumber_objects();
    let mut out = Vec::with_capacity(bytes.len());
    doc.save_to(&mut out).map_err(|e| failed(&e))?;
    Ok(Some(out))
}

/// Takes annotations marked as deleted out of every page's `/Annots`.
fn strip_deleted_annotations(doc: &mut Document, key: &[u8]) {
    let is_deleted = |doc: &Document, item: &Object| {
        let dict = match item {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        };
        dict.and_then(|d| d.get(key).ok())
            .is_some_and(|v| matches!(v, Object::String(s, _) if s.as_slice() == b"1"))
    };
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    for page_id in pages {
        let Ok(annots) = doc
            .get_dictionary(page_id)
            .and_then(|p| p.get(b"Annots"))
            .cloned()
        else {
            continue;
        };
        // `/Annots` is an array, either in the page or an object of its own.
        let (array, holder) = match &annots {
            Object::Array(a) => (a.clone(), None),
            Object::Reference(id) => match doc.get_object(*id).and_then(Object::as_array) {
                Ok(a) => (a.clone(), Some(*id)),
                Err(_) => continue,
            },
            _ => continue,
        };
        let kept: Vec<Object> = array
            .iter()
            .filter(|a| !is_deleted(doc, a))
            .cloned()
            .collect();
        if kept.len() == array.len() {
            continue;
        }
        match holder {
            Some(id) => {
                doc.objects.insert(id, Object::Array(kept));
            }
            None => {
                if let Ok(page) = doc.get_dictionary_mut(page_id) {
                    page.set("Annots", Object::Array(kept));
                }
            }
        }
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
