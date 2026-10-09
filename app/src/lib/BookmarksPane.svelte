<script lang="ts">
// The document's bookmarks (its outline): go to a page, add, rename, delete,
// drag to reorder or nest, and move with the keyboard (Alt+arrows). Changes
// are written into the PDF when it is saved, so every reader shows them.
import { SvelteSet } from "svelte/reactivity";
import BookmarkNode, { type BookmarkTree } from "./BookmarkNode.svelte";
import { addBookmark, editBookmarks, loadBookmarks } from "./bookmarks";
import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { at, type Bookmark, type Drop, flatten, indent, move, outdent, pathOf, removeAt, shift } from "./outlineTree";
import { type RivetError, toRivetError } from "./pdf";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  ongoto: (page: number) => void;
  onerror?: (e: RivetError) => void;
}
let { tab, ongoto, onerror }: Props = $props();

$effect(() => {
  loadBookmarks(tab);
});

let items = $derived(tab.outline);
let editable = $derived(tab.info.canEditOutline);
const expanded = new SvelteSet<number>();
let renaming = $state<number | null>(null);
let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
let list = $state<HTMLElement>();

// Dragging a bookmark to another place.
let drag = $state<{ key: number; startX: number; startY: number; moving: boolean } | null>(null);
let dropTarget = $state<{ key: number; where: Drop } | null>(null);

/** Puts keyboard focus on a bookmark (after it moved or was added). */
function focusBookmark(key: number) {
  requestAnimationFrame(() => list?.querySelector<HTMLElement>(`[data-bookmark="${key}"]`)?.focus());
}

/** Opens the parents of a bookmark so it can be seen. */
function reveal(key: number) {
  const path = items ? pathOf(items, key) : null;
  if (!path || !items) return;
  let level = items;
  for (const i of path.slice(0, -1)) {
    expanded.add(level[i].key);
    level = level[i].children;
  }
}

async function edit(change: (items: Bookmark[]) => boolean | undefined, focusKey?: number) {
  try {
    if ((await editBookmarks(tab, change)) && focusKey !== undefined) {
      reveal(focusKey);
      focusBookmark(focusKey);
    }
  } catch (e) {
    onerror?.(toRivetError(e));
  }
}

/** Bookmarks a page (inside `parent`, if given) and starts renaming the new bookmark. */
export async function add(page = tab.page, parent?: number) {
  if (!editable) return;
  try {
    const key = await addBookmark(tab, page, parent);
    if (parent !== undefined) expanded.add(parent);
    reveal(key);
    renaming = key;
  } catch (e) {
    onerror?.(toRivetError(e));
  }
}

function remove(key: number) {
  // Focus goes to the entry that takes its place.
  const visible = items ? flatten(items, (k) => expanded.has(k)) : [];
  const index = visible.findIndex((p) => items && pathOf(items, key)?.join() === p.join());
  edit((all) => {
    const path = pathOf(all, key);
    return path ? removeAt(all, path) !== undefined : false;
  }).then(() => {
    const after = items ? flatten(items, (k) => expanded.has(k)) : [];
    const next = after[Math.min(index, after.length - 1)];
    const item = next && items ? at(items, next) : undefined;
    if (item) focusBookmark(item.key);
  });
}

function moveFocus(from: number, by: 1 | -1 | "first" | "last") {
  if (!items) return;
  const visible = flatten(items, (k) => expanded.has(k));
  const here = visible.findIndex((p) => items && pathOf(items, from)?.join() === p.join());
  const index = by === "first" ? 0 : by === "last" ? visible.length - 1 : here + by;
  const item = visible[index] && at(items, visible[index]);
  if (item) focusBookmark(item.key);
}

