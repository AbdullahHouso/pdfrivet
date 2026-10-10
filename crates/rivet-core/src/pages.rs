//! Changing a document's pages: their order, rotation, deleting them and
//! inserting pages from other documents or blank ones.
//!
//! Everything happens in place, in the open document, so what belongs to the
//! document rather than to a page (form fields, named destinations, metadata,
//! attachments) stays. Pages from elsewhere are copied in with PDFium's import,
//! which brings their annotations along.
//!
//! Undo works on whole-document [`Snapshot`]s: the bytes PDFium would save,
//! plus the page-keyed bookkeeping that goes with them.

#![allow(unsafe_code)]

use std::{collections::BTreeSet, sync::Arc};

use pdfium_render::prelude::*;
use serde::Deserialize;
use ts_rs::TS;

use crate::{
    Document, Error, ErrorCode, OutlineItem, Result,
    document::{RawBindings, Source, load},
};

/// Where a page of the new arrangement comes from.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum PageSource {
    /// A page of this document (0-based). Listing one twice duplicates it.
    Page { index: u32 },
    /// A page of another open document.
    Other { doc: crate::DocId, index: u32 },
    /// A new, empty page of this size (points).
    Blank { width: f32, height: f32 },
}

/// One page of a new arrangement, in order.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct PageSlot {
    pub source: PageSource,
    /// Quarter turns clockwise added to the page's own rotation (negative: counter-clockwise).
    #[serde(default)]
    pub turns: i32,
}

/// A page as it is during [`Document::arrange`]: one of the original pages,
/// or the page made for a slot.
#[derive(Clone, Copy, PartialEq)]
enum Placed {
    Original(u32),
    ForSlot(usize),
}

