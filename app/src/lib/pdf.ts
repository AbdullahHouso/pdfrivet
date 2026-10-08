// Calls into the Rust side (rivet-core through Tauri).

import { invoke } from "@tauri-apps/api/core";
import type { DocInfo } from "./bindings/DocInfo";
import type { DocProperties } from "./bindings/DocProperties";
import type { ErrorCode } from "./bindings/ErrorCode";
import type { FieldChange } from "./bindings/FieldChange";
import type { FormField } from "./bindings/FormField";
import type { Metadata } from "./bindings/Metadata";
import type { OutlineItem } from "./bindings/OutlineItem";
import type { PageLink } from "./bindings/PageLink";
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

export function getOutline(docId: number): Promise<OutlineItem[]> {
  return invoke("get_outline", { docId });
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

/** Saves the document, including filled-in forms, to `path`. */
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

// Custom protocols are exposed as http://<name>.localhost on Windows/Android
// and as <name>://localhost elsewhere.
const PROTOCOL_BASE = /Windows|Android/.test(navigator.userAgent) ? "http://rivet.localhost" : "rivet://localhost";

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
  signal?: AbortSignal;
}

/**
 * Renders a page to raw pixels (`scale` 1 = 72 DPI).
 * Resolves to `null` when the engine skipped it because it scrolled out of view.
 */
export async function renderPage(docId: number, page: number, opts: RenderOptions): Promise<PagePixels | null> {
  let url = `${PROTOCOL_BASE}/page/${docId}/${page}?scale=${opts.scale.toFixed(3)}&rot=${opts.rotation}`;
  if (opts.thumbnail) url += "&thumb=1";
  const response = await fetch(url, { signal: opts.signal });
  if (response.status === 204) return null;
  if (!response.ok) {
    throw await response.json().catch(() => ({ code: "internal", detail: response.statusText }));
  }
  const buffer = await response.arrayBuffer();
  // Body = width (u32 LE) + height (u32 LE) + RGBA pixels.
  const header = new DataView(buffer, 0, 8);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  const data = new ImageData(new Uint8ClampedArray(buffer, 8), width, height);
  return { width, height, data };
}
