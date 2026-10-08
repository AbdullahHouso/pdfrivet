<script lang="ts">
// "Open recent": a list of recently opened files that opens beside the app
// menu (towards the window's middle, so it mirrors in RTL). Files that no
// longer exist are marked; opening one explains what happened (see App.svelte).
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { filesExist } from "./pdf";
import { settings } from "./settings.svelte";

interface Props {
  onopen: (path: string) => void;
}
let { onopen }: Props = $props();

/** How many recent files the menu lists (the start screen shows them all). */
const MAX = 10;

let menu: HTMLDivElement;
let missing = $state<Set<string>>(new Set());
let files = $derived(settings.recent.slice(0, MAX));

/** Opens the list next to `anchor` (the "Open recent" item). */
export function open(anchor: HTMLElement) {
  const paths = files.map((f) => f.path);
  if (paths.length > 0) {
    filesExist(paths)
      .then((exists) => (missing = new Set(paths.filter((_, i) => !exists[i]))))
      .catch(() => {});
  }
  menu.showPopover();
  const item = anchor.getBoundingClientRect();
  const parent = (anchor.closest("[popover]") ?? anchor).getBoundingClientRect();
  const width = menu.offsetWidth;
  const rtl = document.documentElement.dir === "rtl";
  // The app menu sits at the window's end, so the list opens towards its start.
  const left = rtl ? parent.right + 4 : parent.left - width - 4;
  menu.style.left = `${Math.max(8, Math.min(left, innerWidth - width - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(item.top - 8, innerHeight - menu.offsetHeight - 8))}px`;
}

export function close() {
  if (menu?.matches(":popover-open")) menu.hidePopover();
}

function choose(path: string) {
  close();
  onopen(path);
}
</script>

<!-- A popover inside the app menu, so opening it keeps the app menu open. -->
<div class="recent-menu" popover bind:this={menu} role="menu" aria-label={i18n.t("open-recent")}>
  {#if files.length === 0}
    <p class="empty">{i18n.t("no-recent-files")}</p>
  {:else}
    {#each files as file (file.path)}
      {@const gone = missing.has(file.path)}
      <button class="item" class:gone role="menuitem" onclick={() => choose(file.path)}>
        <Icon name="file" />
        <span class="text">
          <span class="name" dir="auto">{file.title}</span>
          <span class="path" dir="ltr">{gone ? i18n.t("file-missing") : file.path}</span>
        </span>
      </button>
    {/each}
    <hr />
    <button class="item clear" role="menuitem" onclick={() => { settings.clearRecent(); close(); }}>
      {i18n.t("clear-recent")}
    </button>
  {/if}
</div>

<style>
  .recent-menu {
    position: fixed;
    inset: auto;
    margin: 0;
    width: 340px;
    max-height: min(480px, calc(100vh - 16px));
    overflow: auto;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 10px;
    border: none;
    font-weight: 400;
    text-align: start;
  }
  .item :global(.icon) {
    flex: none;
    color: var(--muted);
  }
  .text {
    display: grid;
    min-width: 0;
  }
  .name,
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path {
    font-size: 12px;
    color: var(--muted);
    text-align: start;
  }
  .gone .name {
    color: var(--muted);
    text-decoration: line-through;
  }
  .gone .path {
    color: var(--error-fg);
  }
  .clear {
    color: var(--muted);
    font-size: 13px;
  }
  .empty {
    margin: 8px 10px;
    color: var(--muted);
  }
  hr {
    border: none;
    border-block-start: 1px solid var(--border);
    margin-block: 4px;
  }
</style>
