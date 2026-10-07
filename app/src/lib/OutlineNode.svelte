<script lang="ts">
// One outline entry, with its children (recursive).
import type { OutlineItem } from "./bindings/OutlineItem";
import OutlineNode from "./OutlineNode.svelte";

interface Props {
  item: OutlineItem;
  depth: number;
  current: number;
  ongoto: (page: number) => void;
}
let { item, depth, current, ongoto }: Props = $props();

let open = $state(false);
let hasChildren = $derived(item.children.length > 0);
</script>

<li role="treeitem" aria-expanded={hasChildren ? open : undefined} aria-selected={item.page === current}>
  <div class="row" style:padding-inline-start="{depth * 14}px">
    {#if hasChildren}
      <button class="toggle" onclick={() => (open = !open)} aria-label={item.title} tabindex="-1">
        <svg class:open class="flip-rtl" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg>
      </button>
    {:else}
      <span class="toggle"></span>
    {/if}
    <button
      class="title"
      class:current={item.page === current}
      disabled={item.page === null}
      onclick={() => item.page !== null && ongoto(item.page)}
      title={item.title}
    >
      {item.title || "—"}
    </button>
  </div>
  {#if hasChildren && open}
    <ul role="group">
      {#each item.children as child, i (i)}
        <OutlineNode item={child} depth={depth + 1} {current} {ongoto} />
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
    display: flex;
    align-items: center;
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
    transition: transform 0.12s;
  }
  .toggle svg.open {
    transform: rotate(90deg);
  }
  :global([dir="rtl"]) .toggle svg.open {
    transform: scaleX(-1) rotate(90deg);
  }
  .title {
    flex: 1;
    min-width: 0;
    text-align: start;
    border: none;
    background: none;
    padding-block: 5px;
    padding-inline: 6px;
    border-radius: 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title.current {
    color: var(--accent);
    font-weight: 600;
  }
  .title:disabled {
    opacity: 1;
    color: var(--muted);
    cursor: default;
  }
</style>
