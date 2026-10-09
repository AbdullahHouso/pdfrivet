<script lang="ts">
// The page viewer for one tab: continuous scrolling or one page (or spread)
// at a time, with one or two pages per row. Only pages near the viewport are
// mounted; everything else is just empty space.
import { tick, untrack } from "svelte";
import {
  annotate,
  isMarkupTool,
  markArea,
  markForRedaction,
  markSelection,
  markSelectionForRedaction,
  worksOnText,
} from "./annotate.svelte";
import { topmostAt } from "./annotGeometry";
import type { SearchHit } from "./bindings/SearchHit";
import {
  type Anchor,
  anchorAt,
  clampZoom,
  computeLayout,
  currentPage,
  type Degrees,
  type FractionPoint,
  fitPageZoom,
  fitWidthZoom,
  type Layout,
  PAGE_GAP,
  pageAtPoint,
  rotatedSize,
  rotateRect,
  scrollTopFor,
  unrotatePoint,
  visibleRange,
} from "./layout";
import PageView from "./PageView.svelte";
import { loadPageText, peekPageText } from "./pageText";
import { type RivetError, setVisiblePages, toRivetError } from "./pdf";
import { settings } from "./settings.svelte";
import type { Tab } from "./tabs.svelte";
import {
  caretAt,
  charAt,
  isEmpty,
  isOverText,
  lineAt,
  rangeOnPage,
  selectionRects,
  type TextPosition,
  wordAt,
} from "./textSelect";

interface Props {
  tab: Tab;
  onerror?: (e: RivetError) => void;
  /** Right-click on the pages (for the selection's menu). */
  oncontextmenu?: (e: MouseEvent) => void;
}
let { tab, onerror, oncontextmenu }: Props = $props();

let scroller: HTMLElement;
let content: HTMLElement;
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

/** Scrolls so a search result is in view (leaves the view alone if it already is). */
async function revealHit(hit: SearchHit) {
  const text = await loadPageText(tab.docId, hit.page).catch(() => null);
  const rects = text ? selectionRects(text, hit.start, hit.end) : [];
  if (!tab.continuous && fullLayout.rowOf[hit.page] !== fullLayout.rowOf[tab.page]) {
    goToPage(hit.page);
    await tick();
  }
  if (rects.length === 0) {
    goToPage(hit.page);
    return;
  }
  const r = rotateRect(rects[0], tab.rotation);
  const top = layout.tops[hit.page] + r.top * layout.heights[hit.page];
  const bottom = layout.tops[hit.page] + r.bottom * layout.heights[hit.page];
  if (top < scroller.scrollTop || bottom > scroller.scrollTop + viewportHeight) {
    scroller.scrollTop = top - viewportHeight / 3;
  }
  const left = offsetX + layout.lefts[hit.page] + r.left * layout.widths[hit.page];
  const right = offsetX + layout.lefts[hit.page] + r.right * layout.widths[hit.page];
  if (left < scroller.scrollLeft || right > scroller.scrollLeft + viewportWidth) {
    scroller.scrollLeft = left - viewportWidth / 3;
  }
}

