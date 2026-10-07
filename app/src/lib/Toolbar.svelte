<script lang="ts">
import Icon from "./Icon.svelte";
import { i18n, languages } from "./i18n.svelte";
import { ZOOM_STEPS } from "./layout";
import type { ZoomMode } from "./recent";
import { settings, type Theme } from "./settings.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab | null;
  sidebarOpen: boolean;
  onopen: () => void;
  ontogglesidebar: () => void;
  ongoto: (page: number) => void;
  onzoomstep: (direction: 1 | -1) => void;
  onzoom: (zoom: number) => void;
  onzoommode: (mode: ZoomMode) => void;
  onrotate: () => void;
  onabout: () => void;
  onsave: () => void;
  onsaveas: () => void;
}
let props: Props = $props();
let tab = $derived(props.tab);

let pageInput: HTMLInputElement | undefined = $state();
let pageText = $state("");

// Show the current page unless the user is typing in the box.
$effect(() => {
  const page = tab ? tab.page + 1 : 0;
  if (document.activeElement !== pageInput) pageText = String(page);
});

/** Lets other parts of the app focus the page box (Ctrl+G). */
export function focusPageInput() {
  pageInput?.focus();
  pageInput?.select();
}

// Accept Arabic-Indic and Persian digits too.
function parseNumber(text: string): number {
  const western = text.replace(/[٠-٩۰-۹]/g, (d) => String((d.charCodeAt(0) & 0xf) % 10));
  return Number.parseInt(western, 10);
}

function submitPage(e: Event) {
  e.preventDefault();
  if (!tab) return;
  const page = parseNumber(pageText);
  if (Number.isFinite(page)) props.ongoto(page - 1);
  pageInput?.blur();
}

let zoomValue = $derived(tab ? (tab.zoomMode === "custom" ? String(Math.round(tab.zoom * 100)) : tab.zoomMode) : "");
let zoomPercents = $derived.by(() => {
  const list = ZOOM_STEPS.map((z) => Math.round(z * 100));
  const current = tab ? Math.round(tab.zoom * 100) : 100;
  return list.includes(current) ? list : [...list, current].sort((a, b) => a - b);
});

function onZoomSelect(e: Event) {
  const value = (e.currentTarget as HTMLSelectElement).value;
  if (value === "fit-width" || value === "fit-page") props.onzoommode(value);
  else props.onzoom(Number(value) / 100);
}

const themes: Theme[] = ["system", "light", "dark"];
</script>

