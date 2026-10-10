// Calls into the Rust side (rivet-core through Tauri).

import { invoke } from "@tauri-apps/api/core";
import type { Annotation } from "./bindings/Annotation";
import type { AnnotationBatch } from "./bindings/AnnotationBatch";
import type { DocInfo } from "./bindings/DocInfo";
import type { DocProperties } from "./bindings/DocProperties";
import type { ErrorCode } from "./bindings/ErrorCode";
import type { FieldChange } from "./bindings/FieldChange";
import type { FontInfo } from "./bindings/FontInfo";
import type { FormField } from "./bindings/FormField";
import type { MergePart } from "./bindings/MergePart";
import type { Metadata } from "./bindings/Metadata";
import type { OutlineItem } from "./bindings/OutlineItem";
import type { PageLink } from "./bindings/PageLink";
import type { PageRect } from "./bindings/PageRect";
import type { PageSlot } from "./bindings/PageSlot";
import type { Protection } from "./bindings/Protection";
import type { RepairReport } from "./bindings/RepairReport";
import type { SearchBatch } from "./bindings/SearchBatch";
import type { SearchQuery } from "./bindings/SearchQuery";
import type { TextBoxSize } from "./bindings/TextBoxSize";
import type { TextRange } from "./bindings/TextRange";
import type { TextStyle } from "./bindings/TextStyle";
import type { Degrees } from "./layout";

export interface OpenedDocument {
  docId: number;
  info: DocInfo;
}

/** Error shape returned by every Rust command. */
export interface RivetError {
  code: ErrorCode;
  detail: string;
}

export function isRivetError(e: unknown): e is RivetError {
  return typeof e === "object" && e !== null && "code" in e;
}

export function toRivetError(e: unknown): RivetError {
  return isRivetError(e) ? e : { code: "internal", detail: String(e) };
}

export function openDocument(path: string, password?: string): Promise<OpenedDocument> {
  return invoke("open_document", { path, password: password ?? null });
}

/** Facts about a document that is already open (for a window taking over its tab). */
export function documentInfo(docId: number): Promise<DocInfo> {
  return invoke("document_info", { docId });
}

export function getOutline(docId: number): Promise<OutlineItem[]> {
  return invoke("get_outline", { docId });
}

/** Annotations of the pages from `first` on, a batch of pages at a time. */
export function getAnnotationsFrom(docId: number, first: number): Promise<AnnotationBatch> {
  return invoke("get_annotations_from", { docId, first });
}

/** Fonts text boxes can use: PDFRivet's own, then the installed ones. */
export function listFonts(): Promise<FontInfo[]> {
  return invoke("list_fonts");
}

/** The size (points) a text box needs for `text`. */
export function measureText(text: string, style: TextStyle): Promise<TextBoxSize> {
  return invoke("measure_text", { text, style });
}

/** Replaces the bookmarks (written into the file on the next save). */
export function setOutline(docId: number, items: OutlineItem[]): Promise<void> {
  return invoke("set_outline", { docId, items });
}

export function getLinks(docId: number, page: number): Promise<PageLink[]> {
  return invoke("get_links", { docId, page });
}

export function getFormFields(docId: number, page: number): Promise<FormField[]> {
  return invoke("get_form_fields", { docId, page });
}

export function changeField(docId: number, page: number, field: number, change: FieldChange): Promise<void> {
  return invoke("change_field", { docId, page, field, change });
}

/** The text of a range of characters (for copying). */
export function getText(docId: number, range: TextRange): Promise<string> {
  return invoke("get_text", { docId, range });
}

/** Searches from page `first` on; returns after a batch of pages (continue from `nextPage`). */
export function searchDocument(docId: number, query: SearchQuery, first: number): Promise<SearchBatch> {
  return invoke("search_document", { docId, query, first });
}

/** The name of the user signed in to the OS (the default author of annotations). */
export function userName(): Promise<string> {
  return invoke("user_name");
}

/** Whether PDFRivet opens PDF files by default; null if that can't be told here. */
export function isDefaultPdfApp(): Promise<boolean | null> {
  return invoke("is_default_pdf_app");
}

/**
 * Makes PDFRivet the default PDF app ("done"), or opens the system settings
 * where the user can (Windows, which doesn't let apps do it themselves).
 */
export function makeDefaultPdfApp(): Promise<"done" | "opened-settings"> {
  return invoke("make_default_pdf_app");
}

export function getAnnotations(docId: number, page: number): Promise<Annotation[]> {
  return invoke("get_annotations", { docId, page });
}

/** Adds an annotation; resolves to its id. */
export function addAnnotation(docId: number, page: number, annotation: Annotation): Promise<string> {
  return invoke("add_annotation", { docId, page, annotation });
}

