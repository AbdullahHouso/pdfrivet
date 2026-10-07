//! Clickable links on a page ("go to page 5", web addresses).

use pdfium_render::prelude::*;
use serde::Serialize;
use ts_rs::TS;

/// Where a link goes.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum LinkTarget {
    /// A page in this document (0-based).
    Page { page: u32 },
    /// A web or mail address.
    Uri { uri: String },
}

/// A link area on the page. The rectangle is given as fractions (0..1) of the
/// page, measured from its top-left corner, before any view rotation, so the
/// UI can place it at any zoom.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PageLink {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub target: LinkTarget,
}

pub(crate) fn read(page: &PdfPage) -> Vec<PageLink> {
    let width = page.width().value;
    let height = page.height().value;
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    page.links()
        .iter()
        .filter_map(|link| {
            let target = target_of(&link)?;
            let rect = link.rect().ok()?;
            Some(PageLink {
                left: (rect.left().value / width).clamp(0.0, 1.0),
                right: (rect.right().value / width).clamp(0.0, 1.0),
                // PDF coordinates start at the bottom; the UI's start at the top.
                top: (1.0 - rect.top().value / height).clamp(0.0, 1.0),
                bottom: (1.0 - rect.bottom().value / height).clamp(0.0, 1.0),
                target,
            })
        })
        .collect()
}

fn target_of(link: &PdfLink) -> Option<LinkTarget> {
    if let Some(dest) = link.destination() {
        return dest
            .page_index()
            .ok()
            .map(|p| LinkTarget::Page { page: p as u32 });
    }
    let action = link.action()?;
    if let Some(local) = action.as_local_destination_action() {
        let page = local.destination().ok()?.page_index().ok()?;
        return Some(LinkTarget::Page { page: page as u32 });
    }
    let uri = action.as_uri_action()?.uri().ok()?;
    // Only open safe kinds of addresses; never local files or scripts.
    let lower = uri.trim().to_ascii_lowercase();
    let safe = ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme));
    safe.then(|| LinkTarget::Uri {
        uri: uri.trim().to_owned(),
    })
}
