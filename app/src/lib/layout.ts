// Page layout maths for the continuous viewer. Pure functions, unit-tested.
//
// Units: PDF points (pt) for page sizes, CSS pixels (px) for the screen.
// 100% zoom shows a page at its physical size on a 96 DPI screen.

import type { PageSize } from "./bindings/PageSize";

export const PX_PER_PT = 96 / 72;
/** Space between pages and around the document, in CSS px. */
export const PAGE_GAP = 16;

export type Degrees = 0 | 90 | 180 | 270;

export interface Layout {
  /** Top of each page, in px from the top of the scrollable content. */
  tops: number[];
  /** Left edge of each page, in px from the left of the content (width `totalWidth`). */
  lefts: number[];
  widths: number[];
  heights: number[];
  /** Which row each page is in (a row holds one page, or two side by side). */
  rowOf: number[];
  rows: Row[];
  totalHeight: number;
  /** Width of the widest row plus margins. */
  totalWidth: number;
}

export interface Row {
  /** First and last page index in the row (inclusive). */
  first: number;
  last: number;
  top: number;
  height: number;
}

export interface LayoutOptions {
  /** Pages per row: 1 (single page) or 2 (two pages side by side). */
  columns?: 1 | 2;
  /** Right-to-left reading: in two-page rows the first page goes on the right. */
  rtl?: boolean;
}

/** Page size after view rotation (90° and 270° swap width and height). */
export function rotatedSize(size: PageSize, rotation: Degrees): PageSize {
  return rotation % 180 === 0 ? size : { width: size.height, height: size.width };
}

export function computeLayout(
  sizes: PageSize[],
  zoom: number,
  rotation: Degrees,
  { columns = 1, rtl = false }: LayoutOptions = {},
): Layout {
  const widths = sizes.map((s) => rotatedSize(s, rotation).width * zoom * PX_PER_PT);
  const heights = sizes.map((s) => rotatedSize(s, rotation).height * zoom * PX_PER_PT);
  const tops: number[] = [];
  const lefts: number[] = [];
  const rowOf: number[] = [];
  const rows: Row[] = [];
  const rowWidths: number[] = [];

  let y = PAGE_GAP;
  for (let first = 0; first < sizes.length; first += columns) {
    const last = Math.min(first + columns, sizes.length) - 1;
    let height = 0;
    let width = -PAGE_GAP;
    for (let i = first; i <= last; i++) {
      height = Math.max(height, heights[i]);
      width += widths[i] + PAGE_GAP;
    }
    for (let i = first; i <= last; i++) {
      // Pages in a row are centred vertically on each other.
      tops[i] = y + (height - heights[i]) / 2;
      rowOf[i] = rows.length;
    }
    rows.push({ first, last, top: y, height });
    rowWidths.push(width);
    y += height + PAGE_GAP;
  }

  const totalWidth = Math.max(0, ...rowWidths) + 2 * PAGE_GAP;
  rows.forEach((row, r) => {
    let x = (totalWidth - rowWidths[r]) / 2;
    const order = [];
    for (let i = row.first; i <= row.last; i++) order.push(i);
    if (rtl) order.reverse();
    for (const i of order) {
      lefts[i] = x;
      x += widths[i] + PAGE_GAP;
    }
  });

  return { tops, lefts, widths, heights, rowOf, rows, totalHeight: y, totalWidth };
}

