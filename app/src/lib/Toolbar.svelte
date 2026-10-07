<script lang="ts">
import Icon from "./Icon.svelte";
import { i18n, languages } from "./i18n.svelte";
import { ZOOM_STEPS } from "./layout";
import { westernDigits } from "./pageRange";
import type { ZoomMode } from "./recent";
import { settings, type Theme } from "./settings.svelte";
import type { PageLayout, Tab } from "./tabs.svelte";

interface Props {
  tab: Tab | null;
  sidebarOpen: boolean;
  onopen: () => void;
  ontogglesidebar: () => void;
  ongoto: (page: number) => void;
  /** Next (+1) or previous (-1) page, or spread in two-page view. */
  onstep: (direction: 1 | -1) => void;
  onzoomstep: (direction: 1 | -1) => void;
  onzoom: (zoom: number) => void;
  onzoommode: (mode: ZoomMode) => void;
  onrotate: () => void;
  onabout: () => void;
  onsave: () => void;
  onsaveas: () => void;
  onprint: () => void;
  onproperties: () => void;
  onactualsize: () => void;
  onpagelayout: (layout: PageLayout) => void;
  oncontinuous: (continuous: boolean) => void;
  onpagesrtl: (rtl: boolean) => void;
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

// In two-page view the last spread may start one page before the end.
let lastRow = $derived(tab ? tab.page >= tab.info.pageCount - (tab.pageLayout === "double" ? 2 : 1) : true);

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
  <!-- Start (left in English, right in Arabic): files -->
  <div class="zone start">
    <button class="icon" onclick={props.ontogglesidebar} disabled={!tab} aria-pressed={props.sidebarOpen}
      aria-label={i18n.t("toggle-sidebar")} title={i18n.t("toggle-sidebar")}>
      <Icon name="sidebar" />
    </button>
    <button class="primary open" onclick={props.onopen} title={i18n.t("open-file")}>
      <Icon name="folder" />
      <span class="label">{i18n.t("open-file")}</span>
    </button>
    {#if tab}
      <button class="icon" onclick={props.onsave} disabled={!tab.dirty} aria-label={i18n.t("save")} title={i18n.t("save")}>
        <Icon name="save" />
      </button>
      <button class="icon" onclick={props.onprint} aria-label={i18n.t("print")} title={i18n.t("print")}>
        <Icon name="print" />
      </button>
    {/if}
  </div>

  <!-- Center: moving around the document -->
  <div class="zone center">
    {#if tab}
      <div class="group">
        <button class="icon" onclick={() => props.onstep(-1)} disabled={tab.page === 0}
          aria-label={i18n.t("previous-page")} title={i18n.t("previous-page")}>
          <Icon name="chevron-up" />
        </button>
        <form class="page-box" onsubmit={submitPage}>
          <input bind:this={pageInput} bind:value={pageText} inputmode="numeric" aria-label={i18n.t("page-number")}
            onfocus={() => pageInput?.select()} onblur={() => tab && (pageText = String(tab.page + 1))} />
          <span class="total">{i18n.t("of-total", { total: tab.info.pageCount })}</span>
        </form>
        <button class="icon" onclick={() => props.onstep(1)} disabled={lastRow}
          aria-label={i18n.t("next-page")} title={i18n.t("next-page")}>
          <Icon name="chevron-down" />
        </button>
      </div>

      <span class="sep" aria-hidden="true"></span>

      <div class="group segmented-tools" role="radiogroup" aria-label={i18n.t("mouse-mode")}>
        <button class="icon" role="radio" aria-checked={settings.tool === "select"} onclick={() => (settings.tool = "select")}
          aria-label={i18n.t("tool-select")} title={i18n.t("tool-select")}>
          <Icon name="pointer" />
        </button>
        <button class="icon" role="radio" aria-checked={settings.tool === "hand"} onclick={() => (settings.tool = "hand")}
          aria-label={i18n.t("tool-hand")} title={i18n.t("tool-hand")}>
          <Icon name="hand" />
        </button>
      </div>

      <span class="sep" aria-hidden="true"></span>

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
        <button class="icon" onclick={() => props.onzoommode("fit-page")} aria-pressed={tab.zoomMode === "fit-page"}
          aria-label={i18n.t("fit-page")} title={i18n.t("fit-page")}>
          <Icon name="fit-page" />
        </button>
        <button class="icon" onclick={() => props.onzoommode("fit-width")} aria-pressed={tab.zoomMode === "fit-width"}
          aria-label={i18n.t("fit-width")} title={i18n.t("fit-width")}>
          <Icon name="fit-width" />
        </button>
        <button class="icon" onclick={props.onactualsize}
          aria-pressed={tab.zoomMode === "custom" && Math.abs(tab.zoom - 1) < 0.001}
          aria-label={i18n.t("actual-size")} title={i18n.t("actual-size")}>
          <Icon name="actual-size" />
        </button>
      </div>

      <span class="sep" aria-hidden="true"></span>

      <div class="group">
        <button class="icon" popovertarget="view-menu" aria-label={i18n.t("page-display")} title={i18n.t("page-display")}>
          <Icon name={tab.pageLayout === "double" ? "two-pages" : "one-page"} />
        </button>
        <div id="view-menu" class="menu view-menu" popover>
          <div class="menu-section" role="radiogroup" aria-label={i18n.t("page-display")}>
            <span class="menu-label">{i18n.t("page-display")}</span>
            {#each [["single", "one-page", "single-page"], ["double", "two-pages", "two-pages"]] as const as [value, icon, label] (value)}
              <button class="menu-item option" role="radio" aria-checked={tab.pageLayout === value}
                onclick={() => props.onpagelayout(value)}>
                <Icon name={icon} />
                {i18n.t(label)}
              </button>
            {/each}
          </div>
          <label class="menu-item option check">
            <input type="checkbox" checked={tab.continuous} onchange={(e) => props.oncontinuous(e.currentTarget.checked)} />
            {i18n.t("continuous-scrolling")}
          </label>
          <label class="menu-item option check sub" title={i18n.t("pages-rtl-hint")}>
            <input type="checkbox" checked={tab.pagesRtl} onchange={(e) => props.onpagesrtl(e.currentTarget.checked)} />
            {i18n.t("pages-rtl")}
          </label>
        </div>
        <button class="icon" onclick={props.onrotate} aria-label={i18n.t("rotate-view")} title={i18n.t("rotate-view")}>
          <Icon name="rotate" />
        </button>
      </div>
    {/if}
  </div>

  <!-- End (right in English, left in Arabic): app menu -->
  <div class="zone end">
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
      <button class="menu-item" popovertarget="app-menu" popovertargetaction="hide" onclick={props.onproperties}>
        {i18n.t("document-properties")}
      </button>
    {/if}
    <button class="menu-item" popovertarget="app-menu" popovertargetaction="hide" onclick={props.onabout}>
      {i18n.t("about")}
    </button>
  </div>
  </div>
</header>

<style>
  /* Three zones; the middle one stays centred whatever the sides contain. */
  .toolbar {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
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
  .zone {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .zone.end {
    justify-content: flex-end;
  }
  .sep {
    width: 1px;
    height: 22px;
    margin-inline: 4px;
    background: var(--border);
  }
  .open {
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
  /* On narrow windows the Open button shows only its icon. */
  @media (max-width: 1350px) {
    .open .label {
      display: none;
    }
    .open {
      padding-inline: 8px;
    }
  }
  .segmented-tools {
    gap: 0;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .segmented-tools button {
    border: none;
    width: 28px;
    height: 28px;
  }
  .segmented-tools button[aria-checked="true"] {
    background: var(--accent);
    color: var(--accent-text);
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
  /* The view menu opens under its button, not at the toolbar's end. */
  .view-menu {
    inset-inline-end: auto;
    inset-inline-start: 50%;
    min-width: 220px;
  }
  .option {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-block-start: 0;
    padding-block: 6px;
  }
  .option[aria-checked="true"] {
    color: var(--accent);
    font-weight: 600;
  }
  .check {
    cursor: pointer;
    padding-inline: 12px;
    border-block-start: 1px solid var(--border);
    border-radius: 0;
    padding-block-start: 10px;
  }
  .check.sub {
    border-block-start: none;
    padding-block-start: 4px;
  }
  .check input {
    accent-color: var(--accent);
  }
  button.icon[aria-pressed="true"] {
    background: var(--hover);
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
