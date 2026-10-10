//! The document outline (also called bookmarks or table of contents).
//!
//! PDFium can read the outline but not change it. Edited bookmarks are kept
//! here while the document is open and written into the file on save with
//! lopdf ([`write`]). Entries that came from the file keep their original
//! dictionary (their destination or action, colour and style); only their
//! title and place in the tree are rewritten.

use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::textstrings::Texts;

/// One entry in the outline. `page` is `None` when the entry doesn't point
/// to a page in this document (for example, a web link).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutlineItem {
    pub title: String,
    pub page: Option<u32>,
    pub children: Vec<OutlineItem>,
    /// Which entry of the file's outline this is (its place when walking the
    /// outline in order), or `None` for a bookmark added in PDFRivet.
    #[serde(default)]
    pub origin: Option<u32>,
}

/// Outlines can be deeply nested or even (in broken files) circular,
/// so we stop at a sane depth and entry count.
const MAX_DEPTH: usize = 32;
const MAX_ITEMS: usize = 20_000;

pub(crate) fn read(doc: &PdfDocument, texts: &Texts) -> Vec<OutlineItem> {
    let mut walk = Walk::default();
    match doc.bookmarks().root() {
        Some(first) => siblings(first, 0, &mut walk, texts),
        None => Vec::new(),
    }
}

/// Counts entries as they are visited (the same order as [`original_entries`]).
#[derive(Default)]
struct Walk {
    seen: u32,
}

fn siblings(first: PdfBookmark, depth: usize, walk: &mut Walk, texts: &Texts) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    let mut current = Some(first);
    while let Some(bookmark) = current {
        if walk.seen as usize >= MAX_ITEMS {
            break;
        }
        let origin = walk.seen;
        walk.seen += 1;
        let children = match bookmark.first_child() {
            Some(child) if depth < MAX_DEPTH => siblings(child, depth + 1, walk, texts),
            _ => Vec::new(),
        };
        items.push(OutlineItem {
            title: texts
                .repair_title(bookmark.title().unwrap_or_default())
                .trim()
                .to_owned(),
            page: bookmark
                .destination()
                .and_then(|d| d.page_index().ok())
                .map(|i| i as u32),
            children,
            origin: Some(origin),
        });
        current = bookmark.next_sibling();
    }
    items
}

/// The file's outline entries in the order [`read`] visits them.
fn original_entries(doc: &Document) -> Vec<ObjectId> {
    fn visit(
        doc: &Document,
        first: Option<ObjectId>,
        depth: usize,
        seen: &mut BTreeSet<ObjectId>,
        out: &mut Vec<ObjectId>,
    ) {
        let mut current = first;
        while let Some(id) = current {
            // A loop in a broken file: stop, as PDFium does.
            if out.len() >= MAX_ITEMS || !seen.insert(id) {
                break;
            }
            out.push(id);
            let Ok(dict) = doc.get_dictionary(id) else {
                break;
            };
            if depth < MAX_DEPTH {
                visit(doc, reference(dict, b"First"), depth + 1, seen, out);
            }
            current = reference(dict, b"Next");
        }
    }
    let mut out = Vec::new();
    let first = outlines_root(doc).and_then(|root| reference(root, b"First"));
    visit(doc, first, 0, &mut BTreeSet::new(), &mut out);
    out
}

fn reference(dict: &Dictionary, key: &[u8]) -> Option<ObjectId> {
    dict.get(key).and_then(Object::as_reference).ok()
}

