// Splitting a document into several files: which pages go into each file,
// and what the files are called. Pure, so it's unit-tested.

import { westernDigits } from "../pageRange";

export interface SplitPart {
  /** 0-based pages, in order. */
  pages: number[];
  /** Names the file, if the part has a name of its own (a bookmark's title). */
  title?: string;
}

/** A file every `size` pages. */
export function splitEvery(pageCount: number, size: number): SplitPart[] {
  const n = Math.max(1, Math.floor(size));
  const parts: SplitPart[] = [];
  for (let first = 0; first < pageCount; first += n) {
    parts.push({ pages: range(first, Math.min(pageCount, first + n) - 1) });
  }
  return parts;
}

/**
 * A file per range typed, e.g. "1-3, 4-10, 12" (1-based, any order). Null if
 * the text is invalid or mentions pages that don't exist.
 */
export function splitRanges(text: string, pageCount: number): SplitPart[] | null {
  const groups = westernDigits(text)
    .split(/[,،;]+/)
    .map((g) => g.trim())
    .filter(Boolean);
  if (groups.length === 0) return null;
  const parts: SplitPart[] = [];
  for (const group of groups) {
    const match = /^(\d+)(?:\s*[-–]\s*(\d+))?$/.exec(group);
    if (!match) return null;
    const from = Number(match[1]);
    const to = match[2] === undefined ? from : Number(match[2]);
    if (from < 1 || to < 1 || from > pageCount || to > pageCount) return null;
    parts.push({ pages: from <= to ? range(from - 1, to - 1) : range(to - 1, from - 1).reverse() });
  }
  return parts;
}

/**
 * A file per top-level bookmark: from its page up to the next one's. Pages
 * before the first bookmark make a file of their own.
 */
export function splitAtBookmarks(bookmarks: { title: string; page: number | null }[], pageCount: number): SplitPart[] {
  const starts = new Map<number, string>();
  for (const b of bookmarks) {
    if (b.page !== null && b.page >= 0 && b.page < pageCount && !starts.has(b.page)) starts.set(b.page, b.title);
  }
  const pages = [...starts.keys()].sort((a, b) => a - b);
  if (pages.length === 0) return [];
  const parts: SplitPart[] = [];
  if (pages[0] > 0) parts.push({ pages: range(0, pages[0] - 1) });
  pages.forEach((first, i) => {
    const last = (pages[i + 1] ?? pageCount) - 1;
    parts.push({ pages: range(first, last), title: starts.get(first) });
  });
  return parts;
}

/** Characters no file system allows in a name. */
const UNSAFE = /[\\/:*?"<>|]/g;

/** A title made safe to use as a file name (empty if nothing is left). */
export function safeFileName(title: string): string {
  // Control characters aren't allowed either.
  const visible = [...title].map((c) => (c.charCodeAt(0) < 32 ? " " : c)).join("");
  return visible
    .replace(UNSAFE, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/, "")
    .slice(0, 80)
    .trim();
}

/**
 * File names for the parts: "name-1.pdf", "name-2.pdf"… (numbers padded so
 * they sort), or "name - Title.pdf" for parts with a title. A name that is
 * already `taken` (or used by another part) gets " (2)", " (3)"…
 */
export function partFileNames(base: string, parts: SplitPart[], taken: Set<string>): string[] {
  const width = String(parts.length).length;
  const used = new Set([...taken].map((t) => t.toLowerCase()));
  return parts.map((part, i) => {
    const title = part.title ? safeFileName(part.title) : "";
    const stem = title ? `${base} - ${title}` : `${base}-${String(i + 1).padStart(width, "0")}`;
    let name = `${stem}.pdf`;
    for (let k = 2; used.has(name.toLowerCase()); k++) name = `${stem} (${k}).pdf`;
    used.add(name.toLowerCase());
    return name;
  });
}

function range(first: number, last: number): number[] {
  return Array.from({ length: last - first + 1 }, (_, i) => first + i);
}
