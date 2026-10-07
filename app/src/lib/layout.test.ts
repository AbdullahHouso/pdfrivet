import { describe, expect, it } from "vitest";
import {
  anchorAt,
  computeLayout,
  currentPage,
  fitPageZoom,
  fitWidthZoom,
  PAGE_GAP,
  PX_PER_PT,
  scrollTopFor,
  stepZoom,
  visibleRange,
} from "./layout";

const A4 = { width: 595, height: 842 };
const LANDSCAPE = { width: 842, height: 595 };

describe("computeLayout", () => {
  it("stacks pages with gaps", () => {
    const l = computeLayout([A4, A4], 1, 0);
    const h = 842 * PX_PER_PT;
    expect(l.tops).toEqual([PAGE_GAP, PAGE_GAP * 2 + h]);
    expect(l.totalHeight).toBeCloseTo(PAGE_GAP * 3 + 2 * h);
  });

  it("swaps width and height when rotated", () => {
    const l = computeLayout([A4], 1, 90);
    expect(l.widths[0]).toBeCloseTo(842 * PX_PER_PT);
    expect(l.heights[0]).toBeCloseTo(595 * PX_PER_PT);
  });
});

describe("visible pages", () => {
  // 10,000 pages: the binary search must stay fast and exact.
  const many = computeLayout(Array(10_000).fill(A4), 1, 0);
  const pageHeight = 842 * PX_PER_PT + PAGE_GAP;

  it("finds the visible range anywhere in a huge document", () => {
    const top = many.tops[5000];
    expect(visibleRange(many, top, pageHeight * 1.5)).toEqual([5000, 5001]);
  });

  it("skips a page that only ends in the gap above the viewport", () => {
    const y = many.tops[1] - PAGE_GAP / 2; // inside the gap after page 0
    expect(visibleRange(many, y, 100)[0]).toBe(1);
  });

  it("reports the page under the middle of the viewport as current", () => {
    expect(currentPage(many, many.tops[42], pageHeight)).toBe(42);
  });
});

describe("fit zoom", () => {
  it("fits the widest page to the width", () => {
    const zoom = fitWidthZoom([A4, LANDSCAPE], 0, 842 * PX_PER_PT + 2 * PAGE_GAP);
    expect(zoom).toBeCloseTo(1);
  });

  it("fits a whole page into the viewport", () => {
    const zoom = fitPageZoom(A4, 0, 2000, 842 * PX_PER_PT + 2 * PAGE_GAP);
    expect(zoom).toBeCloseTo(1);
  });
});

describe("zoom steps and anchoring", () => {
  it("steps through zoom levels", () => {
    expect(stepZoom(1, 1)).toBe(1.1);
    expect(stepZoom(1, -1)).toBe(0.9);
    expect(stepZoom(1.05, -1)).toBe(1);
  });

  it("stays at the very top of the document when rotating or zooming", () => {
    const before = computeLayout([A4, A4], 1, 0);
    const after = computeLayout([A4, A4], 0.7, 90);
    expect(scrollTopFor(after, anchorAt(before, 0, 0))).toBe(0);
  });

  it("keeps the point under the cursor in place when zooming", () => {
    const before = computeLayout(Array(20).fill(A4), 1, 0);
    const anchor = anchorAt(before, before.tops[7] + 300, 250);
    const after = computeLayout(Array(20).fill(A4), 2, 0);
    const scrollTop = scrollTopFor(after, anchor);
    const again = anchorAt(after, scrollTop, 250);
    expect(again.page).toBe(7);
    expect(again.fraction).toBeCloseTo(anchor.fraction);
  });
});
