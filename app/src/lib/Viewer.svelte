<script lang="ts">
import { tick, untrack } from "svelte";
// The continuous, virtualized page viewer for one tab.
// Only pages near the viewport are mounted; everything else is just space.
import {
  type Anchor,
  anchorAt,
  clampZoom,
  computeLayout,
  currentPage,
  fitPageZoom,
  fitWidthZoom,
  PAGE_GAP,
  rotatedSize,
  scrollTopFor,
  visibleRange,
} from "./layout";
import PageView from "./PageView.svelte";
import { type RivetError, setVisiblePages } from "./pdf";
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

let layout = $derived(computeLayout(tab.info.pageSizes, tab.zoom, tab.rotation));
let range = $derived(visibleRange(layout, scrollTop, viewportHeight));
// Mount one extra page on each side so scrolling never shows empty frames.
let mounted = $derived.by(() => {
  const [first, last] = range;
  const pages: number[] = [];
  for (let i = Math.max(0, first - 1); i <= Math.min(tab.info.pageCount - 1, last + 1); i++) pages.push(i);
  return pages;
});

// Where to keep the view steady when the zoom changes (cursor or centre).
let pendingAnchor: Anchor | null = null;
// (The viewer is re-created per tab, so reading the starting values once is intended.)
let lastZoom = untrack(() => tab.zoom);
let lastRotation = untrack(() => tab.rotation);
let restored = false;

/** Scrolls so that `page` (0-based) starts at the top. */
export function goToPage(page: number) {
  const p = Math.max(0, Math.min(tab.info.pageCount - 1, page));
  scroller.scrollTop = layout.tops[p] - PAGE_GAP;
  tab.page = p;
}

/** Zooms around a point of the viewport (default: its centre). */
export function zoomTo(zoom: number, offsetY = viewportHeight / 2) {
  pendingAnchor = anchorAt(layout, scroller.scrollTop, offsetY);
  tab.zoomMode = "custom";
  tab.zoom = clampZoom(zoom);
}

// Fit modes follow the window size and the current page.
$effect(() => {
  if (viewportWidth === 0) return;
  const sizes = tab.info.pageSizes;
  if (tab.zoomMode === "fit-width") tab.zoom = fitWidthZoom(sizes, tab.rotation, viewportWidth);
  // Fit the page you're on when the mode or window changes, not on every scroll.
  const page = untrack(() => tab.page);
  if (tab.zoomMode === "fit-page")
    tab.zoom = fitPageZoom(sizes[page] ?? sizes[0], tab.rotation, viewportWidth, viewportHeight);
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
  const before = computeLayout(tab.info.pageSizes, lastZoom, lastRotation);
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
  if (tab.scrollTop > 0) {
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
  tab.page = currentPage(layout, scrollTop, viewportHeight);
}

function onWheel(e: WheelEvent) {
  // Ctrl + wheel (and touchpad pinch, which browsers report the same way) zooms.
  if (!e.ctrlKey) return;
  e.preventDefault();
  const rect = scroller.getBoundingClientRect();
  zoomTo(tab.zoom * Math.exp(-e.deltaY * 0.0025), e.clientY - rect.top);
}
</script>

<!-- dir="ltr": page geometry doesn't depend on the UI language, and this
     avoids browsers' different RTL scroll coordinates. -->
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
>
  <div class="content" style:height="{layout.totalHeight}px" style:width="{Math.max(layout.totalWidth, viewportWidth)}px">
    {#each mounted as index (index)}
      {@const width = layout.widths[index]}
      <div
        class="slot"
        style:top="{layout.tops[index]}px"
        style:left="{(Math.max(layout.totalWidth, viewportWidth) - width) / 2}px"
      >
        <PageView
          docId={tab.docId}
          {index}
          {width}
          height={layout.heights[index]}
          widthPt={rotatedSize(tab.info.pageSizes[index], tab.rotation).width}
          rotation={tab.rotation}
          {onerror}
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
  .content {
    position: relative;
  }
  .slot {
    position: absolute;
  }
</style>
