// Selecting text. The engine sends every character of a page with its box
// (`rivet-core/src/text.rs`); this file finds the character under the pointer,
// grows a selection to a word or line, and turns a selection into rectangles
// to draw. Selections are ranges of character positions in PDFium's text
// order, like Chrome's PDF viewer, which keeps Arabic and other right-to-left
// text right (an invisible HTML text layer would rely on the browser's bidi,
// which doesn't match how PDFs place characters).

import type { TextRange } from "./bindings/TextRange";
import type { FractionRect } from "./layout";

/** PDFium added this character (a space or line break that isn't in the file). */
export const CHAR_GENERATED = 1;
/** The character has no box on the page. */
export const CHAR_NO_BOX = 2;

/** Values per character in the engine's binary form (4 box edges, code, flags). */
const STRIDE = 6;

/** Every character of a page. Boxes are fractions of the page (top-left origin, before view rotation). */
export interface PageText {
  count: number;
  /** Per character: left, top, right, bottom (indexes 0–3 of each group of STRIDE). */
  boxes: Float32Array;
  /** Same buffer as `boxes`: code point at +4, flags at +5. */
  words: Uint32Array;
}

/** Reads the engine's binary form (see `PageText::to_bytes` in rivet-core). */
export function parsePageText(buffer: ArrayBuffer): PageText {
  const count = new DataView(buffer).getUint32(0, true);
  return {
    count,
    boxes: new Float32Array(buffer, 8, count * STRIDE),
    words: new Uint32Array(buffer, 8, count * STRIDE),
  };
}

/** Builds a PageText from plain values (for tests). */
export function makePageText(chars: { box?: FractionRect; char: string; flags?: number }[]): PageText {
  const buffer = new ArrayBuffer(8 + chars.length * STRIDE * 4);
  new DataView(buffer).setUint32(0, chars.length, true);
  const text = parsePageText(buffer);
  chars.forEach((c, i) => {
    const b = c.box ?? { left: 0, top: 0, right: 0, bottom: 0 };
    text.boxes.set([b.left, b.top, b.right, b.bottom], i * STRIDE);
    text.words[i * STRIDE + 4] = c.char.codePointAt(0) ?? 0;
    text.words[i * STRIDE + 5] = (c.flags ?? 0) | (c.box ? 0 : CHAR_NO_BOX);
  });
  return text;
}

export function codeAt(text: PageText, i: number): number {
  return text.words[i * STRIDE + 4];
}

function flagsAt(text: PageText, i: number): number {
  return text.words[i * STRIDE + 5];
}

export function hasBox(text: PageText, i: number): boolean {
  return (flagsAt(text, i) & CHAR_NO_BOX) === 0;
}

export function boxAt(text: PageText, i: number): FractionRect {
  const o = i * STRIDE;
  const b = text.boxes;
  return { left: b[o], top: b[o + 1], right: b[o + 2], bottom: b[o + 3] };
}

/** Arabic, Hebrew, Syriac, Thaana, N'Ko… and their presentation forms. */
export function isRtlCode(code: number): boolean {
  return (code >= 0x0590 && code <= 0x08ff) || (code >= 0xfb1d && code <= 0xfdff) || (code >= 0xfe70 && code <= 0xfeff);
}

function isLineBreak(code: number): boolean {
  return code === 0x0a || code === 0x0d;
}

/** The character whose box contains the point, or -1. */
export function charAt(text: PageText, x: number, y: number): number {
  for (let i = 0; i < text.count; i++) {
    if (!hasBox(text, i)) continue;
    const o = i * STRIDE;
    const b = text.boxes;
    if (x >= b[o] && x <= b[o + 2] && y >= b[o + 1] && y <= b[o + 3]) return i;
  }
  return -1;
}

/** Whether the point is over text (to show the text cursor). */
export function isOverText(text: PageText, x: number, y: number): boolean {
  return charAt(text, x, y) >= 0;
}

/**
 * The position (0…count) between characters closest to a point, as a caret
 * would sit: before or after the character under it, depending on which half
 * was hit (mirrored for right-to-left characters). Points beside a line snap
 * to that line; points elsewhere snap to the nearest character.
 */
