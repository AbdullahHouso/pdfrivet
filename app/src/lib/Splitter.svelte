<script lang="ts">
// The edge of a side panel, dragged to resize it. Sits on the panel's inner
// edge: its end for the sidebar, its start for the comments panel (so it
// mirrors in Arabic). Arrow keys resize too; double-click goes back to the
// default width. The width is only saved when the drag ends.

interface Props {
  /** The panel's width in px, updated live while dragging. */
  value: number;
  min: number;
  max: number;
  defaultValue: number;
  /** Which edge of the panel this is (logical, so it mirrors in RTL). */
  edge: "start" | "end";
  label: string;
  /** Called with the final width when a drag or key press ends. */
  oncommit: (width: number) => void;
}
let { value = $bindable(), min, max, defaultValue, edge, label, oncommit }: Props = $props();

const STEP = 16;
let dragging = $state(false);
let startX = 0;
let startWidth = 0;
// +1 when moving the pointer right makes the panel wider.
let direction = 1;

function clamp(width: number): number {
  return Math.round(Math.min(Math.max(width, min), max));
}

function onpointerdown(e: PointerEvent) {
  if (e.button !== 0) return;
  e.preventDefault();
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  const rtl = getComputedStyle(e.currentTarget as HTMLElement).direction === "rtl";
  direction = (edge === "end" ? 1 : -1) * (rtl ? -1 : 1);
  startX = e.clientX;
  startWidth = value;
  dragging = true;
}

function onpointermove(e: PointerEvent) {
  if (!dragging) return;
  value = clamp(startWidth + (e.clientX - startX) * direction);
}

function onpointerup() {
  if (!dragging) return;
  dragging = false;
  oncommit(value);
}

function onkeydown(e: KeyboardEvent) {
  const rtl = getComputedStyle(e.currentTarget as HTMLElement).direction === "rtl";
  // The arrow pointing away from the panel makes it wider.
  const outward = (edge === "end") !== rtl ? "ArrowRight" : "ArrowLeft";
  const inward = outward === "ArrowRight" ? "ArrowLeft" : "ArrowRight";
  const next = {
    [outward]: value + STEP,
    [inward]: value - STEP,
    Home: min,
    End: max,
  }[e.key];
  if (next === undefined) return;
  e.preventDefault();
  value = clamp(next);
  oncommit(value);
}
</script>

<!-- A window splitter: focusable and adjustable with the keyboard. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="splitter {edge}"
  class:dragging
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={value}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  onpointercancel={onpointerup}
  ondblclick={() => {
    value = clamp(defaultValue);
    oncommit(value);
  }}
  {onkeydown}
></div>

{#if dragging}
  <!-- Keeps the resize cursor everywhere and stops text getting selected while dragging. -->
  <div class="drag-cover" aria-hidden="true"></div>
{/if}

<style>
  .splitter {
    position: absolute;
    inset-block: 0;
    width: 7px;
    z-index: 5;
    cursor: col-resize;
    touch-action: none;
  }
  .splitter.end {
    inset-inline-end: -4px;
  }
  .splitter.start {
    inset-inline-start: -4px;
  }
  /* A thin line shows where to grab, on hover and while dragging. */
  .splitter::after {
    content: "";
    position: absolute;
    inset-block: 0;
    inset-inline-start: 2px;
    width: 3px;
    background: var(--accent);
    opacity: 0;
    transition: opacity var(--motion-hover) var(--ease-out);
  }
  .splitter:hover::after,
  .splitter:focus-visible::after,
  .splitter.dragging::after {
    opacity: 1;
  }
  .splitter:focus-visible {
    outline: none;
  }
  .drag-cover {
    position: fixed;
    inset: 0;
    z-index: 4;
    cursor: col-resize;
    user-select: none;
  }
</style>
