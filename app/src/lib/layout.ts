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
  widths: number[];
  heights: number[];
  totalHeight: number;
  /** Width of the widest page plus margins. */
  totalWidth: number;
}

/** Page size after view rotation (90° and 270° swap width and height). */
export function rotatedSize(size: PageSize, rotation: Degrees): PageSize {
  return rotation % 180 === 0 ? size : { width: size.height, height: size.width };
}

export function computeLayout(sizes: PageSize[], zoom: number, rotation: Degrees): Layout {
  const tops: number[] = [];
  const widths: number[] = [];
  const heights: number[] = [];
  let y = PAGE_GAP;
  let maxWidth = 0;
  for (const size of sizes) {
    const r = rotatedSize(size, rotation);
    const w = r.width * zoom * PX_PER_PT;
    const h = r.height * zoom * PX_PER_PT;
    tops.push(y);
    widths.push(w);
    heights.push(h);
    y += h + PAGE_GAP;
    maxWidth = Math.max(maxWidth, w);
  }
  return { tops, widths, heights, totalHeight: y, totalWidth: maxWidth + 2 * PAGE_GAP };
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

/** First and last page (inclusive) that intersect the viewport. */
export function visibleRange(layout: Layout, scrollTop: number, viewportHeight: number): [number, number] {
  const count = layout.tops.length;
  if (count === 0) return [0, -1];
  let first = pageAt(layout, scrollTop);
  // The page at scrollTop may end above the viewport (we're in the gap below it).
  if (layout.tops[first] + layout.heights[first] < scrollTop && first < count - 1) first++;
  const last = pageAt(layout, scrollTop + viewportHeight);
  return [first, Math.max(first, last)];
}

/**
 * The "current" page: the one covering the middle of the viewport,
 * which matches what people mean by "the page I'm on".
 */
export function currentPage(layout: Layout, scrollTop: number, viewportHeight: number): number {
  return pageAt(layout, scrollTop + viewportHeight / 2);
}

/** Zoom that makes the widest page fill the viewport width. */
export function fitWidthZoom(sizes: PageSize[], rotation: Degrees, viewportWidth: number): number {
  const widest = Math.max(1, ...sizes.map((s) => rotatedSize(s, rotation).width));
  return clampZoom((viewportWidth - 2 * PAGE_GAP) / (widest * PX_PER_PT));
}

/** Zoom that fits a whole page (the current one) inside the viewport. */
export function fitPageZoom(size: PageSize, rotation: Degrees, viewportWidth: number, viewportHeight: number): number {
  const r = rotatedSize(size, rotation);
  const byWidth = (viewportWidth - 2 * PAGE_GAP) / (r.width * PX_PER_PT);
  const byHeight = (viewportHeight - 2 * PAGE_GAP) / (r.height * PX_PER_PT);
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