<header class="toolbar">
  <button class="icon" onclick={props.ontogglesidebar} disabled={!tab} aria-pressed={props.sidebarOpen}
    aria-label={i18n.t("toggle-sidebar")} title={i18n.t("toggle-sidebar")}>
    <Icon name="sidebar" />
  </button>
  <button class="primary" onclick={props.onopen}>{i18n.t("open-file")}</button>
  {#if tab}
    <button class="icon" onclick={props.onsave} disabled={!tab.dirty} aria-label={i18n.t("save")} title={i18n.t("save")}>
      <Icon name="save" />
    </button>
  {/if}

  {#if tab}
    <div class="group">
      <button class="icon" onclick={() => tab && props.ongoto(tab.page - 1)} disabled={tab.page === 0}
        aria-label={i18n.t("previous-page")} title={i18n.t("previous-page")}>
        <Icon name="chevron-up" />
      </button>
      <form class="page-box" onsubmit={submitPage}>
        <input bind:this={pageInput} bind:value={pageText} inputmode="numeric" aria-label={i18n.t("page-number")}
          onfocus={() => pageInput?.select()} onblur={() => tab && (pageText = String(tab.page + 1))} />
        <span class="total">{i18n.t("of-total", { total: tab.info.pageCount })}</span>
      </form>
      <button class="icon" onclick={() => tab && props.ongoto(tab.page + 1)} disabled={tab.page >= tab.info.pageCount - 1}
        aria-label={i18n.t("next-page")} title={i18n.t("next-page")}>
        <Icon name="chevron-down" />
      </button>
    </div>

    <div class="group">
      <button class="icon" onclick={() => props.onzoomstep(-1)} aria-label={i18n.t("zoom-out")} title={i18n.t("zoom-out")}>
        <Icon name="minus" />
      </button>
      <select class="zoom" value={zoomValue} onchange={onZoomSelect} aria-label={i18n.t("zoom")}>
        <option value="fit-width">{i18n.t("fit-width")}</option>
        <option value="fit-page">{i18n.t("fit-page")}</option>
        <hr />
        {#each zoomPercents as percent (percent)}
          <option value={String(percent)}>{i18n.t("zoom-level", { percent })}</option>
        {/each}
      </select>
      <button class="icon" onclick={() => props.onzoomstep(1)} aria-label={i18n.t("zoom-in")} title={i18n.t("zoom-in")}>
        <Icon name="plus" />
      </button>
    </div>

    <button class="icon" onclick={props.onrotate} aria-label={i18n.t("rotate-view")} title={i18n.t("rotate-view")}>
      <Icon name="rotate" />
    </button>
  {/if}

  <span class="spacer"></span>

  <button class="icon" popovertarget="app-menu" aria-label={i18n.t("menu")} title={i18n.t("menu")}>
    <Icon name="menu" />
  </button>
  <div id="app-menu" class="menu" popover>
    <div class="menu-section">
      <span class="menu-label">{i18n.t("theme")}</span>
      <div class="segmented" role="radiogroup" aria-label={i18n.t("theme")}>
        {#each themes as theme (theme)}
          <button role="radio" aria-checked={settings.theme === theme} onclick={() => (settings.theme = theme)}>
            {i18n.t(`theme-${theme}`)}
          </button>
        {/each}
      </div>
    </div>
    <label class="menu-section">
      <span class="menu-label">{i18n.t("language")}</span>
      <select bind:value={i18n.locale}>
        {#each languages as lang (lang.code)}
          <option value={lang.code} lang={lang.code}>{lang.name}</option>
        {/each}
      </select>
    </label>
    {#if tab}
      <button class="menu-item" popovertarget="app-menu" popovertargetaction="hide" onclick={props.onsaveas}>
        {i18n.t("save-as")}
      </button>
    {/if}
    <button class="menu-item" popovertarget="app-menu" popovertargetaction="hide" onclick={props.onabout}>
      {i18n.t("about")}
    </button>
  </div>
</header>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-block: 6px;
    padding-inline: 8px;
    background: var(--surface);
    border-block-end: 1px solid var(--border);
  }
  .group {
    display: flex;
    align-items: center;
    /* Room for the focus ring, so it never overlaps the next button. */
    gap: 6px;
  }
  .spacer {
    flex: 1;
  }
  .page-box {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-inline: 4px;
  }
  .page-box input {
    width: 4em;
    text-align: center;
    font: inherit;
    color: inherit;
    background: var(--canvas);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding-block: 4px;
  }
  .total {
    color: var(--muted);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .zoom {
    padding-inline: 8px;
    min-width: 7.5em;
  }
  .menu {
    /* Opens under the menu button, at the toolbar's end (mirrors in RTL). */
    position: fixed;
    margin: 0;
    inset: auto;
    inset-block-start: 46px;
    inset-inline-end: 8px;
    min-width: 240px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
  }
  .menu-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
  }
  .menu-label {
    font-size: 12px;
    color: var(--muted);
  }
  .segmented {
    display: flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .segmented button {
    flex: 1;
    border: none;
    padding-block: 4px;
    padding-inline: 6px;
    font-size: 13px;
  }
  .segmented button[aria-checked="true"] {
    background: var(--accent);
    color: var(--accent-text);
  }
  .menu-item {
    width: 100%;
    text-align: start;
    border: none;
    margin-block-start: 4px;
  }
</style>
