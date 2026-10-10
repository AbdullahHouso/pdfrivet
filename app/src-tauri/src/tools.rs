//! Commands for the page tools: organizing, merging, splitting, converting…
//! Like the rest, they are thin wrappers around `rivet-core`.

use rivet_core::{DocId, DocInfo, Error, MergePart, PageSlot, Protection, SnapshotId};
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

/// Finishes a merged document (bookmarks, form fields) and writes it.
#[tauri::command]
pub(crate) async fn finish_merge(
    doc_id: DocId,
    parts: Vec<MergePart>,
    bookmark_files: bool,
    path: std::path::PathBuf,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.finish_merge(doc_id, parts, bookmark_files, path)).await
}

/// Adds, changes or removes password protection on the next save.
#[tauri::command]
pub(crate) async fn set_protection(
    doc_id: DocId,
    protection: Protection,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.set_protection(doc_id, protection)).await
}

/// Checks a protected file's owner password; true if it was right.
#[tauri::command]
pub(crate) async fn unlock_owner(
    doc_id: DocId,
    password: String,
    state: State<'_, AppState>,
) -> Result<bool, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.unlock_owner(doc_id, password)).await
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
