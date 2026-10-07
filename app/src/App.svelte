<script lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { i18n, languages } from "./lib/i18n.svelte";
import PageCanvas from "./lib/PageCanvas.svelte";
import {
  closeDocument,
  isRivetError,
  type OpenedDocument,
  openDocument,
  type RivetError,
  takeInitialFile,
} from "./lib/pdf";

const ZOOM_STEPS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

let doc = $state<OpenedDocument | null>(null);
let fileName = $state("");
let page = $state(0);
let zoom = $state(1);
let error = $state<RivetError | null>(null);
let viewport: HTMLElement;

let pageCount = $derived(doc?.info.pageCount ?? 0);
let title = $derived(doc?.info.title || fileName);

async function pickFile() {
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (path) await load(path);
}

async function load(path: string) {
  try {
    const opened = await openDocument(path);
    if (doc) await closeDocument(doc.docId);
    doc = opened;
    fileName = path.split(/[\\/]/).at(-1) ?? path;
    page = 0;
    error = null;
  } catch (e) {
    error = isRivetError(e) ? e : { code: "internal", detail: String(e) };
  }
}

function zoomBy(direction: 1 | -1) {
  const index = ZOOM_STEPS.findIndex((z) => z >= zoom - 1e-6);
  zoom = ZOOM_STEPS[Math.min(Math.max(index + direction, 0), ZOOM_STEPS.length - 1)];
}

function onKey(e: KeyboardEvent) {
  if (!doc) return;
  if (e.key === "PageDown") page = Math.min(page + 1, pageCount - 1);
  if (e.key === "PageUp") page = Math.max(page - 1, 0);
  if ((e.ctrlKey || e.metaKey) && (e.key === "=" || e.key === "+")) {
    e.preventDefault();
    zoomBy(1);
  }
  if ((e.ctrlKey || e.metaKey) && e.key === "-") {
    e.preventDefault();
    zoomBy(-1);
  }
}

// Start each page at the top (continuous scrolling replaces this in M1).
$effect(() => {
  void page;
  viewport?.scrollTo({ top: 0 });
});

// Open the file Rivet was launched with, if any.
takeInitialFile().then((path) => {
  if (path) load(path);
});

$effect(() => {
  document.title = title ? `${title} – ${i18n.t("app-name")}` : i18n.t("app-name");
});
</script>

<svelte:window onkeydown={onKey} />

<header class="toolbar">
  <button class="primary" onclick={pickFile}>{i18n.t("open-file")}</button>

  {#if doc}
    <span class="title" title={fileName}>{title}</span>
    <div class="group">
      <button class="icon" onclick={() => page--} disabled={page === 0} aria-label={i18n.t("previous-page")} title={i18n.t("previous-page")}>
        <svg class="flip-rtl" viewBox="0 0 24 24" aria-hidden="true"><path d="M15 18l-6-6 6-6" /></svg>
      </button>
      <span class="status">{i18n.t("page-of-total", { current: page + 1, total: pageCount })}</span>
      <button class="icon" onclick={() => page++} disabled={page >= pageCount - 1} aria-label={i18n.t("next-page")} title={i18n.t("next-page")}>
        <svg class="flip-rtl" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg>
      </button>
    </div>
    <div class="group">
      <button class="icon" onclick={() => zoomBy(-1)} aria-label={i18n.t("zoom-out")} title={i18n.t("zoom-out")}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14" /></svg>
      </button>
      <span class="status zoom">{i18n.t("zoom-level", { percent: Math.round(zoom * 100) })}</span>
      <button class="icon" onclick={() => zoomBy(1)} aria-label={i18n.t("zoom-in")} title={i18n.t("zoom-in")}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
      </button>
    </div>
  {/if}

  <label class="language">
    <span class="visually-hidden">{i18n.t("language")}</span>
    <select bind:value={i18n.locale}>
      {#each languages as lang (lang.code)}
        <option value={lang.code} lang={lang.code}>{lang.name}</option>
      {/each}
    </select>
  </label>
</header>

{#if error}
  <div class="error" role="alert">
    <span>{i18n.t(`error-${error.code}`)}</span>
    <button onclick={() => (error = null)}>{i18n.t("dismiss")}</button>
  </div>
{/if}

<main bind:this={viewport}>
  {#if doc}
    {#key doc.docId}
      <PageCanvas
        docId={doc.docId}
        {page}
        size={doc.info.pageSizes[page]}
        {zoom}
        onerror={(e) => (error = e)}
      />
    {/key}
  {:else}
    <div class="start">
      <h1>{i18n.t("app-name")}</h1>
      <p class="tagline">{i18n.t("app-tagline")}</p>
      <p>{i18n.t("start-title")}</p>
      <p class="hint">{i18n.t("start-hint")}</p>
    </div>
  {/if}
</main>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-block: 8px;
    padding-inline: 12px;
    background: var(--surface);
    border-block-end: 1px solid var(--border);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .status {
    min-width: 7em;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .status.zoom {
    min-width: 4em;
  }
  .language {
    margin-inline-start: auto;
  }
  .error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-block: 8px;
    padding-inline: 16px;
    background: var(--error-bg);
    color: var(--error-fg);
  }
  main {
    flex: 1;
    overflow: auto;
    display: grid;
    place-items: start center;
    padding: 24px;
    background: var(--canvas);
  }
  .start {
    margin-block-start: 15vh;
    text-align: center;
    color: var(--muted);
  }
  .start h1 {
    margin: 0;
    font-size: 2.5rem;
    color: var(--text);
  }
  .tagline {
    margin-block: 4px 32px;
  }
  .hint {
    font-size: 0.9rem;
  }
</style>
