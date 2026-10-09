<script lang="ts" module>
import type { Bookmark, Drop } from "./outlineTree";

/** What the bookmarks pane shares with every entry. */
export interface BookmarkTree {
  readonly current: number;
  readonly editable: boolean;
  readonly renaming: number | null;
  /** Where the dragged entry would land. */
  readonly dropTarget: { key: number; where: Drop } | null;
  readonly draggingKey: number | null;
  isOpen(key: number): boolean;
  toggle(key: number, open?: boolean): void;
  go(item: Bookmark): void;
  startRename(key: number): void;
  finishRename(key: number, title: string | null): void;
  onkey(e: KeyboardEvent, item: Bookmark): void;
  onmenu(e: MouseEvent, item: Bookmark): void;
  ondragstart(e: PointerEvent, item: Bookmark): void;
}
</script>

<script lang="ts">
// One bookmark, with its children (recursive). Click to go to its page,
// double-click (or F2) to rename; the pane handles the rest.
import BookmarkNode from "./BookmarkNode.svelte";

interface Props {
  item: Bookmark;
  depth: number;
  tree: BookmarkTree;
}
let { item, depth, tree }: Props = $props();

let hasChildren = $derived(item.children.length > 0);
let open = $derived(hasChildren && tree.isOpen(item.key));
let drop = $derived(tree.dropTarget?.key === item.key ? tree.dropTarget.where : null);

function focusInput(input: HTMLInputElement) {
  input.focus();
  input.select();
}

function onRenameKey(e: KeyboardEvent) {
  e.stopPropagation();
  if (e.key === "Enter") tree.finishRename(item.key, e.currentTarget instanceof HTMLInputElement ? e.currentTarget.value : null);
  else if (e.key === "Escape") tree.finishRename(item.key, null);
}
</script>

<li role="treeitem" aria-expanded={hasChildren ? open : undefined} aria-selected={item.page === tree.current}>
  <div
    class="row"
    class:drop-before={drop === "before"}
    class:drop-after={drop === "after"}
    class:drop-inside={drop === "inside"}
    class:dragged={tree.draggingKey === item.key}
    data-key={item.key}
    style:padding-inline-start="{depth * 14}px"
  >
    {#if hasChildren}
      <button class="toggle" onclick={() => tree.toggle(item.key)} aria-label={item.title} tabindex="-1">
        <svg class:open class="flip-rtl" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg>
      </button>
    {:else}
      <span class="toggle"></span>
    {/if}
    {#if tree.renaming === item.key}
      <input
        class="rename"
        dir="auto"
        value={item.title}
        use:focusInput
        onkeydown={onRenameKey}
        onblur={(e) => tree.finishRename(item.key, e.currentTarget.value)}
      />
    {:else}
      <button
        class="title"
        class:current={item.page === tree.current}
        class:no-page={item.page === null}
        data-bookmark={item.key}
        onclick={() => tree.go(item)}
        ondblclick={() => tree.editable && tree.startRename(item.key)}
        onkeydown={(e) => tree.onkey(e, item)}
        oncontextmenu={(e) => tree.onmenu(e, item)}
        onpointerdown={(e) => tree.ondragstart(e, item)}
        title={item.title}
        dir="auto"
      >
        {item.title || "—"}
      </button>
    {/if}
  </div>
  {#if open}
    <ul role="group">
      {#each item.children as child (child.key)}
        <BookmarkNode item={child} depth={depth + 1} {tree} />
      {/each}
    </ul>
  {/if}
</li>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    border-radius: 4px;
  }
  .row.dragged {
    opacity: 0.5;
  }
  /* Where a dragged bookmark would go: a line above or below, or into this one. */
  .row.drop-before::before,
  .row.drop-after::before {
    content: "";
    position: absolute;
    inset-inline: 0;
    height: 2px;
    background: var(--accent);
    pointer-events: none;
  }
  .row.drop-before::before {
    top: -1px;
  }
  .row.drop-after::before {
    bottom: -1px;
  }
  .row.drop-inside {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .toggle {
    flex: none;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    background: none;
    display: grid;
    place-items: center;
  }
  .toggle svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    transition: transform var(--motion-hover) var(--ease-out);
  }
  .toggle svg.open {
    transform: rotate(90deg);
  }
  :global([dir="rtl"]) .toggle svg.open {
    transform: scaleX(-1) rotate(90deg);
  }
  /* Each title runs in its own direction (so a long English title is cut at
     its end in Arabic too), but all line up with the sidebar's start edge. */
  .title {
    flex: 1;
    min-width: 0;
    text-align: left;
    border: none;
    background: none;
    padding-block: 5px;
    padding-inline: 6px;
    border-radius: 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  :global([dir="rtl"]) .title {
    text-align: right;
  }
  .title:hover {
    background: var(--hover);
  }
  .title.current {
    color: var(--accent);
    font-weight: 600;
  }
  .title.no-page {
    color: var(--muted);
  }
  .rename {
    flex: 1;
    min-width: 0;
    margin-block: 1px;
    padding-block: 3px;
    padding-inline: 5px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--surface);
    color: var(--text);
    font-size: 13px;
  }
</style>
