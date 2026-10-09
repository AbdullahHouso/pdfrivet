<script lang="ts">
// Side panel with page thumbnails, the document outline and search results.
import { i18n } from "./i18n.svelte";
import { fade } from "./motion";
import OutlineNode from "./OutlineNode.svelte";
import { getOutline } from "./pdf";
import SearchPane from "./SearchPane.svelte";
import Thumbnails from "./Thumbnails.svelte";
import type { Tab } from "./tabs.svelte";

export type SidebarPane = "thumbnails" | "outline" | "search";

interface Props {
  tab: Tab;
  ongoto: (page: number) => void;
  /** Opens the find bar. */
  onfind: () => void;
  pane?: SidebarPane;
  /** Width in px (the splitter next to it changes it). */
  width: number;
}
let { tab, ongoto, onfind, pane = $bindable("thumbnails"), width }: Props = $props();

// Load the outline the first time it is shown.
$effect(() => {
  if (pane === "outline" && tab.outline === null) {
    getOutline(tab.docId)
      .then((items) => (tab.outline = items))
      .catch(() => (tab.outline = []));
  }
});
</script>

<aside class="sidebar" style:width="{width}px">
  <div class="switcher" role="tablist">
    <button role="tab" aria-selected={pane === "thumbnails"} onclick={() => (pane = "thumbnails")}>
      {i18n.t("thumbnails")}
    </button>
    <button role="tab" aria-selected={pane === "outline"} onclick={() => (pane = "outline")}>
      {i18n.t("outline")}
    </button>
    <button role="tab" aria-selected={pane === "search"} onclick={() => (pane = "search")}>
      {i18n.t("search-tab")}
    </button>
  </div>
  <div class="pane" class:fixed={pane === "search"}>
    {#key pane}
    <div class="pane-content" in:fade>
    {#if pane === "thumbnails"}
      <Thumbnails {tab} {ongoto} />
    {:else if pane === "search"}
      <SearchPane {tab} {onfind} />
    {:else if tab.outline === null}
      <p class="empty">…</p>
    {:else if tab.outline.length === 0}
      <p class="empty">{i18n.t("outline-empty")}</p>
    {:else}
      <ul class="tree" role="tree" aria-label={i18n.t("outline")}>
        {#each tab.outline as item, i (i)}
          <OutlineNode {item} depth={0} current={tab.page} {ongoto} />
        {/each}
      </ul>
    {/if}
    </div>
    {/key}
  </div>
</aside>

<style>
  .sidebar {
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-inline-end: 1px solid var(--border);
    min-height: 0;
  }
  .switcher {
    display: flex;
    gap: 4px;
    padding: 8px;
    border-block-end: 1px solid var(--border);
  }
  .switcher button {
    flex: 1;
    padding-block: 4px;
    padding-inline: 6px;
    border-color: transparent;
    font-size: 13px;
  }
  .switcher button[aria-selected="true"] {
    background: var(--hover);
    border-color: var(--border);
    font-weight: 600;
  }
  .pane {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .pane-content {
    height: 100%;
  }
  /* The search results scroll by themselves (they're virtualized). */
  .pane.fixed {
    overflow: hidden;
  }
  .tree {
    list-style: none;
    margin: 0;
    padding: 8px;
    font-size: 13px;
  }
  .empty {
    padding: 16px;
    color: var(--muted);
    font-size: 13px;
  }
</style>