export function caretAt(text: PageText, x: number, y: number): number {
  let best = -1;
  let bestOnLine = false;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (let i = 0; i < text.count; i++) {
    if (!hasBox(text, i)) continue;
    const { left, top, right, bottom } = boxAt(text, i);
    const onLine = y >= top && y <= bottom;
    // Prefer characters on the pointer's line, however far along it.
    if (bestOnLine && !onLine) continue;
    const dx = x < left ? left - x : x > right ? x - right : 0;
    const dy = y < top ? top - y : y > bottom ? y - bottom : 0;
    const distance = onLine ? dx : Math.hypot(dx, dy * 4);
    if ((onLine && !bestOnLine) || distance < bestDistance) {
      best = i;
      bestOnLine = onLine;
      bestDistance = distance;
    }
  }
  if (best < 0) return 0;
  const box = boxAt(text, best);
  // Above or below the nearest character's line: before / after that character.
  if (!bestOnLine && (y < box.top || y > box.bottom)) {
    return y < box.top ? best : best + 1;
  }
  const leftHalf = x < (box.left + box.right) / 2;
  const rtl = isRtlCode(codeAt(text, best));
  return leftHalf !== rtl ? best : best + 1;
}

const WORD_CHAR = /[\p{L}\p{N}\p{M}_'’-]/u;

function isWordCode(code: number): boolean {
  return WORD_CHAR.test(String.fromCodePoint(code));
}

/** The word around character `i`: [start, end). A non-word character is its own "word". */
export function wordAt(text: PageText, i: number): [number, number] {
  if (i < 0 || i >= text.count) return [i, i];
  if (!isWordCode(codeAt(text, i))) return [i, i + 1];
  let start = i;
  let end = i + 1;
  while (start > 0 && isWordCode(codeAt(text, start - 1))) start--;
  while (end < text.count && isWordCode(codeAt(text, end))) end++;
  return [start, end];
}

/** The line around character `i`: [start, end), without its line break. */
export function lineAt(text: PageText, i: number): [number, number] {
  if (i < 0 || i >= text.count) return [i, i];
  let start = i;
  let end = i;
  while (start > 0 && !isLineBreak(codeAt(text, start - 1))) start--;
  while (end < text.count && !isLineBreak(codeAt(text, end))) end++;
  return [start, end];
}

/**
 * Rectangles covering characters [start, end): one per line, where a line is a
 * run of consecutive characters that overlap vertically.
 */
export function selectionRects(text: PageText, start: number, end: number): FractionRect[] {
  const rects: FractionRect[] = [];
  let current: FractionRect | null = null;
  for (let i = Math.max(0, start); i < Math.min(end, text.count); i++) {
    if (!hasBox(text, i)) continue;
    const b = boxAt(text, i);
    if (current) {
      const overlap = Math.min(current.bottom, b.bottom) - Math.max(current.top, b.top);
      const height = Math.min(current.bottom - current.top, b.bottom - b.top);
      if (overlap > height / 2) {
        current.left = Math.min(current.left, b.left);
        current.right = Math.max(current.right, b.right);
        current.top = Math.min(current.top, b.top);
        current.bottom = Math.max(current.bottom, b.bottom);
        continue;
      }
      rects.push(current);
    }
    current = { ...b };
  }
  if (current) rects.push(current);
  return rects;
}

/** A caret position: between characters `index - 1` and `index` of `page`. */
export interface TextPosition {
  page: number;
  index: number;
}

/** Where the selection started (anchor) and where it ends now (focus). */
export interface TextSelection {
  anchor: TextPosition;
  focus: TextPosition;
}

function before(a: TextPosition, b: TextPosition): boolean {
  return a.page < b.page || (a.page === b.page && a.index < b.index);
}

/** The selection's start and end in reading order. */
export function ordered(sel: TextSelection): [TextPosition, TextPosition] {
  return before(sel.focus, sel.anchor) ? [sel.focus, sel.anchor] : [sel.anchor, sel.focus];
}

export function isEmpty(sel: TextSelection | null): boolean {
  if (!sel) return true;
  return sel.anchor.page === sel.focus.page && sel.anchor.index === sel.focus.index;
}

/** The part of the selection on `page`: [start, end) character positions, or null. */
export function rangeOnPage(sel: TextSelection | null, page: number): [number, number] | null {
  if (!sel || isEmpty(sel)) return null;
  const [start, end] = ordered(sel);
  if (page < start.page || page > end.page) return null;
  return [page === start.page ? start.index : 0, page === end.page ? end.index : Number.MAX_SAFE_INTEGER];
}

/** The selection as the engine's TextRange (for copying). */
export function toTextRange(sel: TextSelection): TextRange {
  const [start, end] = ordered(sel);
  return { startPage: start.page, start: start.index, endPage: end.page, end: Math.min(end.index, 0xffffffff) };
}
