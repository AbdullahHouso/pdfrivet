<script lang="ts">
// The page viewer for one tab: continuous scrolling or one page (or spread)
// at a time, with one or two pages per row. Only pages near the viewport are
// mounted; everything else is just empty space.
import { tick, untrack } from "svelte";
import {
  type Anchor,
  anchorAt,
  clampZoom,
  computeLayout,
  currentPage,
  type Degrees,
  fitPageZoom,
  fitWidthZoom,
  type Layout,
  PAGE_GAP,
  rotatedSize,
  scrollTopFor,
  visibleRange,
} from "./layout";
import PageView from "./PageView.svelte";
import { type RivetError, setVisiblePages } from "./pdf";
import { settings } from "./settings.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  onerror?: (e: RivetError) => void;
}
let { tab, onerror }: Props = $props();

let scroller: HTMLElement;
let viewportWidth = $state(0);
let viewportHeight = $state(0);
let scrollTop = $state(0);

let columns = $derived<1 | 2>(tab.pageLayout === "double" ? 2 : 1);
// Spreads follow the document's reading direction (detected, or set in Page display).
let rtl = $derived(tab.pagesRtl);

/** Layout of the whole document, or (page at a time) of just the current row. */
function makeView(zoom: number, rotation: Degrees): Layout {
  const full = computeLayout(tab.info.pageSizes, zoom, rotation, { columns, rtl });
  if (tab.continuous) return full;
  // Page at a time: shift everything so the current row is at the top,
  // or centred vertically when it's shorter than the window.
  const row = full.rows[full.rowOf[tab.page] ?? 0];
  const height = row.height + 2 * PAGE_GAP;
  const centring = Math.max(0, (viewportHeight - height) / 2);
  const offset = row.top - PAGE_GAP - centring;
  return {
    ...full,
    tops: full.tops.map((t) => t - offset),
    rows: [{ ...row, top: PAGE_GAP + centring }],
    totalHeight: height + 2 * centring,
  };
}

let layout = $derived(makeView(tab.zoom, tab.rotation));
let fullLayout = $derived(computeLayout(tab.info.pageSizes, tab.zoom, tab.rotation, { columns, rtl }));
let range = $derived(visibleRange(layout, scrollTop, viewportHeight));
// Mount one extra row on each side so scrolling never shows empty frames.
let mounted = $derived.by(() => {
  const [first, last] = range;
  if (last < first) return [];
  const { rows, rowOf } = fullLayout;
  const extra = tab.continuous ? 1 : 0;
  const from = rows[Math.max(0, rowOf[first] - extra)].first;
  const to = rows[Math.min(rows.length - 1, rowOf[last] + extra)].last;
  const pages: number[] = [];
  for (let i = from; i <= to; i++) pages.push(i);
  return pages;
});
let contentWidth = $derived(Math.max(layout.totalWidth, viewportWidth));
let offsetX = $derived((contentWidth - layout.totalWidth) / 2);

// Where to keep the view steady when the zoom changes (cursor or centre).
let pendingAnchor: Anchor | null = null;
// (The viewer is re-created per tab, so reading the starting values once is intended.)
let lastZoom = untrack(() => tab.zoom);
let lastRotation = untrack(() => tab.rotation);
let restored = false;

/** Shows `page` (0-based): scrolls to its row, or (page at a time) switches to it. */
export function goToPage(page: number, atBottom = false) {
  const p = Math.max(0, Math.min(tab.info.pageCount - 1, page));
  const { rows, rowOf } = fullLayout;
  const row = rows[rowOf[p]];
  tab.page = row.first;
  if (tab.continuous) {
    scroller.scrollTop = row.top - PAGE_GAP;
  } else {
    tick().then(() => {
      scroller.scrollTop = atBottom ? scroller.scrollHeight : 0;
    });
  }
}

/** Moves one row (page or spread) forward (+1) or back (-1). */
export function step(direction: 1 | -1, atBottom = false) {
  const { rows, rowOf } = fullLayout;
  const target = rows[rowOf[tab.page] + direction];
  if (target) goToPage(target.first, atBottom);
}

/** Zooms around a point of the viewport (default: its centre). */
export function zoomTo(zoom: number, offsetY = viewportHeight / 2) {
  pendingAnchor = anchorAt(layout, scroller.scrollTop, offsetY);
  tab.zoomMode = "custom";
  tab.zoom = clampZoom(zoom);
}

