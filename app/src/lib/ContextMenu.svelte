<script lang="ts" module>
export interface MenuItem {
  label: string;
  /** Shown at the end, e.g. "Ctrl+C" (formatted for the platform). */
  shortcut?: string;
  disabled?: boolean;
  /** Explains why the item is disabled. */
  hint?: string;
  action: () => void;
}
</script>

<script lang="ts">
// A right-click menu at the pointer. Closes on Escape, on a click elsewhere,
// when the window loses focus or after choosing an item.
//
// A manual popover: on Linux and macOS the right-click menu event comes when
// the button goes down, and an automatic popover would close again as soon
// as the button is released outside it.
import { formatShortcut } from "./tooltip";

interface Props {
  x: number;
  y: number;
  items: MenuItem[];
  onclose: () => void;
}
let { x, y, items, onclose }: Props = $props();

let menu: HTMLDivElement;

$effect(() => {
  menu.showPopover();
  // Keep the menu inside the window.
  const width = menu.offsetWidth;
  const height = menu.offsetHeight;
  menu.style.left = `${Math.max(4, Math.min(x, innerWidth - width - 4))}px`;
  menu.style.top = `${Math.max(4, Math.min(y, innerHeight - height - 4))}px`;
  menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();

  const outside = (e: Event) => {
    if (!menu.contains(e.target as Node)) onclose();
  };
  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onclose();
    }
  };
  window.addEventListener("pointerdown", outside, true);
  window.addEventListener("wheel", onclose, true);
  window.addEventListener("blur", onclose);
  window.addEventListener("keydown", key, true);
  return () => {
    window.removeEventListener("pointerdown", outside, true);
    window.removeEventListener("wheel", onclose, true);
    window.removeEventListener("blur", onclose);
    window.removeEventListener("keydown", key, true);
  };
});

function choose(item: MenuItem) {
  onclose();
  item.action();
}
</script>

<div class="context-menu" popover="manual" role="menu" bind:this={menu}>
  {#each items as item (item.label)}
    <button class="item" role="menuitem" disabled={item.disabled} title={item.disabled ? item.hint : undefined}
      onclick={() => choose(item)}>
      <span>{item.label}</span>
      {#if item.shortcut}<kbd dir="ltr">{formatShortcut(item.shortcut)}</kbd>{/if}
    </button>
  {/each}
</div>

<style>
  .context-menu {
    position: fixed;
    margin: 0;
    inset: auto;
    min-width: 200px;
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
    justify-content: space-between;
    gap: 24px;
    width: 100%;
    border: none;
    font-weight: 400;
    text-align: start;
  }
  kbd {
    font: inherit;
    font-size: 12px;
    color: var(--muted);
  }
</style>
