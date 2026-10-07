<script lang="ts">
// Virtualized page thumbnails. Clicking one jumps to that page.
import { i18n } from "./i18n.svelte";
import { type Layout, rotatedSize, visibleRange } from "./layout";
import PageView from "./PageView.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  ongoto: (page: number) => void;
}
let { tab, ongoto }: Props = $props();

const THUMB_WIDTH = 112;
const LABEL = 24;
const GAP = 12;

let scroller: HTMLElement;
let scrollTop = $state(0);
let viewportHeight = $state(0);

// Same shape as the viewer's layout, so the binary search can be reused.
let layout = $derived.by((): Layout => {
  const tops: number[] = [];
  const widths: number[] = [];
  const heights: number[] = [];
  let y = GAP;
  for (const size of tab.info.pageSizes) {
    const r = rotatedSize(size, tab.rotation);
    const h = Math.round((THUMB_WIDTH * r.height) / r.width);
    tops.push(y);
    widths.push(THUMB_WIDTH);
    heights.push(h + LABEL);
    y += h + LABEL + GAP;
  }
  return { tops, widths, heights, totalHeight: y, totalWidth: THUMB_WIDTH };
});
let range = $derived(visibleRange(layout, scrollTop, viewportHeight));

// Keep the current page's thumbnail in view while reading.
$effect(() => {
  const page = tab.page;
  if (!scroller) return;
  const top = layout.tops[page];
  const bottom = top + layout.heights[page];
  if (top < scroller.scrollTop || bottom > scroller.scrollTop + viewportHeight) {
    scroller.scrollTo({ top: top - GAP });
  }
});
</script>

<div class="thumbs" bind:this={scroller} bind:clientHeight={viewportHeight} onscroll={() => (scrollTop = scroller.scrollTop)}>
  <div class="content" style:height="{layout.totalHeight}px">
    {#each { length: Math.max(0, range[1] - range[0] + 1) } as _, i (range[0] + i)}
      {@const index = range[0] + i}
      {@const size = rotatedSize(tab.info.pageSizes[index], tab.rotation)}
      <button
        class="thumb"
        class:current={index === tab.page}
        style:top="{layout.tops[index]}px"
        onclick={() => ongoto(index)}
        aria-label={i18n.t("go-to-page-n", { page: index + 1 })}
        aria-current={index === tab.page ? "page" : undefined}
      >
        <PageView
          docId={tab.docId}
          {index}
          width={THUMB_WIDTH}
          height={layout.heights[index] - LABEL}
          widthPt={size.width}
          rotation={tab.rotation}
          thumbnail
        />
        <span class="label">{(index + 1).toLocaleString(i18n.locale)}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .thumbs {
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .content {
    position: relative;
  }
  .thumb {
    position: absolute;
    inset-inline-start: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 0;
    border: none;
    background: none;
    border-radius: 4px;
  }
  :global([dir="rtl"]) .thumb {
    transform: translateX(50%);
  }
  .thumb :global(.page) {
    outline: 2px solid transparent;
    outline-offset: 2px;
  }
  .thumb.current :global(.page) {
    outline-color: var(--accent);
  }
  .thumb:hover:not(.current) :global(.page) {
    outline-color: var(--border);
  }
  .label {
    font-size: 12px;
    color: var(--muted);
  }
  .thumb.current .label {
    color: var(--text);
    font-weight: 600;
  }
</style>