// Fit modes follow the window size, the page layout and (for fit page) the current row.
$effect(() => {
  if (viewportWidth === 0) return;
  const sizes = tab.info.pageSizes;
  if (tab.zoomMode === "fit-width") tab.zoom = fitWidthZoom(sizes, tab.rotation, viewportWidth, columns);
  // Fit the page you're on when the mode or window changes, not on every scroll.
  const page = untrack(() => tab.page);
  if (tab.zoomMode === "fit-page")
    tab.zoom = fitPageZoom(sizes, page, tab.rotation, viewportWidth, viewportHeight, columns);
});

// When the page layout or scrolling mode changes, keep showing the same page.
let lastMode = untrack(() => `${tab.pageLayout}/${tab.continuous}`);
$effect(() => {
  const mode = `${tab.pageLayout}/${tab.continuous}`;
  if (mode === lastMode) return;
  lastMode = mode;
  const page = untrack(() => tab.page);
  tick().then(() => goToPage(page));
});

// After a zoom or rotation, put the anchor point back where it was.
// Toolbar zoom and Ctrl+wheel pass their own anchor (centre or cursor);
// everything else (rotation, fit modes) keeps the top of the view steady.
// If several changes arrive before the DOM updates (rotating also changes the
// fit-width zoom), they share the first anchor instead of reading a stale scroll.
let queuedAnchor: Anchor | null = null;
$effect.pre(() => {
  const zoom = tab.zoom;
  const rotation = tab.rotation;
  if (!scroller || (zoom === lastZoom && rotation === lastRotation)) return;
  const before = untrack(() => makeView(lastZoom, lastRotation));
  const anchor = pendingAnchor ?? queuedAnchor ?? anchorAt(before, scroller.scrollTop, 0);
  pendingAnchor = null;
  queuedAnchor = anchor;
  lastZoom = zoom;
  lastRotation = rotation;
  tick().then(() => {
    if (queuedAnchor !== anchor) return;
    queuedAnchor = null;
    scroller.scrollTop = scrollTopFor(layout, anchor);
  });
});

// Tell the engine what is visible, so it skips pages we scrolled past.
$effect(() => {
  const [first, last] = range;
  if (last >= first) setVisiblePages(tab.docId, first, last).catch(() => {});
});

// Focus the pages so PageUp/PageDown, arrows and Space scroll right away.
$effect(() => {
  scroller.focus({ preventScroll: true });
});

// Restore the tab's scroll position (or its saved page) once laid out.
$effect(() => {
  if (restored || viewportHeight === 0) return;
  restored = true;
  if (tab.continuous && tab.scrollTop > 0) {
    scroller.scrollTop = tab.scrollTop;
    scroller.scrollLeft = tab.scrollLeft;
  } else if (tab.page > 0) {
    goToPage(tab.page);
  }
});

function onScroll() {
  scrollTop = scroller.scrollTop;
  tab.scrollTop = scroller.scrollTop;
  tab.scrollLeft = scroller.scrollLeft;
  if (tab.continuous) tab.page = currentPage(layout, scrollTop, viewportHeight);
}

// Page at a time: scrolling past the end of a page turns to the next one.
let lastTurn = 0;
function turnPage(direction: 1 | -1): boolean {
  const atTop = scroller.scrollTop <= 0;
  const atBottom = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 1;
  if ((direction > 0 && !atBottom) || (direction < 0 && !atTop)) return false;
  const now = Date.now();
  // One turn per wheel gesture, not one per wheel event.
  if (now - lastTurn < 350) return true;
  lastTurn = now;
  step(direction, direction < 0);
  return true;
}

function onWheel(e: WheelEvent) {
  // Ctrl + wheel (and touchpad pinch, which browsers report the same way) zooms.
  if (e.ctrlKey) {
    e.preventDefault();
    const rect = scroller.getBoundingClientRect();
    zoomTo(tab.zoom * Math.exp(-e.deltaY * 0.0025), e.clientY - rect.top);
    return;
  }
  if (!tab.continuous && e.deltaY !== 0 && turnPage(e.deltaY > 0 ? 1 : -1)) e.preventDefault();
}

function onKeyDown(e: KeyboardEvent) {
  if (tab.continuous || e.ctrlKey || e.metaKey || e.altKey) return;
  // Left/right turn pages like a book (mirrored for right-to-left languages).
  const forward = rtl ? "ArrowLeft" : "ArrowRight";
  const back = rtl ? "ArrowRight" : "ArrowLeft";
  if (e.key === forward) step(1);
  else if (e.key === back) step(-1);
  else if (e.key === "PageDown" || e.key === " " || e.key === "ArrowDown") {
    if (!turnPage(1)) return;
  } else if (e.key === "PageUp" || e.key === "ArrowUp") {
    if (!turnPage(-1)) return;
  } else return;
  e.preventDefault();
}

