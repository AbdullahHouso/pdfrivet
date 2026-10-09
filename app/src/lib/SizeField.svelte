<script lang="ts">
// A font size: type any size, or pick a common one from the list under the
// arrow. Up and down arrow keys step through the common sizes. (A number input
// with a datalist leaves almost no room for the digits on Windows.)
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { clampSize, FONT_SIZES, stepSize } from "./textBox";

interface Props {
  value: number;
  onchange: (size: number) => void;
}
let { value, onchange }: Props = $props();

let menu: HTMLDivElement;
let field: HTMLDivElement;
let text = $state("");
$effect(() => {
  text = String(value);
});

function commit() {
  // Arabic-Indic digits count too.
  const western = text.replace(/[٠-٩]/g, (d) => String(d.charCodeAt(0) - 0x0660)).replace(",", ".");
  const n = Number.parseFloat(western);
  if (Number.isFinite(n) && n > 0) onchange(clampSize(n));
  else text = String(value);
}

function open() {
  menu.showPopover();
  const r = field.getBoundingClientRect();
  menu.style.left = `${r.left}px`;
  menu.style.top = `${r.bottom + 4}px`;
  menu.style.minWidth = `${r.width}px`;
  menu.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: "center" });
}

function pick(size: number) {
  menu.hidePopover();
  onchange(size);
}
</script>

<div class="size-field" bind:this={field}>
  <input
    value={text}
    inputmode="decimal"
    title={i18n.t("text-size")}
    aria-label={i18n.t("text-size")}
    oninput={(e) => (text = e.currentTarget.value)}
    onchange={commit}
    onkeydown={(e) => {
      if (e.key === "Enter") commit();
      else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
        e.preventDefault();
        onchange(stepSize(value, e.key === "ArrowUp" ? 1 : -1));
      }
    }}
  />
  <button class="arrow" onclick={open} tabindex="-1" aria-label={i18n.t("text-size")} aria-haspopup="listbox">
    <Icon name="chevron-down" />
  </button>
</div>

<div class="size-menu" popover bind:this={menu} role="listbox" aria-label={i18n.t("text-size")}>
  {#each FONT_SIZES as size (size)}
    <button role="option" aria-selected={size === value} onclick={() => pick(size)}>
      {size.toLocaleString(i18n.locale)}
    </button>
  {/each}
</div>

<style>
  .size-field {
    display: flex;
    align-items: center;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }
  .size-field:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  input {
    width: 3.2em;
    padding-block: 0;
    padding-inline: 8px 0;
    border: none;
    background: none;
    color: var(--text);
    text-align: center;
    outline: none;
  }
  .arrow {
    display: grid;
    place-items: center;
    width: 22px;
    height: 100%;
    padding: 0;
    border: none;
    background: none;
  }
  .arrow :global(svg) {
    width: 14px;
    height: 14px;
  }
  .size-menu {
    position: fixed;
    margin: 0;
    inset: auto;
    max-height: 280px;
    overflow: auto;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
  }
  .size-menu button {
    display: block;
    width: 100%;
    padding: 4px 10px;
    border: none;
    background: none;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .size-menu button:hover {
    background: var(--hover);
  }
  .size-menu button[aria-selected="true"] {
    color: var(--accent);
    font-weight: 600;
  }
</style>