const tree: BookmarkTree = {
  get current() {
    return tab.page;
  },
  get editable() {
    return editable;
  },
  get renaming() {
    return renaming;
  },
  get dropTarget() {
    return dropTarget;
  },
  get draggingKey() {
    return drag?.moving ? drag.key : null;
  },
  isOpen: (key) => expanded.has(key),
  toggle(key, open = !expanded.has(key)) {
    if (open) expanded.add(key);
    else expanded.delete(key);
  },
  go(item) {
    if (item.page !== null) ongoto(item.page);
  },
  startRename(key) {
    renaming = key;
  },
  finishRename(key, title) {
    if (renaming !== key) return;
    renaming = null;
    focusBookmark(key);
    const text = title?.trim();
    if (!text) return;
    edit((all) => {
      const path = pathOf(all, key);
      const item = path && at(all, path);
      if (!item || item.title === text) return false;
      item.title = text;
    });
  },
  onkey(e, item) {
    const rtl = getComputedStyle(e.currentTarget as HTMLElement).direction === "rtl";
    const forward = rtl ? "ArrowLeft" : "ArrowRight";
    const back = rtl ? "ArrowRight" : "ArrowLeft";
    if (e.altKey && editable) {
      const moved =
        e.key === "ArrowUp"
          ? (all: Bookmark[]) => shift(all, item.key, -1)
          : e.key === "ArrowDown"
            ? (all: Bookmark[]) => shift(all, item.key, 1)
            : e.key === forward
              ? (all: Bookmark[]) => indent(all, item.key)
              : e.key === back
                ? (all: Bookmark[]) => outdent(all, item.key)
                : null;
      if (moved) {
        e.preventDefault();
        edit(moved, item.key);
      }
      return;
    }
    const has = item.children.length > 0;
    switch (e.key) {
      case "F2":
        if (editable) renaming = item.key;
        break;
      case "Delete":
        if (editable) remove(item.key);
        break;
      case "ArrowDown":
        moveFocus(item.key, 1);
        break;
      case "ArrowUp":
        moveFocus(item.key, -1);
        break;
      case "Home":
        moveFocus(item.key, "first");
        break;
      case "End":
        moveFocus(item.key, "last");
        break;
      case forward:
        if (has && !expanded.has(item.key)) expanded.add(item.key);
        else if (has) moveFocus(item.key, 1);
        break;
      case back:
        if (has && expanded.has(item.key)) expanded.delete(item.key);
        else {
          const path = items ? pathOf(items, item.key) : null;
          const parent = path && path.length > 1 && items ? at(items, path.slice(0, -1)) : undefined;
          if (parent) focusBookmark(parent.key);
        }
        break;
      default:
        return;
    }
    e.preventDefault();
    // Keys handled here aren't shortcuts for the rest of the app.
    e.stopPropagation();
  },
  onmenu(e, item) {
    e.preventDefault();
    const entries: MenuItem[] = [];
    if (item.page !== null)
      entries.push({ label: i18n.t("go-to-page-n", { page: item.page + 1 }), action: () => tree.go(item) });
    if (editable) {
      entries.push(
        { label: i18n.t("bookmark-rename"), shortcut: "F2", action: () => (renaming = item.key) },
        { label: i18n.t("bookmark-add-inside"), action: () => add(item.page ?? tab.page, item.key) },
        { label: i18n.t("bookmark-delete"), shortcut: "Delete", action: () => remove(item.key) },
      );
    }
    menu = { x: e.clientX, y: e.clientY, items: entries };
  },
  ondragstart(e, item) {
    if (!editable || e.button !== 0) return;
    drag = { key: item.key, startX: e.clientX, startY: e.clientY, moving: false };
  },
};

function onpointermove(e: PointerEvent) {
  if (!drag) return;
  if (!drag.moving) {
    if (Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY) < 5) return;
    drag.moving = true;
  }
  const row = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-key]");
  const key = row ? Number(row.dataset.key) : null;
  if (!row || key === null || key === drag.key) {
    dropTarget = null;
    return;
  }
  const r = row.getBoundingClientRect();
  const y = (e.clientY - r.top) / r.height;
  dropTarget = { key, where: y < 0.3 ? "before" : y > 0.7 ? "after" : "inside" };
}

function onpointerup() {
  const d = drag;
  const target = dropTarget;
  drag = null;
  dropTarget = null;
  if (!d?.moving) return;
  // The click that follows the drop shouldn't also go to the page.
  justDragged = true;
  setTimeout(() => (justDragged = false));
  if (!target) return;
  if (target.where === "inside") expanded.add(target.key);
  edit((all) => move(all, d.key, target.key, target.where), d.key);
}

let justDragged = false;
function onclickcapture(e: MouseEvent) {
  if (justDragged) {
    e.stopPropagation();
    e.preventDefault();
  }
}
</script>

<svelte:window {onpointermove} {onpointerup} onpointercancel={() => ((drag = null), (dropTarget = null))} />

<div class="pane">
  <div class="bar">
    <button class="add" onclick={() => add()} disabled={!editable}
      title={editable ? i18n.t("bookmark-add") : i18n.t("bookmarks-protected")} data-shortcut="Ctrl+B">
      <Icon name="bookmark-add" />
      {i18n.t("bookmark-add")}
    </button>
  </div>
  {#if items === null}
    <p class="empty">…</p>
  {:else if items.length === 0}
    <p class="empty">{i18n.t("bookmarks-empty")}</p>
  {:else}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <ul class="tree" role="tree" aria-label={i18n.t("bookmarks")} bind:this={list} onclickcapture={onclickcapture}
      class:dragging={drag?.moving}>
      {#each items as item (item.key)}
        <BookmarkNode {item} depth={0} {tree} />
      {/each}
    </ul>
  {/if}
</div>

{#if menu}
  <ContextMenu {...menu} onclose={() => (menu = null)} />
{/if}

<style>
  .pane {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .bar {
    padding-block: 6px;
    padding-inline: 8px;
    border-block-end: 1px solid var(--border);
  }
  .add {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding-block: 4px;
    padding-inline: 6px;
    border: none;
    font-size: 13px;
    text-align: start;
  }
  .add :global(.icon) {
    flex: none;
    width: 16px;
    height: 16px;
  }
  .tree {
    flex: 1;
    min-height: 0;
    overflow: auto;
    list-style: none;
    margin: 0;
    padding: 8px;
    font-size: 13px;
  }
  .tree.dragging {
    cursor: grabbing;
    user-select: none;
  }
  .empty {
    padding: 16px;
    color: var(--muted);
    font-size: 13px;
  }
</style>
