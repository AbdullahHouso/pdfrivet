// Pages that were just on screen, kept as canvases (within a small memory
// budget) so a page that comes back appears in the same frame, without asking
// the engine again. The engine has its own cache, but getting pixels from it
// still means sending megabytes through the webview, which shows the empty
// page for a moment. Pages about to be shown (the next page when turning
// pages) can be prepared ahead (`prefetch`).

import type { Degrees } from "./layout";
import { renderPage } from "./pdf";

/** Memory for kept pages: a few pages at any zoom. */
const BUDGET = 32 * 1024 * 1024;

interface Entry {
  canvas: HTMLCanvasElement;
  scale: number;
  bytes: number;
}

const entries = new Map<string, Entry>();
let used = 0;
const pending = new Set<string>();

const key = (docId: number, page: number, rotation: Degrees, revision: number) =>
  `${docId}/${page}/${rotation}/${revision}`;

function evict() {
  for (const [k, e] of entries) {
    if (used <= BUDGET) break;
    entries.delete(k);
    used -= e.bytes;
  }
}

/** Keeps a page's pixels (the canvas is no longer shown). */
export function keep(
  docId: number,
  page: number,
  rotation: Degrees,
  revision: number,
  canvas: HTMLCanvasElement,
  scale: number,
) {
  const bytes = canvas.width * canvas.height * 4;
  if (bytes === 0 || bytes > BUDGET / 2) return;
  const k = key(docId, page, rotation, revision);
  const old = entries.get(k);
  if (old) used -= old.bytes;
  entries.delete(k);
  entries.set(k, { canvas, scale, bytes });
  used += bytes;
  evict();
}

/** Takes kept pixels of a page, if they were rendered at (nearly) this scale. */
export function take(docId: number, page: number, rotation: Degrees, revision: number, scale: number) {
  const k = key(docId, page, rotation, revision);
  const e = entries.get(k);
  if (!e || Math.abs(e.scale - scale) >= 0.01) return null;
  entries.delete(k);
  used -= e.bytes;
  return e;
}

/** Renders a page ahead of time, so it shows at once when it comes on screen. */
export async function prefetch(docId: number, page: number, rotation: Degrees, revision: number, scale: number) {
  const k = key(docId, page, rotation, revision);
  const e = entries.get(k);
  if ((e && Math.abs(e.scale - scale) < 0.01) || pending.has(k)) return;
  pending.add(k);
  try {
    const pixels = await renderPage(docId, page, { scale, rotation });
    if (!pixels) return;
    const canvas = document.createElement("canvas");
    canvas.width = pixels.width;
    canvas.height = pixels.height;
    canvas.getContext("2d")?.putImageData(pixels.data, 0, 0);
    keep(docId, page, rotation, revision, canvas, scale);
  } catch {
    // Only a head start; the page renders normally when shown.
  } finally {
    pending.delete(k);
  }
}

/** Drops a closed document's pages. */
export function forgetPages(docId: number) {
  for (const [k, e] of entries) {
    if (k.startsWith(`${docId}/`)) {
      entries.delete(k);
      used -= e.bytes;
    }
  }
}