/** Index of the last page whose top is at or above `y` (binary search). */
export function pageAt(layout: Layout, y: number): number {
  const { tops } = layout;
  let lo = 0;
  let hi = tops.length - 1;
  if (hi < 0) return 0;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (tops[mid] <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** The row at height `y` (binary search over rows). */
export function rowAt(layout: Layout, y: number): number {
  const { rows } = layout;
  let lo = 0;
  let hi = rows.length - 1;
  if (hi < 0) return 0;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (rows[mid].top <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** First and last page (inclusive) that intersect the viewport. */
export function visibleRange(layout: Layout, scrollTop: number, viewportHeight: number): [number, number] {
  if (layout.rows.length === 0) return [0, -1];
  let first = rowAt(layout, scrollTop);
  // The row at scrollTop may end above the viewport (we're in the gap below it).
  const row = layout.rows[first];
  if (row.top + row.height < scrollTop && first < layout.rows.length - 1) first++;
  const last = Math.max(first, rowAt(layout, scrollTop + viewportHeight));
  return [layout.rows[first].first, layout.rows[last].last];
}

/**
 * The "current" page: the first page of the row covering the middle of the
 * viewport, which matches what people mean by "the page I'm on".
 */
export function currentPage(layout: Layout, scrollTop: number, viewportHeight: number): number {
  return layout.rows[rowAt(layout, scrollTop + viewportHeight / 2)]?.first ?? 0;
}

/** Widths (at zoom 1, in px) and page counts of each row. */
function rowWidthsAtZoom1(sizes: PageSize[], rotation: Degrees, columns: 1 | 2) {
  const result: { width: number; count: number; height: number }[] = [];
  for (let first = 0; first < sizes.length; first += columns) {
    const row = sizes.slice(first, first + columns).map((s) => rotatedSize(s, rotation));
    result.push({
      width: row.reduce((sum, s) => sum + s.width * PX_PER_PT, 0),
      height: Math.max(...row.map((s) => s.height * PX_PER_PT)),
      count: row.length,
    });
  }
  return result;
}

/** Zoom that makes the widest row fill the viewport width. */
export function fitWidthZoom(sizes: PageSize[], rotation: Degrees, viewportWidth: number, columns: 1 | 2 = 1): number {
  const rows = rowWidthsAtZoom1(sizes, rotation, columns);
  if (rows.length === 0) return 1;
  const zooms = rows.map((r) => (viewportWidth - 2 * PAGE_GAP - (r.count - 1) * PAGE_GAP) / Math.max(1, r.width));
  return clampZoom(Math.min(...zooms));
}

/** Zoom that fits the row containing `page` (one page, or a two-page spread) inside the viewport. */
export function fitPageZoom(
  sizes: PageSize[],
  page: number,
  rotation: Degrees,
  viewportWidth: number,
  viewportHeight: number,
  columns: 1 | 2 = 1,
): number {
  const rows = rowWidthsAtZoom1(sizes, rotation, columns);
  const row = rows[Math.floor(page / columns)] ?? rows[0];
  if (!row) return 1;
  const byWidth = (viewportWidth - 2 * PAGE_GAP - (row.count - 1) * PAGE_GAP) / Math.max(1, row.width);
  const byHeight = (viewportHeight - 2 * PAGE_GAP) / Math.max(1, row.height);
  return clampZoom(Math.min(byWidth, byHeight));
}

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 8;
export const ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4, 5, 6, 8];

export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Number.isFinite(zoom) ? zoom : 1));
}

/** Next zoom step up (+1) or down (-1) from the current zoom. */
export function stepZoom(zoom: number, direction: 1 | -1): number {
  if (direction > 0) return ZOOM_STEPS.find((z) => z > zoom + 1e-3) ?? MAX_ZOOM;
  return [...ZOOM_STEPS].reverse().find((z) => z < zoom - 1e-3) ?? MIN_ZOOM;
}

/** A point in the document that should stay under the cursor while zooming. */
export interface Anchor {
  page: number;
  /** Position inside the page, 0..1 from its top. */
  fraction: number;
  /** Distance of the anchor from the top of the viewport, in px. */
  offset: number;
}

export function anchorAt(layout: Layout, scrollTop: number, offsetInViewport: number): Anchor {
  const y = scrollTop + offsetInViewport;
  const page = pageAt(layout, y);
  const h = layout.heights[page] || 1;
  const fraction = Math.min(1, Math.max(0, (y - layout.tops[page]) / h));
  // If the point is in the gap above the page (e.g. the very top of the
  // document), remember that gap so it is kept too.
  const gapAbove = Math.max(0, layout.tops[page] - y);
  return { page, fraction, offset: offsetInViewport + gapAbove };
}

/** The scrollTop that puts `anchor` back at the same place in the viewport. */
export function scrollTopFor(layout: Layout, anchor: Anchor): number {
  const top = layout.tops[anchor.page] ?? 0;
  const h = layout.heights[anchor.page] ?? 0;
  return Math.max(0, top + anchor.fraction * h - anchor.offset);
}

/** A rectangle as fractions (0..1) of a page, measured from its top-left corner. */
export interface FractionRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Where a rectangle on an upright page ends up after rotating the view clockwise. */
export function rotateRect(r: FractionRect, rotation: Degrees): FractionRect {
  switch (rotation) {
    case 90:
      return { left: 1 - r.bottom, top: r.left, right: 1 - r.top, bottom: r.right };
    case 180:
      return { left: 1 - r.right, top: 1 - r.bottom, right: 1 - r.left, bottom: 1 - r.top };
    case 270:
      return { left: r.top, top: 1 - r.right, right: r.bottom, bottom: 1 - r.left };
    default:
      return r;
  }
}
