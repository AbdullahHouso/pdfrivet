import { describe, expect, it } from "vitest";
import {
  CHAR_GENERATED,
  caretAt,
  charAt,
  isEmpty,
  lineAt,
  makePageText,
  rangeOnPage,
  selectionRects,
  toTextRange,
  wordAt,
} from "./textSelect";

/** Characters of one line, each 0.02 wide, starting at `left` (or right to left from `left`). */
function line(text: string, top: number, left: number, rtl = false) {
  return [...text].map((char, i) => {
    const l = rtl ? left - (i + 1) * 0.02 : left + i * 0.02;
    return { char, box: { left: l, top, right: l + 0.02, bottom: top + 0.03 } };
  });
}

const newline = { char: "\n", flags: CHAR_GENERATED };

// "hello world" on one line, "second" on the next.
const page = makePageText([...line("hello world", 0.1, 0.1), newline, ...line("second", 0.2, 0.1)]);

describe("finding characters", () => {
  it("finds the character under a point", () => {
    expect(charAt(page, 0.105, 0.11)).toBe(0); // h
    expect(charAt(page, 0.5, 0.5)).toBe(-1);
  });

  it("puts the caret before or after the character, by the half that was hit", () => {
    expect(caretAt(page, 0.104, 0.11)).toBe(0);
    expect(caretAt(page, 0.116, 0.11)).toBe(1);
  });

  it("snaps points beside a line to that line", () => {
    // Far right of line 1: after "d" (index 11).
    expect(caretAt(page, 0.9, 0.11)).toBe(11);
    // Left of line 2: before "s" (index 12).
    expect(caretAt(page, 0.01, 0.21)).toBe(12);
  });

  it("snaps points above or below the text to the nearest character", () => {
    expect(caretAt(page, 0.1, 0.01)).toBe(0);
    expect(caretAt(page, 0.21, 0.9)).toBe(page.count);
  });

  it("mirrors the caret for right-to-left characters", () => {
    const arabic = makePageText(line("سلام", 0.1, 0.5, true));
    // The first letter's box is the rightmost; its right half is "before" it.
    expect(caretAt(arabic, 0.495, 0.11)).toBe(0);
    expect(caretAt(arabic, 0.485, 0.11)).toBe(1);
  });
});

describe("words and lines", () => {
  it("selects a word", () => {
    expect(wordAt(page, 7)).toEqual([6, 11]); // "world"
    expect(wordAt(page, 5)).toEqual([5, 6]); // the space alone
  });

  it("selects Arabic words with their marks", () => {
    const text = makePageText(line("كَتَبَ بك", 0.1, 0.5, true));
    expect(wordAt(text, 1)).toEqual([0, 6]);
  });

  it("selects a line without its line break", () => {
    expect(lineAt(page, 2)).toEqual([0, 11]);
    expect(lineAt(page, 13)).toEqual([12, page.count]);
  });
});

describe("selection rectangles", () => {
  it("draws one rectangle per line", () => {
    const rects = selectionRects(page, 6, 15);
    expect(rects).toHaveLength(2);
    expect(rects[0].left).toBeCloseTo(0.22);
    expect(rects[0].right).toBeCloseTo(0.32);
    expect(rects[1].left).toBeCloseTo(0.1);
    expect(rects[1].right).toBeCloseTo(0.16);
  });

  it("skips characters without a box", () => {
    expect(selectionRects(page, 11, 12)).toEqual([]);
  });
});

describe("selections across pages", () => {
  const sel = { anchor: { page: 3, index: 5 }, focus: { page: 1, index: 2 } };

  it("orders a selection made backwards", () => {
    expect(toTextRange(sel)).toEqual({ startPage: 1, start: 2, endPage: 3, end: 5 });
  });

  it("splits the selection by page", () => {
    expect(rangeOnPage(sel, 0)).toBeNull();
    expect(rangeOnPage(sel, 1)).toEqual([2, Number.MAX_SAFE_INTEGER]);
    expect(rangeOnPage(sel, 2)).toEqual([0, Number.MAX_SAFE_INTEGER]);
    expect(rangeOnPage(sel, 3)).toEqual([0, 5]);
  });

  it("treats a click without a drag as no selection", () => {
    const click = { anchor: { page: 0, index: 4 }, focus: { page: 0, index: 4 } };
    expect(isEmpty(click)).toBe(true);
    expect(rangeOnPage(click, 0)).toBeNull();
  });

  it("caps 'to the end' at the engine's u32", () => {
    const all = { anchor: { page: 0, index: 0 }, focus: { page: 2, index: Number.MAX_SAFE_INTEGER } };
    expect(toTextRange(all).end).toBe(0xffffffff);
  });
});