fn outlines_root(doc: &Document) -> Option<&Dictionary> {
    let catalog = doc.catalog().ok()?;
    match catalog.get(b"Outlines").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

/// Replaces the file's outline with `items`. The old entries nothing refers
/// to any more are left for the caller's pruning to remove.
pub(crate) fn write(doc: &mut Document, items: &[OutlineItem]) -> lopdf::Result<()> {
    let originals = original_entries(doc);
    let pages = doc.get_pages();
    let root = doc.new_object_id();
    let written = write_level(doc, items, root, &originals, &pages)?;
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Outlines".to_vec()));
    if let Some((first, last, visible)) = written {
        dict.set("First", first);
        dict.set("Last", last);
        dict.set("Count", visible as i64);
    }
    doc.objects.insert(root, Object::Dictionary(dict));
    let catalog = doc.catalog_mut()?;
    if items.is_empty() {
        catalog.remove(b"Outlines");
    } else {
        catalog.set("Outlines", root);
    }
    Ok(())
}

/// Writes one level of the tree under `parent`. Returns its first and last
/// entries and how many entries show under the parent when it is open.
fn write_level(
    doc: &mut Document,
    items: &[OutlineItem],
    parent: ObjectId,
    originals: &[ObjectId],
    pages: &std::collections::BTreeMap<u32, ObjectId>,
) -> lopdf::Result<Option<(ObjectId, ObjectId, usize)>> {
    let ids: Vec<ObjectId> = items.iter().map(|_| doc.new_object_id()).collect();
    let mut visible = 0;
    for (i, item) in items.iter().enumerate() {
        let original = item
            .origin
            .and_then(|o| originals.get(o as usize))
            .and_then(|id| doc.get_dictionary(*id).ok())
            .cloned();
        // A closed entry (negative /Count in the file) stays closed.
        let closed = original
            .as_ref()
            .and_then(|d| d.get(b"Count").and_then(Object::as_i64).ok())
            .is_some_and(|c| c < 0);
        let mut dict = match original {
            Some(mut d) => {
                for key in [b"First".as_slice(), b"Last", b"Next", b"Prev", b"Count"] {
                    d.remove(key);
                }
                d
            }
            None => {
                let mut d = Dictionary::new();
                if let Some(dest) = item.page.and_then(|p| destination(doc, pages, p)) {
                    d.set("Dest", dest);
                }
                d
            }
        };
        dict.set("Title", text_string(&item.title));
        dict.set("Parent", parent);
        if i > 0 {
            dict.set("Prev", ids[i - 1]);
        }
        if let Some(next) = ids.get(i + 1) {
            dict.set("Next", *next);
        }
        let children = write_level(doc, &item.children, ids[i], originals, pages)?;
        visible += 1;
        if let Some((first, last, count)) = children {
            dict.set("First", first);
            dict.set("Last", last);
            dict.set(
                "Count",
                if closed {
                    -(count as i64)
                } else {
                    count as i64
                },
            );
            if !closed {
                visible += count;
            }
        }
        doc.objects.insert(ids[i], Object::Dictionary(dict));
    }
    Ok(match (ids.first(), ids.last()) {
        (Some(first), Some(last)) => Some((*first, *last, visible)),
        _ => None,
    })
}

/// Goes to the top of page `index` (0-based), keeping the reader's zoom.
fn destination(
    doc: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    index: u32,
) -> Option<Object> {
    let page = *pages.get(&(index + 1))?;
    let top = media_box_top(doc, page).map_or(Object::Null, Object::Real);
    Some(Object::Array(vec![
        Object::Reference(page),
        Object::Name(b"XYZ".to_vec()),
        Object::Null,
        top,
        Object::Null,
    ]))
}

/// The top of a page's media box (inherited from its parents if need be).
fn media_box_top(doc: &Document, page: ObjectId) -> Option<f32> {
    let mut node = doc.get_dictionary(page).ok()?;
    for _ in 0..MAX_DEPTH {
        if let Ok(b) = node.get(b"MediaBox") {
            let b = match b {
                Object::Reference(id) => doc.get_object(*id).ok()?,
                other => other,
            };
            let values = b.as_array().ok()?;
            let y = |i: usize| values.get(i).and_then(|v| v.as_float().ok());
            return Some(y(1)?.max(y(3)?));
        }
        node = doc.get_dictionary(reference(node, b"Parent")?).ok()?;
    }
    None
}

/// A PDF text string: plain bytes for ASCII, otherwise UTF-16 with a byte order mark.
fn text_string(text: &str) -> Object {
    if text.is_ascii() {
        Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}
