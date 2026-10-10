//! Commands for the page tools: organizing, merging, splitting, converting…
//! Like the rest, they are thin wrappers around `rivet-core`.

use rivet_core::{DocId, DocInfo, Error, PageSlot, SnapshotId};
use serde::Serialize;
use tauri::State;

use crate::{AppState, OpenedDocument, blocking};

/// A page change that can be undone: the state before it, and the document now.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Changed {
    snapshot: SnapshotId,
    info: DocInfo,
}

#[tauri::command]
pub(crate) async fn new_document(state: State<'_, AppState>) -> Result<OpenedDocument, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.new_document())
        .await
        .map(|(doc_id, info)| OpenedDocument { doc_id, info })
}

#[tauri::command]
pub(crate) async fn arrange_pages(
    doc_id: DocId,
    slots: Vec<PageSlot>,
    state: State<'_, AppState>,
) -> Result<Changed, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.arrange(doc_id, slots))
        .await
        .map(|(snapshot, info)| Changed { snapshot, info })
}

#[tauri::command]
pub(crate) async fn rotate_pages(
    doc_id: DocId,
    pages: Vec<u32>,
    turns: i32,
    state: State<'_, AppState>,
) -> Result<DocInfo, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.rotate_pages(doc_id, pages, turns)).await
}

#[tauri::command]
pub(crate) async fn import_pages(
    doc_id: DocId,
    source: DocId,
    pages: Vec<u32>,
    at: u32,
    state: State<'_, AppState>,
) -> Result<DocInfo, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.import_pages(doc_id, source, pages, at)).await
}

#[tauri::command]
pub(crate) async fn extract_pages(
    doc_id: DocId,
    pages: Vec<u32>,
    path: std::path::PathBuf,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.extract_pages(doc_id, pages, path)).await
}

/// Undo or redo of a page change (see `Engine::swap_snapshot`).
#[tauri::command]
pub(crate) async fn swap_snapshot(
    doc_id: DocId,
    snapshot: SnapshotId,
    state: State<'_, AppState>,
) -> Result<Changed, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.swap_snapshot(doc_id, snapshot))
        .await
        .map(|(snapshot, info)| Changed { snapshot, info })
}

#[tauri::command]
pub(crate) fn drop_snapshots(
    snapshots: Vec<SnapshotId>,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    state.engine()?.drop_snapshots(snapshots);
    Ok(())
}
