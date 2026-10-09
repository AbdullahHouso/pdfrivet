<script lang="ts" module>
import type { FontInfo } from "./bindings/FontInfo";
import { listFonts } from "./pdf";

// The installed fonts are looked up once (the first time can take a moment).
let fontList: Promise<FontInfo[]> | null = null;
function loadFonts(): Promise<FontInfo[]> {
  fontList ??= listFonts().catch(() => {
    fontList = null;
    return [];
  });
  return fontList;
}
</script>

<script lang="ts">
// Choosing a text box's font: PDFRivet's own fonts first, then the installed
// ones (those with Arabic letters, then the rest). Each name is shown in its font.
import type { TextFont } from "./bindings/TextFont";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { cssFamily, groupFonts } from "./textBox";

interface Props {
  value: TextFont;
  onchange: (font: TextFont) => void;
}
let { value, onchange }: Props = $props();

let menu: HTMLDivElement;
let button: HTMLButtonElement;
let search: HTMLInputElement | undefined = $state();
let fonts = $state<FontInfo[] | null>(null);
let query = $state("");

let groups = $derived.by(() => {
  const q = query.trim().toLowerCase();
  const found = (fonts ?? []).filter((f) => !q || f.family.toLowerCase().includes(q));
  const g = groupFonts(found);
  return [
    { label: "text-fonts-bundled", fonts: g.bundled },
    { label: "text-fonts-arabic", fonts: g.arabic },
    { label: "text-fonts-other", fonts: g.other },
  ].filter((group) => group.fonts.length > 0);
});

function open() {
  menu.showPopover();
  const r = button.getBoundingClientRect();
  const rtl = document.documentElement.dir === "rtl";
  const width = menu.offsetWidth;
  const left = rtl ? r.right - width : r.left;
  menu.style.left = `${Math.max(8, Math.min(left, innerWidth - width - 8))}px`;
  menu.style.top = `${r.bottom + 6}px`;
  menu.style.maxHeight = `${Math.max(200, innerHeight - r.bottom - 24)}px`;
  query = "";
  search?.focus();
  loadFonts().then((list) => (fonts = list));
}

function choose(f: FontInfo) {
  menu.hidePopover();
  onchange({ family: f.family, bundled: f.bundled });
}
</script>

<button class="font" bind:this={button} onclick={open} aria-haspopup="listbox"
  title={i18n.t("text-font")} aria-label={i18n.t("text-font")}>
  <span class="name" style:font-family={cssFamily(value)}>{value.family}</span>
  <Icon name="chevron-down" />
</button>

<div class="font-menu" popover bind:this={menu}>
  <input bind:this={search} bind:value={query} type="search" dir="auto" placeholder={i18n.t("text-font-search")}
    aria-label={i18n.t("text-font-search")}
    onkeydown={(e) => {
      if (e.key === "Enter") {
        const first = groups[0]?.fonts[0];
        if (first) choose(first);
      }
    }} />
  <div class="list" role="listbox" aria-label={i18n.t("text-font")}>
    {#if fonts === null}
      <p class="empty">…</p>
    {:else if groups.length === 0}
      <p class="empty">{i18n.t("text-font-none")}</p>
    {/if}
    {#each groups as group (group.label)}
      <p class="group">{i18n.t(group.label)}</p>
      {#each group.fonts as f (`${f.bundled}/${f.family}`)}
        <button role="option" aria-selected={f.family === value.family && f.bundled === value.bundled}
          onclick={() => choose(f)}>
          <span class="sample" style:font-family={cssFamily({ family: f.family, bundled: f.bundled })}>{f.family}</span>
          {#if f.arabic}
            <span class="arabic" style:font-family={cssFamily({ family: f.family, bundled: f.bundled })}>أبجد</span>
          {/if}
        </button>
      {/each}
    {/each}
  </div>
</div>

<style>
  .font {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 150px;
    padding-block: 3px;
    padding-inline: 8px 4px;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: start;
  }
  .font :global(svg) {
    flex: none;
    width: 14px;
    height: 14px;
  }
  .font-menu {
    position: fixed;
    margin: 0;
    inset: auto;
    width: 280px;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
    flex-direction: column;
    gap: 6px;
  }
  .font-menu:popover-open {
    display: flex;
  }
  input {
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--canvas);
    color: var(--text);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
  }
  .group {
    margin-block: 8px 2px;
    margin-inline: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
  }
  .list button {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    border: none;
    background: none;
    text-align: start;
    /* Each name is drawn in its own font only when scrolled into view. */
    content-visibility: auto;
    contain-intrinsic-size: auto 30px;
  }
  .list button:hover {
    background: var(--hover);
  }
  .list button[aria-selected="true"] {
    color: var(--accent);
  }
  .sample {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 15px;
  }
  .arabic {
    flex: none;
    font-size: 15px;
    color: var(--muted);
  }
  .empty {
    margin: 8px;
    color: var(--muted);
    font-size: 13px;
  }
</style>
