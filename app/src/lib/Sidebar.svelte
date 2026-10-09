<script lang="ts">
// Side panel with page thumbnails, bookmarks (the outline) and search results.
import { tick } from "svelte";
import BookmarksPane from "./BookmarksPane.svelte";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { fade } from "./motion";
import type { RivetError } from "./pdf";
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
  onerror?: (e: RivetError) => void;
}
let { tab, ongoto, onfind, pane = $bindable("thumbnails"), width, onerror }: Props = $props();

const panes = [
  { id: "thumbnails", icon: "pages", label: "thumbnails" },
  { id: "outline", icon: "bookmark", label: "bookmarks-tab" },
  { id: "search", icon: "search", label: "search-tab" },
] as const;

let bookmarks = $state<BookmarksPane>();

/** Bookmarks a page (the current one by default) and shows the new bookmark, ready to rename. */
export async function addBookmark(page = tab.page) {
  pane = "outline";
  await tick();
  await bookmarks?.add(page);
}
</script>

<aside class="sidebar" style:width="{width}px">
  <div class="switcher" role="tablist">
    {#each panes as p (p.id)}
      <button role="tab" aria-selected={pane === p.id} onclick={() => (pane = p.id)} title={i18n.t(p.label)}>
        <Icon name={p.icon} />
        <span class="label">{i18n.t(p.label)}</span>
      </button>
    {/each}
  </div>
  <div class="pane" class:fixed={pane === "search"}>
    {#key pane}
    <div class="pane-content" in:fade>
    {#if pane === "thumbnails"}
      <Thumbnails {tab} {ongoto} onbookmark={addBookmark} />
    {:else if pane === "search"}
      <SearchPane {tab} {onfind} />
    {:else}
      <BookmarksPane bind:this={bookmarks} {tab} {ongoto} {onerror} />
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
    container-type: inline-size;
  }
  .switcher button {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
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
  .switcher :global(.icon) {
    flex: none;
    width: 16px;
    height: 16px;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Narrow sidebar: only the open pane keeps its name; the others are icons. */
  @container (width < 300px) {
    .switcher button:not([aria-selected="true"]) {
      flex: none;
    }
    .switcher button:not([aria-selected="true"]) .label {
      display: none;
    }
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
</style>