/** Changes an annotation (found by its id); resolves to its id. */
export function updateAnnotation(docId: number, page: number, annotation: Annotation): Promise<string> {
  return invoke("update_annotation", { docId, page, annotation });
}

/** A picture's pixels (e.g. from a canvas): RGBA, row by row. */
export interface Picture {
  width: number;
  height: number;
  data: Uint8ClampedArray;
}

/**
 * Places a picture (a signature) in `annotation.rect` as a stamp; resolves to
 * its id. The pixels are sent as binary, not JSON (see `add_image_stamp`).
 */
export function addImageStamp(docId: number, page: number, annotation: Annotation, picture: Picture): Promise<string> {
  const header = new TextEncoder().encode(
    JSON.stringify({ docId, page, annotation, width: picture.width, height: picture.height }),
  );
  const body = new Uint8Array(4 + header.length + picture.data.length);
  new DataView(body.buffer).setUint32(0, header.length, true);
  body.set(header, 4);
  body.set(picture.data, 4 + header.length);
  return invoke("add_image_stamp", body);
}

/** Brings back a deleted annotation (undo; possible until the document is saved). */
export function restoreAnnotation(docId: number, page: number, id: string): Promise<void> {
  return invoke("restore_annotation", { docId, page, id });
}

/** Hides or shows an annotation while it is dragged (so only its preview shows). */
export function setAnnotationHidden(docId: number, page: number, id: string, hidden: boolean): Promise<void> {
  return invoke("set_annotation_hidden", { docId, page, id, hidden });
}

/** Permanently removes what is under `areas` of a page (redaction). */
export function redact(docId: number, page: number, areas: PageRect[]): Promise<void> {
  return invoke("redact", { docId, page, areas });
}

export function deleteAnnotation(docId: number, page: number, id: string): Promise<void> {
  return invoke("delete_annotation", { docId, page, id });
}

/** Saves the document, including filled-in forms and annotations, to `path`. */
export function saveDocument(docId: number, path: string): Promise<void> {
  return invoke("save_document", { docId, path });
}

export function documentProperties(docId: number): Promise<DocProperties> {
  return invoke("document_properties", { docId });
}

/** Changes the title, author, subject and keywords (written into the file on save). */
export function setMetadata(docId: number, metadata: Metadata): Promise<void> {
  return invoke("set_metadata", { docId, metadata });
}

export function setVisiblePages(docId: number, first: number, last: number): Promise<void> {
  return invoke("set_visible_pages", { docId, first, last });
}

export function closeDocument(docId: number): Promise<void> {
  return invoke("close_document", { docId });
}

/** PDFs the OS asked PDFRivet to open ("Open with", a second launch). Returns them only once. */
export function takePendingFiles(): Promise<string[]> {
  return invoke("take_pending_files");
}

export function filesExist(paths: string[]): Promise<boolean[]> {
  return invoke("files_exist", { paths });
}

/** One tab for the Windows taskbar (see `taskbar_tabs.rs`). */
export interface TaskbarTab {
  id: number;
  title: string;
  docId: number;
  page: number;
  /** Page size on screen in PDF points, after rotation. */
  widthPt: number;
  heightPt: number;
  rotation: number;
}

/** Windows: one taskbar preview per tab. Does nothing on other systems. */
export function setTaskbarTabs(tabs: TaskbarTab[], active: number | null, enabled: boolean): Promise<void> {
  return invoke("set_taskbar_tabs", { tabs, active, enabled });
}

/** Windows: captures the window as the active tab's taskbar preview. */
export function captureTaskbarTab(): Promise<void> {
  return invoke("capture_taskbar_tab");
}

// Custom protocols are exposed as http://<name>.localhost on Windows/Android
// and as <name>://localhost elsewhere.
export const PROTOCOL_BASE = /Windows|Android/.test(navigator.userAgent)
  ? "http://rivet.localhost"
  : "rivet://localhost";

export interface PagePixels {
  width: number;
  height: number;
  data: ImageData;
}

export interface RenderOptions {
  scale: number;
  rotation: Degrees;
  /** Thumbnails render even when the page is far from the main view. */
  thumbnail?: boolean;
  /** Only return the page if it's already rendered at this size (never renders). */
  cachedOnly?: boolean;
  signal?: AbortSignal;
}

/**
 * Renders a page to raw pixels (`scale` 1 = 72 DPI).
 * Resolves to `null` when the engine skipped it because it scrolled out of view,
 * or, with `cachedOnly`, when it isn't rendered at this size yet.
 */