impl Document {
    /// Rearranges the pages as `slots` say: pages not listed are deleted, the
    /// rest are put in order, rotated, and new ones inserted. `others` are the
    /// documents that `PageSource::Other` pages come from.
    pub fn arrange(
        &mut self,
        slots: &[PageSlot],
        others: &[(crate::DocId, &Document)],
    ) -> Result<()> {
        if !self.can_assemble() {
            return Err(Error::new(
                ErrorCode::AssembleNotAllowed,
                "document permissions",
            ));
        }
        if slots.is_empty() {
            return Err(Error::new(
                ErrorCode::PageOutOfRange,
                "a document needs at least one page",
            ));
        }
        let count = self.page_count();
        // Where each original page goes: the first slot that lists it.
        let mut first_use: Vec<Option<usize>> = vec![None; count as usize];
        for (i, slot) in slots.iter().enumerate() {
            match slot.source {
                PageSource::Page { index } => {
                    self.check_index(index)?;
                    first_use[index as usize].get_or_insert(i);
                }
                PageSource::Other { doc, index } => {
                    let other = find(others, doc)?;
                    other.check_index(index)?;
                }
                PageSource::Blank { width, height } => {
                    if !(width > 0.0 && height > 0.0) {
                        return Err(Error::new(ErrorCode::PageOutOfRange, "blank page size"));
                    }
                }
            }
        }
        let outline = self.outline();

        // A second copy of this document to take duplicated pages from
        // (PDFium can't import a document's pages into itself).
        let duplicates = slots.iter().enumerate().any(|(i, slot)| {
            matches!(slot.source, PageSource::Page { index } if first_use[index as usize] != Some(i))
        });
        let copy = if duplicates {
            let bytes = self.inner.save_to_bytes()?;
            Some(
                self.pdfium
                    .load_pdf_from_byte_vec(bytes, self.password.as_deref())?,
            )
        } else {
            None
        };

        // 1. Delete the pages that aren't kept (from the end, so indexes stay valid).
        let mut placed: Vec<Placed> = Vec::with_capacity(slots.len());
        for index in (0..count).rev() {
            if first_use[index as usize].is_none() {
                self.inner.pages().get(index as PdfPageIndex)?.delete()?;
            }
        }
        placed.extend(
            (0..count)
                .filter(|&i| first_use[i as usize].is_some())
                .map(Placed::Original),
        );

        // 2. Add the new pages at the end.
        for (i, slot) in slots.iter().enumerate() {
            let at = placed.len() as PdfPageIndex;
            match slot.source {
                PageSource::Page { index } if first_use[index as usize] == Some(i) => continue,
                PageSource::Page { index } => {
                    let source = copy.as_ref().expect("made when there are duplicates");
                    self.inner.pages_mut().copy_page_from_document(
                        source,
                        index as PdfPageIndex,
                        at,
                    )?;
                }
                PageSource::Other { doc, index } => {
                    let source = &find(others, doc)?.inner;
                    self.inner.pages_mut().copy_page_from_document(
                        source,
                        index as PdfPageIndex,
                        at,
                    )?;
                }
                PageSource::Blank { width, height } => {
                    let size =
                        PdfPagePaperSize::Custom(PdfPoints::new(width), PdfPoints::new(height));
                    self.inner.pages_mut().create_page_at_index(size, at)?;
                }
            }
            placed.push(Placed::ForSlot(i));
        }

        // 3. Put every page in its place in one move.
        let order: Vec<i32> = slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let wanted = match slot.source {
                    PageSource::Page { index } if first_use[index as usize] == Some(i) => {
                        Placed::Original(index)
                    }
                    _ => Placed::ForSlot(i),
                };
                placed
                    .iter()
                    .position(|p| *p == wanted)
                    .expect("every slot was placed") as i32
            })
            .collect();
        if order.iter().enumerate().any(|(i, &at)| i as i32 != at) {
            let bindings = self.pdfium.bindings();
            let handle = bindings.get_handle_from_document(&self.inner);
            // SAFETY: `handle` is this live document's; `order` lists every page
            // exactly once (a permutation of 0..len) and outlives the call; we're
            // on the PDFium thread.
            let moved =
                unsafe { bindings.FPDF_MovePages(handle, order.as_ptr(), order.len() as _, 0) };
            if moved == 0 {
                return Err(Error::new(
                    ErrorCode::Internal,
                    "PDFium couldn't move the pages",
                ));
            }
        }

        // 4. Rotate.
        for (i, slot) in slots.iter().enumerate() {
            if slot.turns.rem_euclid(4) != 0 {
                self.turn_page(i as u32, slot.turns)?;
            }
        }

        // Page-keyed bookkeeping follows the pages that were kept.
        let new_index = |old: u32| {
            first_use
                .get(old as usize)
                .copied()
                .flatten()
                .map(|i| i as u32)
        };
        let remap = |set: &BTreeSet<u32>| set.iter().filter_map(|&p| new_index(p)).collect();
        let deleted_on = remap(&self.deleted_on.borrow());
        let redacted = remap(&self.redacted.borrow());
        *self.deleted_on.borrow_mut() = deleted_on;
        *self.redacted.borrow_mut() = redacted;
        let deleted_any = first_use.iter().any(Option::is_none);
        if deleted_any {
            // Deleted pages must really leave the file, not linger unreferenced.
            self.needs_prune.set(true);
        }
        // Bookmarks: the file's own follow their pages wherever they move, so
        // they only need rewriting when one pointed at a deleted page (or they
        // were already edited, with page numbers that are now different).
        let points_at_deleted = |items: &[OutlineItem]| {
            fn any(items: &[OutlineItem], deleted: &dyn Fn(u32) -> bool) -> bool {
                items
                    .iter()
                    .any(|i| i.page.is_some_and(deleted) || any(&i.children, deleted))
            }
            any(items, &|p| new_index(p).is_none())
        };
        if (self.edited_outline.is_some() || points_at_deleted(&outline)) && self.can_edit_outline()
        {
            self.edited_outline = Some(remap_outline(outline, &new_index));
        }
        self.unsaved_changes.set(true);
        Ok(())
    }

    /// Turns pages by quarter turns (clockwise; negative: counter-clockwise).
    pub fn rotate_pages(&mut self, pages: &[u32], turns: i32) -> Result<()> {
        if !self.can_assemble() {
            return Err(Error::new(
                ErrorCode::AssembleNotAllowed,
                "document permissions",
            ));
        }
        for &page in pages {
            self.turn_page(page, turns)?;
        }
        self.unsaved_changes.set(true);
        Ok(())
    }

    fn turn_page(&self, index: u32, turns: i32) -> Result<()> {
        let mut page = self.load_page(index)?;
        let now = match page.rotation()? {
            PdfPageRenderRotation::None => 0,
            PdfPageRenderRotation::Degrees90 => 1,
            PdfPageRenderRotation::Degrees180 => 2,
            PdfPageRenderRotation::Degrees270 => 3,
        };
        page.set_rotation(match (now + turns).rem_euclid(4) {
            1 => PdfPageRenderRotation::Degrees90,
            2 => PdfPageRenderRotation::Degrees180,
            3 => PdfPageRenderRotation::Degrees270,
            _ => PdfPageRenderRotation::None,
        });
        Ok(())
    }

    /// Copies pages of `source` into this document, starting at page `at`.
    pub fn import_pages(&mut self, source: &Document, pages: &[u32], at: u32) -> Result<()> {
        if !source.can_assemble() {
            return Err(Error::new(
                ErrorCode::AssembleNotAllowed,
                "source permissions",
            ));
        }
        let at = at.min(self.page_count());
        for (i, &page) in pages.iter().enumerate() {
            source.check_index(page)?;
            self.inner.pages_mut().copy_page_from_document(
                &source.inner,
                page as PdfPageIndex,
                (at + i as u32) as PdfPageIndex,
            )?;
        }
        self.unsaved_changes.set(true);
        Ok(())
    }

    /// The document as it is now, to come back to later (see [`Document::restore`]).
    pub fn snapshot(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            bytes: Arc::new(self.inner.save_to_bytes()?),
            deleted_on: self.deleted_on.borrow().clone(),
            redacted: self.redacted.borrow().clone(),
            needs_prune: self.needs_prune.get(),
            edited_outline: self.edited_outline.clone(),
            forms_flattened: self.forms_flattened.get(),
        })
    }

    /// Goes back to a [`Snapshot`] of this document.
    pub fn restore(&mut self, snapshot: Snapshot) -> Result<()> {
        let source = Source::Memory(snapshot.bytes);
        self.inner = load(self.pdfium, &source, &self.path, self.password.as_deref())?;
        // Reopening reads these bytes, which hold the current state, not the file's.
        self.source = source;
        self.pages_loaded.set(0);
        *self.deleted_on.borrow_mut() = snapshot.deleted_on;
        *self.redacted.borrow_mut() = snapshot.redacted;
        self.needs_prune.set(snapshot.needs_prune);
        self.edited_outline = snapshot.edited_outline;
        self.forms_flattened.set(snapshot.forms_flattened);
        self.unsaved_changes.set(true);
        Ok(())
    }
}

