// Text boxes on the page: what the editor needs to show text the way the
// engine will draw it (same fonts, sizes, spacing, direction), and the
// default style of new boxes.
//
// The engine lays out and draws the text (rivet-core `textlayout.rs`); the
// editor is a textarea styled to match it, so what you type looks like the
// result. Keep the numbers here in step with `textlayout.rs`.

import type { FontInfo } from "./bindings/FontInfo";
import type { TextFont } from "./bindings/TextFont";
import type { TextStyle } from "./bindings/TextStyle";
import { PROTOCOL_BASE } from "./pdf";

/** Line height as a multiple of the font size (`LINE_HEIGHT` in Rust). */
export const LINE_HEIGHT = 1.3;
/** Space between a box's edge and its text, in points (`TEXT_PADDING` in Rust). */
export const TEXT_PADDING = 2;

export const FONT_SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 60, 72];
export const MIN_SIZE = 4;
export const MAX_SIZE = 200;

export function defaultTextStyle(): TextStyle {
  return {
    font: { family: "Rubik", bundled: true },
    size: 14,
    bold: false,
    align: "auto",
    valign: "top",
    direction: "auto",
    width: null,
    height: null,
  };
}

let registered = false;

/**
 * Makes PDFRivet's own fonts available to the editor under their own names:
 * the same files the engine draws with, fetched from it (like page text) and
 * handed to the browser as data.
 */
export function registerBundledFonts() {
  if (registered || typeof document === "undefined" || typeof FontFace === "undefined") return;
  registered = true;
  const faces: [string, string, string][] = [
    ["PDFRivet Rubik", "rubik", "300 900"],
    ["PDFRivet Amiri", "amiri", "400"],
    ["PDFRivet Amiri", "amiri-bold", "700"],
  ];
  for (const [family, file, weight] of faces) {
    fetch(`${PROTOCOL_BASE}/font/${file}`)
      .then((r) => r.arrayBuffer())
      .then((data) => new FontFace(family, data, { weight }).load())
      .then((face) => document.fonts.add(face))
      .catch((e) => console.warn("[text] font not loaded", file, e));
  }
}

/** The CSS font-family for a text box's font, with the same fallbacks the engine uses. */
export function cssFamily(font: TextFont): string {
  const quote = (name: string) => `"${name.replace(/["\\]/g, "")}"`;
  const main = font.bundled ? quote(`PDFRivet ${font.family}`) : quote(font.family);
  return `${main}, "PDFRivet Rubik", "PDFRivet Amiri"`;
}

// Letters that set a paragraph's direction (Unicode "strong" characters, roughly).
const RTL_LETTER = /[\p{Script=Arabic}\p{Script=Hebrew}\p{Script=Syriac}\p{Script=Thaana}\p{Script=Nko}]/u;
const LETTER = /\p{L}/u;

/** Whether text starts right to left: its first letter is Arabic, Hebrew… */
export function startsRtl(text: string): boolean {
  for (const ch of text) {
    if (RTL_LETTER.test(ch)) return true;
    if (LETTER.test(ch)) return false;
  }
  return false;
}

/** Whether a box's (first) paragraph runs right to left, forced or from its text. */
export function isRtl(text: string, style: TextStyle): boolean {
  if (style.direction !== "auto") return style.direction === "rtl";
  return startsRtl(text.split("\n")[0] ?? "");
}

/** Fonts for the picker: PDFRivet's own first, then those with Arabic, then the rest. */
export function groupFonts(fonts: FontInfo[]): { bundled: FontInfo[]; arabic: FontInfo[]; other: FontInfo[] } {
  return {
    bundled: fonts.filter((f) => f.bundled),
    arabic: fonts.filter((f) => !f.bundled && f.arabic),
    other: fonts.filter((f) => !f.bundled && !f.arabic),
  };
}

export function clampSize(size: number): number {
  return Math.min(MAX_SIZE, Math.max(MIN_SIZE, Math.round(size * 2) / 2));
}

/** The next size up or down in the list of common sizes (Ctrl+] and Ctrl+[). */
export function stepSize(size: number, direction: 1 | -1): number {
  if (direction > 0) return FONT_SIZES.find((s) => s > size) ?? clampSize(size + 12);
  return [...FONT_SIZES].reverse().find((s) => s < size) ?? clampSize(size - 1);
}