export async function renderPage(docId: number, page: number, opts: RenderOptions): Promise<PagePixels | null> {
  let url = `${PROTOCOL_BASE}/page/${docId}/${page}?scale=${opts.scale.toFixed(3)}&rot=${opts.rotation}`;
  if (opts.thumbnail) url += "&thumb=1";
  if (opts.cachedOnly) url += "&cached=1";
  const response = await fetch(url, { signal: opts.signal });
  if (response.status === 204) return null;
  if (!response.ok) {
    throw await response.json().catch(() => ({ code: "internal", detail: response.statusText }));
  }
  return readPixels(await response.arrayBuffer());
}

/** Body = width (u32 LE) + height (u32 LE) + RGBA pixels. */
function readPixels(buffer: ArrayBuffer): PagePixels {
  const header = new DataView(buffer, 0, 8);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  const data = new ImageData(new Uint8ClampedArray(buffer, 8), width, height);
  return { width, height, data };
}

/**
 * Renders one area of a page: pixels `area` of the page rendered at `scale`
 * and `rotation`, exactly as they look in the whole render (not cached).
 */
export async function renderRegion(
  docId: number,
  page: number,
  scale: number,
  rotation: number,
  area: { x: number; y: number; w: number; h: number },
): Promise<PagePixels> {
  const url =
    `${PROTOCOL_BASE}/region/${docId}/${page}?scale=${scale.toFixed(3)}&rot=${rotation}` +
    `&x=${area.x}&y=${area.y}&w=${area.w}&h=${area.h}`;
  const response = await fetch(url);
  if (!response.ok) throw await response.json().catch(() => ({ code: "internal", detail: response.statusText }));
  return readPixels(await response.arrayBuffer());
}

/** Every character of a page with its box, in the engine's binary form (see textSelect.ts). */
export async function fetchPageText(docId: number, page: number): Promise<ArrayBuffer> {
  const response = await fetch(`${PROTOCOL_BASE}/text/${docId}/${page}`);
  if (!response.ok) {
    throw await response.json().catch(() => ({ code: "internal", detail: response.statusText }));
  }
  return response.arrayBuffer();
}

// --- Page tools ---------------------------------------------------------------

/** A page change that can be undone: the kept state before it, and the document now. */
export interface PagesChanged {
  snapshot: number;
  info: DocInfo;
}

/** A new, empty document in memory (merging, images to PDF…). */
export function newDocument(): Promise<OpenedDocument> {
  return invoke("new_document");
}

/** Rearranges pages: order, rotation, deleted and inserted pages (see pages.rs). */
export function arrangePages(docId: number, slots: PageSlot[]): Promise<PagesChanged> {
  return invoke("arrange_pages", { docId, slots });
}

/** Turns pages by quarter turns (clockwise; negative: counter-clockwise). */
export function rotatePages(docId: number, pages: number[], turns: number): Promise<DocInfo> {
  return invoke("rotate_pages", { docId, pages, turns });
}

/** Copies pages of another open document into this one, starting at page `at`. */
export function importPages(docId: number, source: number, pages: number[], at: number): Promise<DocInfo> {
  return invoke("import_pages", { docId, source, pages, at });
}

/** Writes some pages (as they are now) into a new PDF file. */
export function extractPages(docId: number, pages: number[], path: string): Promise<void> {
  return invoke("extract_pages", { docId, pages, path });
}

/** Finishes a merged document (pages already imported): bookmarks, form fields; writes it. */
export function finishMerge(docId: number, parts: MergePart[], bookmarkFiles: boolean, path: string): Promise<void> {
  return invoke("finish_merge", { docId, parts, bookmarkFiles, path });
}

/** Repairs a damaged PDF into a new file. */
export function repairPdf(from: string, to: string, password?: string): Promise<RepairReport> {
  return invoke("repair_pdf", { from, to, password: password ?? null });
}

/** Adds, changes or removes password protection; applied by the next save. */
export function setProtection(docId: number, protection: Protection): Promise<void> {
  return invoke("set_protection", { docId, protection });
}

/** Checks a protected file's owner password (true if right); then its protection can change. */
export function unlockOwner(docId: number, password: string): Promise<boolean> {
  return invoke("unlock_owner", { docId, password });
}

/** Undo/redo of a page change: back to a kept state; returns the state it replaced. */
export function swapSnapshot(docId: number, snapshot: number): Promise<PagesChanged> {
  return invoke("swap_snapshot", { docId, snapshot });
}

/** Lets go of kept states that can no longer be reached by undo or redo. */
export function dropSnapshots(snapshots: number[]): Promise<void> {
  return invoke("drop_snapshots", { snapshots });
}