/// A whole document at one moment, for undoing changes to its pages.
pub struct Snapshot {
    bytes: Arc<Vec<u8>>,
    deleted_on: BTreeSet<u32>,
    redacted: BTreeSet<u32>,
    needs_prune: bool,
    edited_outline: Option<Vec<OutlineItem>>,
    forms_flattened: bool,
}

impl Snapshot {
    /// About how much memory it takes.
    pub fn size(&self) -> usize {
        self.bytes.len()
    }
}

fn find<'a>(others: &[(crate::DocId, &'a Document)], doc: crate::DocId) -> Result<&'a Document> {
    others
        .iter()
        .find(|(id, _)| *id == doc)
        .map(|(_, d)| *d)
        .ok_or_else(|| Error::new(ErrorCode::DocumentNotOpen, format!("document {doc}")))
}

/// Bookmarks with their pages renumbered; ones whose page was deleted are
/// dropped, and their children move up a level.
pub(crate) fn remap_outline(
    items: Vec<OutlineItem>,
    new_index: &dyn Fn(u32) -> Option<u32>,
) -> Vec<OutlineItem> {
    let mut out = Vec::with_capacity(items.len());
    for mut item in items {
        let children = remap_outline(std::mem::take(&mut item.children), new_index);
        match item.page {
            Some(old) => match new_index(old) {
                Some(new) => {
                    item.page = Some(new);
                    item.children = children;
                    out.push(item);
                }
                None => out.extend(children),
            },
            None => {
                item.children = children;
                out.push(item);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, page: Option<u32>, children: Vec<OutlineItem>) -> OutlineItem {
        OutlineItem {
            title: title.into(),
            page,
            children,
            origin: None,
        }
    }

    #[test]
    fn bookmarks_follow_their_pages() {
        // Pages 0 and 2 swap; page 1 is deleted.
        let map = |p: u32| match p {
            0 => Some(1),
            2 => Some(0),
            _ => None,
        };
        let items = vec![
            item("A", Some(0), vec![]),
            item("B", Some(1), vec![item("B1", Some(2), vec![])]),
            item("C", None, vec![]),
        ];
        let out = remap_outline(items, &map);
        let titles: Vec<_> = out.iter().map(|i| (i.title.as_str(), i.page)).collect();
        assert_eq!(titles, [("A", Some(1)), ("B1", Some(0)), ("C", None)]);
    }
}