// Dragging pans the view: always with the middle button (like Figma), and with
// the left button in Hand mode. In Hand mode a drag only starts after the
// pointer moves a few pixels, so a simple click still follows links and fields.
const DRAG_THRESHOLD = 4;
let pan: { x: number; y: number; left: number; top: number; id: number; active: boolean } | null = $state(null);

function onPointerDown(e: PointerEvent) {
  const middle = e.button === 1;
  const handDrag = e.button === 0 && settings.tool === "hand";
  if (!middle && !handDrag) return;
  if (middle) e.preventDefault();
  pan = {
    x: e.clientX,
    y: e.clientY,
    left: scroller.scrollLeft,
    top: scroller.scrollTop,
    id: e.pointerId,
    active: middle,
  };
  if (middle) scroller.setPointerCapture(e.pointerId);
}

function onPointerMove(e: PointerEvent) {
  if (!pan || e.pointerId !== pan.id) return;
  const dx = e.clientX - pan.x;
  const dy = e.clientY - pan.y;
  if (!pan.active) {
    if (Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
    pan.active = true;
    scroller.setPointerCapture(e.pointerId);
  }
  scroller.scrollLeft = pan.left - dx;
  scroller.scrollTop = pan.top - dy;
}

function onPointerUp(e: PointerEvent) {
  if (!pan || e.pointerId !== pan.id) return;
  if (scroller.hasPointerCapture(e.pointerId)) scroller.releasePointerCapture(e.pointerId);
  pan = null;
}

function onFieldChange() {
  tab.dirty = true;
  tab.revision++;
}
</script>

<!-- dir="ltr": page geometry is computed in code (including right-to-left
     spreads), and this avoids browsers' different RTL scroll coordinates.
     The scroll area handles page-turning keys itself, like a native document view. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="scroller"
  dir="ltr"
  role="document"
  tabindex="-1"
  bind:this={scroller}
  bind:clientWidth={viewportWidth}
  bind:clientHeight={viewportHeight}
  onscroll={onScroll}
  onwheel={onWheel}
  onkeydown={onKeyDown}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onmousedown={(e) => e.button === 1 && e.preventDefault()}
  class:panning={pan?.active}
  class:hand={settings.tool === "hand"}
  data-tone={settings.pageTone}
>
  <div class="content" style:height="{layout.totalHeight}px" style:width="{contentWidth}px">
    {#each mounted as index (index)}
      <div class="slot" style:top="{layout.tops[index]}px" style:left="{offsetX + layout.lefts[index]}px">
        <PageView
          docId={tab.docId}
          {index}
          width={layout.widths[index]}
          height={layout.heights[index]}
          widthPt={rotatedSize(tab.info.pageSizes[index], tab.rotation).width}
          rotation={tab.rotation}
          {onerror}
          ongotopage={goToPage}
          revision={tab.revision}
          onfieldchange={onFieldChange}
        />
      </div>
    {/each}
  </div>
</div>

<style>
  .scroller {
    flex: 1;
    min-width: 0;
    overflow: auto;
    background: var(--canvas);
    outline: none;
    overscroll-behavior: contain;
  }
  .scroller.hand {
    cursor: grab;
  }
  /* In Hand mode, page text and canvases shouldn't get selected or dragged. */
  .scroller.hand :global(.page) {
    user-select: none;
    -webkit-user-drag: none;
  }
  .scroller.panning {
    cursor: grabbing;
    user-select: none;
  }
  .content {
    position: relative;
  }
  /* Page colours (see PageTone): only how pages look on screen; files and
     printing are unchanged. A colour multiplied over the page tints white
     paper and leaves dark text dark. */
  .scroller:is([data-tone="warm"], [data-tone="green"], [data-tone="dimmed"]) :global(.page)::after {
    content: "";
    position: absolute;
    inset: 0;
    mix-blend-mode: multiply;
    pointer-events: none;
  }
  .scroller[data-tone="warm"] :global(.page)::after {
    background: #f3e3bd;
  }
  .scroller[data-tone="green"] :global(.page)::after {
    background: #cce8cf;
  }
  .scroller[data-tone="dimmed"] :global(.page)::after {
    background: #b9b9b9;
  }
  /* Dark pages: light text on dark paper. Turning the hue back keeps
     colours (red stays red), and lowering the brightness softens the white. */
  .scroller[data-tone="dark"] :global(.page) {
    background: #232428;
  }
  .scroller[data-tone="dark"] :global(.page canvas) {
    filter: invert(1) hue-rotate(180deg) brightness(0.9);
  }
  .slot {
    position: absolute;
  }
</style>
