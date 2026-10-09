<script lang="ts">
// Virtualized page thumbnails in a grid. Clicking one jumps to that page.
// They fill the sidebar's width: a wider sidebar gives bigger thumbnails, and
// the size control above them fits more per row.
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { maxThumbnailColumns, rotatedSize, THUMB_GAP, thumbnailGrid, visibleRange } from "./layout";
import PageView from "./PageView.svelte";
import { settings } from "./settings.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  ongoto: (page: number) => void;
}
let { tab, ongoto }: Props = $props();

let scroller: HTMLElement;
let scrollTop = $state(0);
let viewportHeight = $state(0);
let paneWidth = $state(0);

let maxColumns = $derived(maxThumbnailColumns(paneWidth));
let columns = $derived(Math.min(settings.thumbnailColumns, maxColumns));
let layout = $derived(thumbnailGrid(tab.info.pageSizes, tab.rotation, paneWidth, columns));
let range = $derived(visibleRange(layout, scrollTop, viewportHeight));

/** Bigger thumbnails are fewer per row. */
function resize(bigger: boolean) {
  settings.thumbnailColumns = Math.min(Math.max(columns + (bigger ? -1 : 1), 1), maxColumns);
}

// Keep the current page's thumbnail in view while reading.
$effect(() => {
  const page = tab.page;
  if (!scroller || layout.rows.length === 0) return;
  const row = layout.rows[layout.rowOf[page]];
  if (!row) return;
  if (row.top < scroller.scrollTop || row.top + row.height > scroller.scrollTop + viewportHeight) {
    scroller.scrollTo({ top: row.top - THUMB_GAP });
  }
});
</script>

<div class="wrap">
  {#if maxColumns > 1 || columns > 1}
    <div class="size" role="group" aria-label={i18n.t("thumbnail-size")}>
      <button class="icon" onclick={() => resize(false)} disabled={columns >= maxColumns}
        aria-label={i18n.t("thumbnails-smaller")} title={i18n.t("thumbnails-smaller")}>
        <Icon name="minus" />
      </button>
      <!-- Right (in reading direction) is bigger: fewer thumbnails per row. -->
      <input type="range" min="1" max={maxColumns} step="1" value={maxColumns + 1 - columns}
        aria-label={i18n.t("thumbnail-size")}
        oninput={(e) => (settings.thumbnailColumns = maxColumns + 1 - Number(e.currentTarget.value))} />
      <button class="icon" onclick={() => resize(true)} disabled={columns <= 1}
        aria-label={i18n.t("thumbnails-bigger")} title={i18n.t("thumbnails-bigger")}>
        <Icon name="plus" />
      </button>
    </div>
  {/if}
  <div class="thumbs" bind:this={scroller} bind:clientHeight={viewportHeight} bind:clientWidth={paneWidth}
    onscroll={() => (scrollTop = scroller.scrollTop)}>
    <div class="content" style:height="{layout.totalHeight}px">
      {#if paneWidth > 0}
        {#each { length: Math.max(0, range[1] - range[0] + 1) } as _, i (range[0] + i)}
          {@const index = range[0] + i}
          {@const size = rotatedSize(tab.info.pageSizes[index], tab.rotation)}
          {@const row = layout.rows[layout.rowOf[index]]}
          <button
            class="thumb"
            class:current={index === tab.page}
            style:inset-inline-start="{layout.lefts[index]}px"
            style:top="{row.top}px"
            style:width="{layout.widths[index]}px"
            style:height="{row.height}px"
            onclick={() => ongoto(index)}
            aria-label={i18n.t("go-to-page-n", { page: index + 1 })}
            aria-current={index === tab.page ? "page" : undefined}
          >
            <PageView
              docId={tab.docId}
              {index}
              width={layout.widths[index]}
              height={layout.heights[index]}
              widthPt={size.width}
              rotation={tab.rotation}
              revision={tab.pageRevision(index)}
              thumbnail
            />
            <span class="label">{(index + 1).toLocaleString(i18n.locale)}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .wrap {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .size {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-block: 6px;
    padding-inline: 8px;
    border-block-end: 1px solid var(--border);
  }
  .size input {
    flex: 1;
    min-width: 0;
    accent-color: var(--accent);
  }
  .size button {
    border: none;
  }
  .thumbs {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    /* The width doesn't jump when the scrollbar comes and goes. */
    scrollbar-gutter: stable;
  }
  .content {
    position: relative;
  }
  .thumb {
    position: absolute;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    gap: 0;
    padding: 0;
    border: none;
    background: none;
    border-radius: 4px;
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
    flex: none;
    height: 24px;
    line-height: 24px;
    font-size: 12px;
    color: var(--muted);
  }
  .thumb.current .label {
    color: var(--text);
    font-weight: 600;
  }
</style>
