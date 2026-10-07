//! The document outline (also called bookmarks or table of contents).

use pdfium_render::prelude::*;
use serde::Serialize;
use ts_rs::TS;

/// One entry in the outline. `page` is `None` when the entry doesn't point
/// to a page in this document (for example, a web link).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct OutlineItem {
    pub title: String,
    pub page: Option<u32>,
    pub children: Vec<OutlineItem>,
}

/// Outlines can be deeply nested or even (in broken files) circular,
/// so we stop at a sane depth and entry count.
const MAX_DEPTH: usize = 32;
const MAX_ITEMS: usize = 20_000;

pub(crate) fn read(doc: &PdfDocument) -> Vec<OutlineItem> {
    let mut budget = MAX_ITEMS;
    match doc.bookmarks().root() {
        Some(first) => siblings(first, 0, &mut budget),
        None => Vec::new(),
    }
}

fn siblings(first: PdfBookmark, depth: usize, budget: &mut usize) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    let mut current = Some(first);
    while let Some(bookmark) = current {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let children = match bookmark.first_child() {
            Some(child) if depth < MAX_DEPTH => siblings(child, depth + 1, budget),
            _ => Vec::new(),
        };
        items.push(OutlineItem {
            title: bookmark.title().unwrap_or_default().trim().to_owned(),
            page: bookmark
                .destination()
                .and_then(|d| d.page_index().ok())
                .map(|i| i as u32),
            children,
        });
        current = bookmark.next_sibling();
    }
    items
}
