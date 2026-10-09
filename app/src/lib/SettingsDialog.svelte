<script lang="ts">
// Settings: preferences you set once, grouped in a few sections. Every change
// applies at once and is saved; there's nothing to confirm, only Close.
import { getVersion } from "@tauri-apps/api/app";
import Icon from "./Icon.svelte";
import { i18n, languages } from "./i18n.svelte";
import { dialogOut } from "./motion";
import { userName } from "./pdf";
import { type DefaultZoom, type DocumentWindows, PAGE_TONES, settings, type Theme } from "./settings.svelte";
import type { PageLayout } from "./tabs.svelte";

interface Props {
  onclose: () => void;
  oncheckupdates: () => void;
}
let { onclose, oncheckupdates }: Props = $props();

type Section = "general" | "appearance" | "reading";
const sections: { id: Section; icon: "sliders" | "contrast" | "book" }[] = [
  { id: "general", icon: "sliders" },
  { id: "appearance", icon: "contrast" },
  { id: "reading", icon: "book" },
];
let section = $state<Section>("general");

const themes: Theme[] = ["system", "light", "dark", "black"];
const documentModes: DocumentWindows[] = ["tabs", "windows"];
const layouts: { value: PageLayout; icon: "one-page" | "two-pages"; label: string }[] = [
  { value: "single", icon: "one-page", label: "single-page" },
  { value: "double", icon: "two-pages", label: "two-pages" },
];
const zooms: DefaultZoom[] = ["fit-width", "fit-page", "50", "75", "100", "125", "150", "200"];
// Per-tab taskbar previews are a Windows feature.
const onWindows = /Windows/.test(navigator.userAgent);

let dialog: HTMLDialogElement;
let version = $state("");
// Shown as the placeholder: the name used when none is set.
let osUser = $state("");
userName()
  .then((name) => (osUser = name))
  .catch(() => {});

$effect(() => {
  dialog.showModal();
  // Start on the section list rather than the close button.
  dialog.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
  getVersion()
    .then((v) => (version = v))
    .catch(() => {});
  return () => dialog.close();
});

/** Up and down arrows move between sections (Home and End go to the ends). */
function onNavKey(e: KeyboardEvent) {
  const i = sections.findIndex((s) => s.id === section);
  const next = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: sections.length - 1 }[e.key];
  if (next === undefined) return;
  e.preventDefault();
  section = sections[(next + sections.length) % sections.length].id;
  document.getElementById(`settings-tab-${section}`)?.focus();
}

function zoomLabel(zoom: DefaultZoom): string {
  return zoom === "fit-width" || zoom === "fit-page" ? i18n.t(zoom) : i18n.t("zoom-level", { percent: Number(zoom) });
}
</script>