// Show the current search result whenever it changes (not when more results arrive).
let revealed = "";
$effect(() => {
  const hit = tab.search.hits[tab.search.current];
  const key = hit ? `${hit.page}:${hit.start}` : "";
  if (!hit || key === revealed) return;
  revealed = key;
  untrack(() => revealHit(hit));
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

// Selecting text (Select mode): drag to select, double-click for a word,
// triple-click for a line, Shift+click to extend. The page's text was loaded
// when the page appeared (TextLayer), so matching the pointer is instant.
let selectingPointer: number | null = null;
let lastClick = { time: 0, x: 0, y: 0, count: 0 };
let overText = $state(false);

/** The page under the pointer and the point on it (upright page fractions). */
function pointOnPage(e: MouseEvent) {
  const box = content.getBoundingClientRect();
  const hit = pageAtPoint(layout, mounted, e.clientX - box.left, e.clientY - box.top, offsetX);
  if (!hit) return null;
  return { page: hit.page, point: unrotatePoint(hit.point, tab.rotation) };
}

function positionAt(e: MouseEvent): TextPosition | null {
  const hit = pointOnPage(e);
  const text = hit && peekPageText(tab.docId, hit.page);
  if (!hit || !text) return null;
  return { page: hit.page, index: caretAt(text, hit.point.x, hit.point.y) };
}

/** Links and form fields keep their own clicks. */
function isControl(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest("button, input, select, textarea, a");
}

function startSelection(e: PointerEvent) {
  const now = performance.now();
  const repeat = now - lastClick.time < 450 && Math.hypot(e.clientX - lastClick.x, e.clientY - lastClick.y) < 6;
  const clicks = repeat ? (lastClick.count % 3) + 1 : 1;
  lastClick = { time: now, x: e.clientX, y: e.clientY, count: clicks };

  const hit = pointOnPage(e);
  const text = hit && peekPageText(tab.docId, hit.page);
  tab.selectedAnnotation = null;
  clickStart = { x: e.clientX, y: e.clientY };
  // Highlight, underline, strike out or redact where there's no text (pictures,
  // scans): drag out an area instead of selecting text.
  const tool = annotate.open ? annotate.tool : null;
  if (worksOnText(tool) && hit && (!text || !isOverText(text, hit.point.x, hit.point.y))) {
    startArea(e, hit.page);
    return;
  }
  if (!hit || !text) {
    tab.selection = null;
    if (hit) selectAnnotationAt(e);
    return;
  }
  // Keep the browser from starting its own selection or dragging the canvas.
  e.preventDefault();
  scroller.focus({ preventScroll: true });
  if (clicks > 1) {
    let i = charAt(text, hit.point.x, hit.point.y);
    if (i < 0) i = Math.max(0, caretAt(text, hit.point.x, hit.point.y) - 1);
    const [start, end] = clicks === 2 ? wordAt(text, i) : lineAt(text, i);
    tab.selection = { anchor: { page: hit.page, index: start }, focus: { page: hit.page, index: end } };
    return;
  }
  const position = { page: hit.page, index: caretAt(text, hit.point.x, hit.point.y) };
  tab.selection =
    e.shiftKey && tab.selection
      ? { anchor: tab.selection.anchor, focus: position }
      : { anchor: position, focus: position };
  selectingPointer = e.pointerId;
  scroller.setPointerCapture(e.pointerId);
}

function extendSelection(e: PointerEvent) {
  // Dragging past the top or bottom edge scrolls, so long selections are possible.
  const box = scroller.getBoundingClientRect();
  if (e.clientY < box.top + 24) scroller.scrollTop -= 24;
  else if (e.clientY > box.bottom - 24) scroller.scrollTop += 24;
  const position = positionAt(e);
  if (position && tab.selection) tab.selection = { anchor: tab.selection.anchor, focus: position };
}

function endSelection(e: PointerEvent) {
  if (scroller.hasPointerCapture(e.pointerId)) scroller.releasePointerCapture(e.pointerId);
  selectingPointer = null;
  if (isEmpty(tab.selection)) {
    tab.selection = null;
    // A click (not a drag) on a highlight, a stamp… selects it.
    if (Math.hypot(e.clientX - clickStart.x, e.clientY - clickStart.y) < 4) selectAnnotationAt(e);
    return;
  }
  // With a text markup tool, selecting text marks it right away; with Redact, marks it for redaction.
  const tool = annotate.open ? annotate.tool : null;
  if (isMarkupTool(tool)) markSelection(tab, tool).catch((err) => onerror?.(toRivetError(err)));
  else if (tool === "redact") markSelectionForRedaction(tab).catch((err) => onerror?.(toRivetError(err)));
}

// Dragging out an area (see startSelection), in fractions of the page as shown.
let area = $state<{ pointerId: number; page: number; from: FractionPoint; to: FractionPoint } | null>(null);

/** Where the pointer is on `page` as shown (fractions, kept inside the page). */
function shownPoint(e: MouseEvent, page: number): FractionPoint {
  const box = content.getBoundingClientRect();
  const x = (e.clientX - box.left - offsetX - layout.lefts[page]) / layout.widths[page];
  const y = (e.clientY - box.top - layout.tops[page]) / layout.heights[page];
  return { x: Math.min(1, Math.max(0, x)), y: Math.min(1, Math.max(0, y)) };
}

function startArea(e: PointerEvent, page: number) {
  e.preventDefault();
  scroller.focus({ preventScroll: true });
  tab.selection = null;
  const p = shownPoint(e, page);
  area = { pointerId: e.pointerId, page, from: p, to: p };
  scroller.setPointerCapture(e.pointerId);
}

function endArea(e: PointerEvent) {
  if (!area || area.pointerId !== e.pointerId) return;
  if (scroller.hasPointerCapture(e.pointerId)) scroller.releasePointerCapture(e.pointerId);
  const { page, from, to } = area;
  area = null;
  const w = Math.abs(to.x - from.x) * layout.widths[page];
  const h = Math.abs(to.y - from.y) * layout.heights[page];
  if (w < 4 || h < 4) {
    selectAnnotationAt(e);
    return;
  }
  const a = unrotatePoint(from, tab.rotation);
  const b = unrotatePoint(to, tab.rotation);
  const rect = {
    left: Math.min(a.x, b.x),
    top: Math.min(a.y, b.y),
    right: Math.max(a.x, b.x),
    bottom: Math.max(a.y, b.y),
  };
  const tool = annotate.tool;
  if (tool === "redact") markForRedaction(tab, page, [rect]);
  else if (isMarkupTool(tool)) markArea(tab, page, tool, rect).catch((err) => onerror?.(toRivetError(err)));
}

let clickStart = { x: 0, y: 0 };

/** Selects the annotation under the pointer, if any (drawings and shapes select themselves). */
function selectAnnotationAt(e: MouseEvent) {
  const box = content.getBoundingClientRect();
  const hit = pageAtPoint(layout, mounted, e.clientX - box.left, e.clientY - box.top, offsetX);
  if (!hit || hit.point.x < 0 || hit.point.x > 1 || hit.point.y < 0 || hit.point.y > 1) return;
  const list = tab.annotations.get(hit.page) ?? [];
  const page = { width: layout.widths[hit.page], height: layout.heights[hit.page], rotation: tab.rotation };
  const found = topmostAt(list, hit.point.x * page.width, hit.point.y * page.height, page);
  if (found) tab.selectedAnnotation = { page: hit.page, id: found.id };
}

/** Shows the text cursor over text in Select mode. */
function updateCursor(e: PointerEvent) {
  if (settings.tool !== "select" || e.buttons !== 0 || isControl(e.target)) {
    overText = false;
    areaCursor = false;
    return;
  }
  const hit = pointOnPage(e);
  const text = hit && peekPageText(tab.docId, hit.page);
  overText = !!hit && !!text && isOverText(text, hit.point.x, hit.point.y);
  areaCursor = !overText && !!hit && worksOnText(annotate.open ? annotate.tool : null);
}

/** The pointer is where a markup or redact tool would drag out an area. */
let areaCursor = $state(false);

function onPointerDown(e: PointerEvent) {
  if (e.button === 0 && settings.tool === "select" && !isControl(e.target)) {
    startSelection(e);
    return;
  }
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
  if (area && area.pointerId === e.pointerId) {
    area = { ...area, to: shownPoint(e, area.page) };
    return;
  }
  if (selectingPointer === e.pointerId) {
    extendSelection(e);
    return;
  }
  updateCursor(e);
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
  if (area && area.pointerId === e.pointerId) {
    endArea(e);
    return;
  }
  if (selectingPointer === e.pointerId) {
    endSelection(e);
    return;
  }
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
  oncontextmenu={(e) => {
    if (isControl(e.target)) return;
    e.preventDefault();
    oncontextmenu?.(e);
  }}
  class:panning={pan?.active}
  class:text-cursor={overText}
  class:area-cursor={areaCursor}
  class:hand={settings.tool === "hand"}
  data-tone={tab.pageTone}
>
  <div class="content" bind:this={content} style:height="{layout.totalHeight}px" style:width="{contentWidth}px">
    {#if area}
      {@const left = offsetX + layout.lefts[area.page] + Math.min(area.from.x, area.to.x) * layout.widths[area.page]}
      {@const top = layout.tops[area.page] + Math.min(area.from.y, area.to.y) * layout.heights[area.page]}
      <div
        class="area"
        class:redact={annotate.tool === "redact"}
        style:left="{left}px"
        style:top="{top}px"
        style:width="{Math.abs(area.to.x - area.from.x) * layout.widths[area.page]}px"
        style:height="{Math.abs(area.to.y - area.from.y) * layout.heights[area.page]}px"
      ></div>
    {/if}
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
          revision={tab.pageRevision(index)}
          onfieldchange={onFieldChange}
          selected={rangeOnPage(tab.selection, index)}
          hits={tab.search.marksOn(index)}
          {tab}
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
  .scroller.text-cursor {
    cursor: text;
  }
  .scroller.area-cursor {
    cursor: crosshair;
  }
  /* Text is selected by our own code (see textSelect.ts), never by the browser. */
  .scroller :global(.page) {
    user-select: none;
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
  /* An area being dragged out with a markup or redact tool, above the pages. */
  .area {
    position: absolute;
    z-index: 3;
    border: 1.5px dashed var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    pointer-events: none;
  }
  .area.redact {
    border: 2px solid #d32f2f;
    background: rgb(211 47 47 / 0.14);
  }
</style>
