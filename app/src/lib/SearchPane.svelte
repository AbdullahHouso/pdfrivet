<script lang="ts">
// Every search result in a list, with the text around it. Clicking one shows
// it on the page. Only the rows in view exist, so thousands of results stay fast.
import { i18n } from "./i18n.svelte";
import type { Tab } from "./tabs.svelte";
import { formatShortcut } from "./tooltip";

interface Props {
  tab: Tab;
  /** Opens the find bar (when nothing has been searched yet). */
  onfind: () => void;
}
let { tab, onfind }: Props = $props();
let search = $derived(tab.search);

const ROW = 68;
let scroller: HTMLElement | undefined = $state();
let scrollTop = $state(0);
let viewportHeight = $state(0);

let first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 2));
let last = $derived(Math.min(search.hits.length, Math.ceil((scrollTop + viewportHeight) / ROW) + 2));

// Keep the current result in view as you step through them.
$effect(() => {
  const i = search.current;
  if (!scroller || i < 0) return;
  const top = i * ROW;
  if (top < scroller.scrollTop || top + ROW > scroller.scrollTop + viewportHeight) {
    scroller.scrollTo({ top: top - viewportHeight / 3 });
  }
});
</script>

{#if !search.searched}
  <div class="empty">
    <p>{i18n.t("search-pane-empty")}</p>
    <button onclick={onfind}>
      {i18n.t("search-placeholder")}
      <kbd dir="ltr">{formatShortcut("Ctrl+F")}</kbd>
    </button>
  </div>
{:else if search.hits.length === 0}
  <p class="empty">{search.running ? i18n.t("search-searching") : i18n.t("search-no-results")}</p>
{:else}
  <p class="summary" aria-live="polite">
    {i18n.t("search-results", { total: search.hits.length })}{search.running ? "…" : ""}
  </p>
  <div
    class="results"
    role="listbox"
    aria-label={i18n.t("search-show-all")}
    bind:this={scroller}
    bind:clientHeight={viewportHeight}
    onscroll={() => scroller && (scrollTop = scroller.scrollTop)}
  >
    <div class="spacer" style:height="{search.hits.length * ROW}px">
      {#each search.hits.slice(first, last) as hit, i (first + i)}
        {@const index = first + i}
        <button
          class="result"
          role="option"
          aria-selected={index === search.current}
          style:top="{index * ROW}px"
          style:height="{ROW - 4}px"
          onclick={() => (search.current = index)}
        >
          <span class="page">{i18n.t("search-page", { page: hit.page + 1 })}</span>
          <span class="snippet" dir="auto">{hit.before}<mark>{hit.text}</mark>{hit.after}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .empty {
    padding: 16px;
    color: var(--muted);
    font-size: 13px;
  }
  .empty button {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    margin-block-start: 8px;
    font-size: 13px;
  }
  .empty kbd {
    font: inherit;
    color: var(--muted);
  }
  .summary {
    margin: 0;
    padding-block: 8px 4px;
    padding-inline: 12px;
    font-size: 12px;
    color: var(--muted);
  }
  .results {
    height: calc(100% - 32px);
    overflow: auto;
  }
  .spacer {
    position: relative;
  }
  .result {
    position: absolute;
    inset-inline: 6px;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
    padding-block: 6px;
    padding-inline: 8px;
    border-color: transparent;
    font-weight: 400;
    text-align: start;
    overflow: hidden;
  }
  .result[aria-selected="true"] {
    background: var(--hover);
    border-color: var(--border);
  }
  .page {
    font-size: 11px;
    color: var(--muted);
  }
  .snippet {
    font-size: 13px;
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    text-align: start;
  }
  mark {
    background: var(--search-hit);
    color: inherit;
    border-radius: 2px;
  }
</style>