<dialog
  out:dialogOut|global bind:this={dialog} aria-labelledby="settings-title"
  oncancel={(e) => {
    // Closed by the app (not the browser), so it can fade out.
    e.preventDefault();
    onclose();
  }}>
  <header>
    <h2 id="settings-title">{i18n.t("settings")}</h2>
    <button class="icon" onclick={onclose} aria-label={i18n.t("close")} title={i18n.t("close")}><Icon name="close" /></button>
  </header>

  <div class="body">
    <!-- Focus moves between the tabs themselves (one is in the Tab order). -->
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="nav" role="tablist" aria-orientation="vertical" aria-label={i18n.t("settings")} onkeydown={onNavKey}>
      {#each sections as s (s.id)}
        <button role="tab" id="settings-tab-{s.id}" aria-selected={section === s.id} aria-controls="settings-panel"
          tabindex={section === s.id ? 0 : -1}
          onclick={() => (section = s.id)}>
          <Icon name={s.icon} />
          {i18n.t(`settings-${s.id}`)}
        </button>
      {/each}
    </div>

    <div class="panel" id="settings-panel" role="tabpanel" aria-labelledby="settings-tab-{section}">
      {#if section === "general"}
        <div class="row">
          <label for="settings-language">{i18n.t("language")}</label>
          <select id="settings-language" bind:value={i18n.locale}>
            {#each languages as lang (lang.code)}
              <option value={lang.code} lang={lang.code}>{lang.name}</option>
            {/each}
          </select>
        </div>

        <div class="row">
          <span id="settings-documents">{i18n.t("open-documents-in")}</span>
          <div class="segmented" role="radiogroup" aria-labelledby="settings-documents">
            {#each documentModes as mode (mode)}
              <button role="radio" aria-checked={settings.documentWindows === mode}
                onclick={() => (settings.documentWindows = mode)}>
                {i18n.t(`documents-${mode}`)}
              </button>
            {/each}
          </div>
        </div>
        {#if onWindows}
          <label class="check">
            <input type="checkbox" bind:checked={settings.taskbarTabs} disabled={settings.documentWindows !== "tabs"} />
            {i18n.t("taskbar-tabs")}
          </label>
        {/if}

        <div class="row">
          <label for="settings-author">{i18n.t("author-name")}</label>
          <input id="settings-author" class="author" dir="auto" bind:value={settings.author} placeholder={osUser} />
        </div>
        <p class="hint">{i18n.t("author-name-hint")}</p>

        <h3>{i18n.t("updates")}</h3>
        <label class="check">
          <input type="checkbox" bind:checked={settings.autoUpdate} />
          {i18n.t("auto-update")}
        </label>
        <div class="row">
          <span class="muted">{version ? i18n.t("version", { version }) : ""}</span>
          <button onclick={oncheckupdates}>{i18n.t("check-now")}</button>
        </div>
      {:else if section === "appearance"}
        <div class="row">
          <span id="settings-theme">{i18n.t("theme")}</span>
          <div class="segmented" role="radiogroup" aria-labelledby="settings-theme">
            {#each themes as theme (theme)}
              <button role="radio" aria-checked={settings.theme === theme} onclick={() => (settings.theme = theme)}>
                {i18n.t(`theme-${theme}`)}
              </button>
            {/each}
          </div>
        </div>

        <div class="stack">
          <span id="settings-tone">{i18n.t("page-tone")}</span>
          <p class="hint">{i18n.t("page-tone-hint")}</p>
          <div class="tones" role="radiogroup" aria-labelledby="settings-tone">
            {#each PAGE_TONES as tone (tone.value)}
              <button class="tone" role="radio" aria-checked={settings.pageTone === tone.value}
                onclick={() => (settings.pageTone = tone.value)}>
                <span class="swatch" style:background={tone.swatch}></span>
                <span>{i18n.t(`page-tone-${tone.value}`)}</span>
              </button>
            {/each}
          </div>
        </div>
        <label class="check">
          <input type="checkbox" bind:checked={settings.animations} />
          {i18n.t("animations")}
        </label>
      {:else}
        <p class="hint">{i18n.t("reading-defaults-hint")}</p>

        <div class="row">
          <label for="settings-zoom">{i18n.t("zoom")}</label>
          <select id="settings-zoom" bind:value={settings.defaultZoom}>
            {#each zooms as zoom (zoom)}
              <option value={zoom}>{zoomLabel(zoom)}</option>
            {/each}
          </select>
        </div>

        <div class="row">
          <span id="settings-layout">{i18n.t("page-display")}</span>
          <div class="segmented" role="radiogroup" aria-labelledby="settings-layout">
            {#each layouts as layout (layout.value)}
              <button role="radio" aria-checked={settings.pageLayout === layout.value}
                onclick={() => (settings.pageLayout = layout.value)}>
                <Icon name={layout.icon} />
                {i18n.t(layout.label)}
              </button>
            {/each}
          </div>
        </div>

        <label class="check">
          <input type="checkbox" bind:checked={settings.continuous} />
          {i18n.t("continuous-scrolling")}
        </label>
      {/if}
    </div>
  </div>
</dialog>

<style>
  dialog {
    width: min(680px, calc(100vw - 32px));
    height: min(460px, calc(100vh - 32px));
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    flex-direction: column;
  }
  dialog[open] {
    display: flex;
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-block: 12px;
    padding-inline: 20px 12px;
    border-block-end: 1px solid var(--border);
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  header button {
    border: none;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  /* The section list sits at the start: left in English, right in Arabic. */
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 180px;
    flex: none;
    padding: 10px;
    border-inline-end: 1px solid var(--border);
    background: var(--chrome);
  }
  .nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    border: none;
    background: transparent;
    text-align: start;
    padding-block: 7px;
    padding-inline: 10px;
  }
  .nav button:hover {
    background: var(--hover);
  }
  .nav button[aria-selected="true"] {
    background: var(--hover);
    color: var(--accent);
    font-weight: 600;
  }
  .panel {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding-block: 12px 20px;
    padding-inline: 24px;
  }
  h3 {
    margin-block: 24px 4px;
    font-size: 13px;
    color: var(--muted);
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 8px 16px;
    padding-block: 10px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-block: 10px;
  }
  .row select,
  .author {
    min-width: 12em;
  }
  .hint,
  .muted {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
  }
  .panel > .hint {
    margin-block: 6px 4px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-block: 8px;
    cursor: pointer;
  }
  .check:has(input:disabled) {
    color: var(--muted);
    cursor: default;
  }
  .check input {
    accent-color: var(--accent);
  }
  .segmented {
    display: flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .segmented button {
    display: flex;
    align-items: center;
    gap: 6px;
    border: none;
    padding-block: 4px;
    padding-inline: 12px;
    font-size: 13px;
  }
  .segmented button[aria-checked="true"] {
    font-weight: 600;
    background: var(--accent);
    color: var(--accent-text);
  }
  .tones {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-block-start: 4px;
  }
  .tone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    min-width: 64px;
    padding: 8px 4px;
    border: none;
    font-size: 12px;
    color: var(--muted);
  }
  .swatch {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 1px solid var(--border);
    box-shadow: inset 0 0 0 2px var(--surface);
  }
  .tone[aria-checked="true"] {
    color: var(--accent);
    font-weight: 600;
  }
  .tone[aria-checked="true"] .swatch {
    border: 2px solid var(--accent);
  }
  @media (max-width: 560px) {
    .nav {
      width: auto;
    }
    .nav button {
      font-size: 0;
      gap: 0;
    }
  }
</style>
