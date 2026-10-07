<script lang="ts">
// Side panel with page thumbnails and the document outline.
import { i18n } from "./i18n.svelte";
import OutlineNode from "./OutlineNode.svelte";
import { getOutline } from "./pdf";
import Thumbnails from "./Thumbnails.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  ongoto: (page: number) => void;
}
let { tab, ongoto }: Props = $props();

let pane = $state<"thumbnails" | "outline">("thumbnails");

// Load the outline the first time it is shown.
$effect(() => {
  if (pane === "outline" && tab.outline === null) {
    getOutline(tab.docId)
      .then((items) => (tab.outline = items))
      .catch(() => (tab.outline = []));
  }
});
</script>

<aside class="sidebar">
  <div class="switcher" role="tablist">
    <button role="tab" aria-selected={pane === "thumbnails"} onclick={() => (pane = "thumbnails")}>
      {i18n.t("thumbnails")}
    </button>
    <button role="tab" aria-selected={pane === "outline"} onclick={() => (pane = "outline")}>
      {i18n.t("outline")}
    </button>
  </div>
  <div class="pane">
    {#if pane === "thumbnails"}
      <Thumbnails {tab} {ongoto} />
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
</aside>

<style>
  .sidebar {
    flex: none;
    width: 200px;
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
