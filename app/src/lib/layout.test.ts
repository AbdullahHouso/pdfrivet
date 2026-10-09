import { describe, expect, it } from "vitest";
import {
  anchorAt,
  clampSidebar,
  computeLayout,
  currentPage,
  fitPageZoom,
  fitWidthZoom,
  maxThumbnailColumns,
  PAGE_GAP,
  PX_PER_PT,
  pageAtPoint,
  rotateRect,
  SIDEBAR_MIN,
  scrollTopFor,
  stepZoom,
  THUMB_GAP,
  THUMB_LABEL,
  thumbnailGrid,
  unrotatePoint,
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
    const zoom = fitPageZoom([A4], 0, 0, 2000, 842 * PX_PER_PT + 2 * PAGE_GAP);
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

describe("rotateRect", () => {
  // A small box in the top-left corner of the page.
  const corner = { left: 0, top: 0, right: 0.2, bottom: 0.1 };

  it("moves the top-left corner clockwise", () => {
    expect(rotateRect(corner, 90)).toEqual({ left: 0.9, top: 0, right: 1, bottom: 0.2 });
    expect(rotateRect(corner, 180)).toEqual({ left: 0.8, top: 0.9, right: 1, bottom: 1 });
    expect(rotateRect(corner, 270)).toEqual({ left: 0, top: 0.8, right: 0.1, bottom: 1 });
  });

  it("returns to the start after four turns", () => {
    let r = corner;
    for (let i = 0; i < 4; i++) r = rotateRect(r, 90);
    for (const key of ["left", "top", "right", "bottom"] as const) expect(r[key]).toBeCloseTo(corner[key]);
  });
});

describe("two pages side by side", () => {
  const l = computeLayout([A4, A4, A4], 1, 0, { columns: 2 });
  const w = 595 * PX_PER_PT;

  it("puts pairs of pages in rows", () => {
    expect(l.rows.map((r) => [r.first, r.last])).toEqual([
      [0, 1],
      [2, 2],
    ]);
    expect(l.tops[0]).toBe(l.tops[1]);
    expect(l.lefts[1]).toBeCloseTo(l.lefts[0] + w + PAGE_GAP);
    expect(l.totalWidth).toBeCloseTo(2 * w + 3 * PAGE_GAP);
  });

  it("puts the first page on the right for right-to-left reading", () => {
    const rtl = computeLayout([A4, A4], 1, 0, { columns: 2, rtl: true });
    expect(rtl.lefts[0]).toBeGreaterThan(rtl.lefts[1]);
  });

  it("reports whole rows as visible and the row's first page as current", () => {
    expect(visibleRange(l, 0, 100)).toEqual([0, 1]);
    expect(currentPage(l, l.rows[1].top, 400)).toBe(2);
  });

  it("fits a spread to the width", () => {
    expect(fitWidthZoom([A4, A4], 0, 2 * w + 3 * PAGE_GAP, 2)).toBeCloseTo(1);
  });
});

describe("points on rotated pages", () => {
  it("undoes rotateRect for points", () => {
    const r = { left: 0.1, top: 0.2, right: 0.3, bottom: 0.5 };
    for (const rotation of [0, 90, 180, 270] as const) {
      const shown = rotateRect(r, rotation);
      // The rotated rectangle's corners map back to the original's corners.
      const a = unrotatePoint({ x: shown.left, y: shown.top }, rotation);
      const b = unrotatePoint({ x: shown.right, y: shown.bottom }, rotation);
      expect(Math.min(a.x, b.x)).toBeCloseTo(r.left);
      expect(Math.max(a.x, b.x)).toBeCloseTo(r.right);
      expect(Math.min(a.y, b.y)).toBeCloseTo(r.top);
      expect(Math.max(a.y, b.y)).toBeCloseTo(r.bottom);
    }
  });

  it("finds the page under a point, or the nearest one in a gap", () => {
    const l = computeLayout([A4, A4], 1, 0);
    const hit = pageAtPoint(l, [0, 1], l.lefts[1] + l.widths[1] / 2, l.tops[1] + l.heights[1] / 4);
    expect(hit?.page).toBe(1);
    expect(hit?.point.x).toBeCloseTo(0.5);
    expect(hit?.point.y).toBeCloseTo(0.25);
    // Just above page 2, in the gap: still page 2, slightly above its top.
    const gap = pageAtPoint(l, [0, 1], l.lefts[1] + 10, l.tops[1] - 2);
    expect(gap?.page).toBe(1);
    expect(gap?.point.y).toBeLessThan(0);
  });
});

describe("thumbnailGrid", () => {
  const sizes = [A4, A4, A4, LANDSCAPE, A4];

  it("fills the pane with one column", () => {
    const g = thumbnailGrid(sizes, 0, 200, 1);
    expect(g.rows).toHaveLength(5);
    expect(g.widths[0]).toBe(200 - 2 * THUMB_GAP);
    expect(g.lefts[0]).toBe(THUMB_GAP);
  });

  it("puts several pages in a row and lines them up at the bottom", () => {
    const g = thumbnailGrid(sizes, 0, 400, 3);
    expect(g.rows.map((r) => [r.first, r.last])).toEqual([
      [0, 2],
      [3, 4],
    ]);
    // The landscape page is shorter: it sits lower, on the same bottom line.
    expect(g.tops[3] + g.heights[3]).toBe(g.tops[4] + g.heights[4]);
    expect(g.lefts[1]).toBeGreaterThan(g.lefts[0] + g.widths[0]);
    expect(g.rows[0].height).toBe(g.heights[0] + THUMB_LABEL);
  });

  it("never fits more columns than the minimum width allows", () => {
    expect(maxThumbnailColumns(160)).toBe(1);
    expect(maxThumbnailColumns(480)).toBe(5);
    expect(thumbnailGrid(sizes, 0, 160, 4).rows[0].last).toBe(0);
  });

  it("finds visible thumbnails by row", () => {
    const g = thumbnailGrid(sizes, 0, 400, 3);
    expect(visibleRange(g, 0, 50)).toEqual([0, 2]);
  });
});

describe("sidebar width", () => {
  it("stays between the minimum and a quarter of the window", () => {
    expect(clampSidebar(100, 1600)).toBe(SIDEBAR_MIN);
    expect(clampSidebar(900, 1600)).toBe(400);
    // Small windows still allow 240 px.
    expect(clampSidebar(300, 800)).toBe(240);
  });
});
