// Calls into the Rust side (rivet-core through Tauri).

import { invoke } from "@tauri-apps/api/core";
import type { DocInfo } from "./bindings/DocInfo";
import type { ErrorCode } from "./bindings/ErrorCode";

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

export function openDocument(path: string): Promise<OpenedDocument> {
  return invoke("open_document", { path });
}

/** The PDF Rivet was launched with ("Open with"), if any. Returns it only once. */
export function takeInitialFile(): Promise<string | null> {
  return invoke("take_initial_file");
}

export function closeDocument(docId: number): Promise<void> {
  return invoke("close_document", { docId });
}

// Custom protocols are exposed as http://<name>.localhost on Windows/Android
// and as <name>://localhost elsewhere.
const PROTOCOL_BASE = /Windows|Android/.test(navigator.userAgent) ? "http://rivet.localhost" : "rivet://localhost";

export interface PagePixels {
  width: number;
  height: number;
  data: ImageData;
}

/** Renders a page to raw pixels. `scale` 1 = 72 DPI. */
export async function renderPage(
  docId: number,
  page: number,
  scale: number,
  signal?: AbortSignal,
): Promise<PagePixels> {
  const url = `${PROTOCOL_BASE}/page/${docId}/${page}?scale=${scale.toFixed(3)}`;
  const response = await fetch(url, { signal });
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
