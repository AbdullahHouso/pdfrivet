<script lang="ts">
// PDFRivet's print dialog: printer, copies, pages, sizing, paper and a live
// preview, all in one place, then native printing (rivet-core/src/print).
// "Use system dialog" keeps the browser-style printing as a fallback.
import { untrack } from "svelte";
import type { Duplex } from "./bindings/Duplex";
import type { Orientation } from "./bindings/Orientation";
import type { PageSize } from "./bindings/PageSize";
import type { Placement } from "./bindings/Placement";
import type { PrinterInfo } from "./bindings/PrinterInfo";
import type { PrintSettings } from "./bindings/PrintSettings";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import PageView from "./PageView.svelte";
import {
  choosePages,
  listPrinters,
  type PageChoice,
  type PrinterProperties,
  printerProperties,
  printPlacement,
  type Subset,
} from "./printing";
import { settings } from "./settings.svelte";

interface Props {
  docId: number;
  title: string;
  pageSizes: PageSize[];
  /** The page you're on (0-based). */
  current: number;
  /** Native printing with these settings (and driver settings, on Windows). */
  onprint: (settings: PrintSettings, printerSettings: number[] | null) => void;
  /** Fallback: the system (webview) print dialog for these pages. */
  onsystemprint: (pages: number[]) => void;
  oncancel: () => void;
}
let { docId, title, pageSizes, current, onprint, onsystemprint, oncancel }: Props = $props();

const pageCount = untrack(() => pageSizes.length);
const MANY_PAGES = 100;
const saved = settings.printPrefs;

let dialog: HTMLDialogElement;
let printers = $state<PrinterInfo[] | null>(null);
let printerName = $state("");
let copies = $state(1);
let collate = $state(true);
let choice = $state<PageChoice>(pageCount <= MANY_PAGES ? "all" : "current");
let rangeText = $state("");
let subset = $state<Subset>("all");
let reverse = $state(false);
let scalingKind = $state<"fit" | "actual" | "shrink" | "custom">(saved.scaling ?? "shrink");
let percent = $state(100);
let orientation = $state<Orientation>(saved.orientation ?? "auto");
let paperId = $state<string | null>(null);
let duplex = $state<Duplex>(saved.duplex ?? "oneSided");
let grayscale = $state(saved.grayscale ?? false);
/** Driver settings from "Printer properties…" (Windows), passed to the job. */
let devmode = $state<number[] | null>(null);

let printer = $derived(printers?.find((p) => p.name === printerName) ?? null);
let pages = $derived(choosePages(choice, current, pageCount, rangeText, subset, reverse));
let rangeInvalid = $derived(choice === "custom" && rangeText.trim() !== "" && pages === null);
let paper = $derived(printer?.papers.find((p) => p.id === paperId) ?? printer?.papers[0] ?? null);
let scaling = $derived(
  scalingKind === "custom" ? { kind: "custom" as const, percent: Math.max(1, percent) } : { kind: scalingKind },
);

$effect(() => {
  dialog.showModal();
  listPrinters()
    .then((list) => {
      printers = list;
      const pick = list.find((p) => p.name === saved.printer) ?? list.find((p) => p.isDefault) ?? list[0];
      if (pick) selectPrinter(pick.name);
    })
    .catch(() => (printers = []));
  return () => dialog.close();
});

function selectPrinter(name: string) {
  printerName = name;
  devmode = null;
  const p = printers?.find((x) => x.name === name);
  paperId = p?.defaultPaper ?? p?.papers[0]?.id ?? null;
  if (p && !p.duplex) duplex = "oneSided";
}

async function openProperties() {
  if (!printer) return;
  const result: PrinterProperties | null = await printerProperties(printer.name, devmode).catch(() => null);
  if (!result) return;
  devmode = result.devmode;
  copies = result.copies;
  duplex = result.duplex;
  grayscale = result.grayscale;
  if (result.paper) paperId = result.paper;
}

// Preview: which of the chosen pages is shown, and where it lands on the paper.
let previewIndex = $state(0);
let placement = $state<Placement | null>(null);
let previewPage = $derived(pages && pages.length > 0 ? pages[Math.min(previewIndex, pages.length - 1)] : null);

$effect(() => {
  if (pages && previewIndex >= pages.length) previewIndex = 0;
});

$effect(() => {
  const page = previewPage;
  const size = page === null ? null : pageSizes[page];
  const paperMm = paper ? { width: paper.widthMm, height: paper.heightMm } : { width: 210, height: 297 };
  if (!size) return;
  printPlacement(size, paperMm, scaling, orientation)
    .then((p) => (placement = p))
    .catch(() => {});
});

// The preview sheet fits a fixed box.
const BOX = { width: 300, height: 380 };
let sheetScale = $derived(
  placement ? Math.min(BOX.width / placement.sheetWidth, BOX.height / placement.sheetHeight) : 1,
);

function submit(e: Event) {
  e.preventDefault();
  if (!printer || !pages || pages.length === 0) return;
  settings.printPrefs = { printer: printer.name, scaling: scalingKind, orientation, duplex, grayscale };
  onprint(
    {
      printer: printer.name,
      pages,
      copies: Math.max(1, Math.round(copies)),
      collate,
      scaling,
      orientation,
      duplex,
      grayscale,
      paper: paper?.id ?? null,
      title,
    },
    devmode,
  );
}

function sheetsLabel(): string {
  if (!pages) return "";
  const perCopy = duplex === "oneSided" ? pages.length : Math.ceil(pages.length / 2);
  return i18n.t("print-sheets", { count: perCopy * Math.max(1, copies) });
}
</script>

<dialog bind:this={dialog} aria-labelledby="print-title" oncancel={(e) => { e.preventDefault(); oncancel(); }}>
  <form onsubmit={submit}>
    <header class="head">
      <Icon name="print" />
      <h2 id="print-title">{i18n.t("print-title")}</h2>
    </header>

    <div class="body">
      <div class="settings">
        <!-- Printer -->
        <section>
          <div class="row">
            <label for="print-printer">{i18n.t("printer")}</label>
            {#if printers === null}
              <span class="muted">{i18n.t("printers-loading")}</span>
            {:else if printers.length === 0}
              <span class="muted">{i18n.t("printers-none")}</span>
            {:else}
              <select id="print-printer" value={printerName} onchange={(e) => selectPrinter(e.currentTarget.value)}>
                {#each printers as p (p.name)}
                  <option value={p.name}>{p.name}{p.isDefault ? ` (${i18n.t("printer-default")})` : ""}</option>
                {/each}
              </select>
              {#if printer?.hasProperties}
                <button type="button" onclick={openProperties}>{i18n.t("printer-properties")}</button>
              {/if}
            {/if}
          </div>
          <div class="row">
            <label for="print-copies">{i18n.t("copies")}</label>
            <input id="print-copies" class="number" type="number" min="1" max="999" bind:value={copies} />
            <label class="inline"><input type="checkbox" bind:checked={collate} disabled={copies < 2} /> {i18n.t("collate")}</label>
            {#if printer?.color !== false}
              <label class="inline"><input type="checkbox" bind:checked={grayscale} /> {i18n.t("grayscale")}</label>
            {/if}
          </div>
        </section>

        <!-- Pages -->
        <fieldset>
          <legend>{i18n.t("print-pages")}</legend>
          <label><input type="radio" bind:group={choice} value="all" /> {i18n.t("print-all-pages", { count: pageCount })}</label>
          <label><input type="radio" bind:group={choice} value="current" /> {i18n.t("print-current-page", { page: current + 1 })}</label>
          <label class="custom">
            <input type="radio" bind:group={choice} value="custom" />
            {i18n.t("print-custom-pages")}
            <input class="range" type="text" dir="auto" placeholder={i18n.t("print-range-example")} bind:value={rangeText}
              onfocus={() => (choice = "custom")} aria-invalid={rangeInvalid} />
          </label>
          {#if rangeInvalid}<p class="error">{i18n.t("print-range-invalid", { count: pageCount })}</p>{/if}
          <div class="row">
            <label for="print-subset">{i18n.t("print-subset")}</label>
            <select id="print-subset" bind:value={subset}>
              <option value="all">{i18n.t("subset-all")}</option>
              <option value="odd">{i18n.t("subset-odd")}</option>
              <option value="even">{i18n.t("subset-even")}</option>
            </select>
            <label class="inline"><input type="checkbox" bind:checked={reverse} /> {i18n.t("print-reverse")}</label>
          </div>
        </fieldset>

        <!-- Page sizing -->
        <fieldset>
          <legend>{i18n.t("page-sizing")}</legend>
          <div class="segmented" role="radiogroup" aria-label={i18n.t("page-sizing")}>
            {#each [["fit", "sizing-fit"], ["shrink", "sizing-shrink"], ["actual", "sizing-actual"], ["custom", "sizing-custom"]] as const as [value, label] (value)}
              <button type="button" role="radio" aria-checked={scalingKind === value} onclick={() => (scalingKind = value)}>
                {i18n.t(label)}
              </button>
            {/each}
          </div>
          {#if scalingKind === "custom"}
            <div class="row">
              <input class="number" type="number" min="1" max="400" bind:value={percent} aria-label={i18n.t("sizing-custom")} />
              <span>%</span>
            </div>
          {/if}
        </fieldset>

        <!-- Paper -->
        <fieldset>
          <legend>{i18n.t("paper")}</legend>
          <div class="row">
            <label for="print-paper">{i18n.t("paper-size")}</label>
            <select id="print-paper" bind:value={paperId} disabled={!printer}>
              {#each printer?.papers ?? [] as p (p.id)}
                <option value={p.id}>{p.name}</option>
              {/each}
            </select>
          </div>
          <div class="row">
            <label for="print-orientation">{i18n.t("orientation")}</label>
            <select id="print-orientation" bind:value={orientation}>
              <option value="auto">{i18n.t("orientation-auto")}</option>
              <option value="portrait">{i18n.t("orientation-portrait")}</option>
              <option value="landscape">{i18n.t("orientation-landscape")}</option>
            </select>
          </div>
          {#if printer?.duplex}
            <div class="row">
              <label for="print-duplex">{i18n.t("two-sided")}</label>
              <select id="print-duplex" bind:value={duplex}>
                <option value="oneSided">{i18n.t("duplex-off")}</option>
                <option value="longEdge">{i18n.t("duplex-long")}</option>
                <option value="shortEdge">{i18n.t("duplex-short")}</option>
              </select>
            </div>
          {/if}
        </fieldset>
      </div>

      <!-- Preview -->
      <section class="preview" aria-label={i18n.t("print-preview")}>
        <div class="box" style:width="{BOX.width}px" style:height="{BOX.height}px">
          {#if placement && previewPage !== null}
            <div
              class="sheet"
              style:width="{placement.sheetWidth * sheetScale}px"
              style:height="{placement.sheetHeight * sheetScale}px"
            >
              <div
                class="placed"
                class:gray={grayscale}
                style:left="{placement.x * sheetScale}px"
                style:top="{placement.y * sheetScale}px"
                style:width="{placement.width * sheetScale}px"
                style:height="{placement.height * sheetScale}px"
              >
                {#key previewPage}
                  <PageView
                    {docId}
                    index={previewPage}
                    width={placement.width * sheetScale}
                    height={placement.height * sheetScale}
                    widthPt={pageSizes[previewPage].width}
                    rotation={0}
                    thumbnail
                  />
                {/key}
              </div>
            </div>
          {/if}
        </div>
        {#if pages && pages.length > 0}
          <div class="pager">
            <button type="button" class="icon" disabled={previewIndex === 0} onclick={() => previewIndex--}
              aria-label={i18n.t("previous-page")}>
              <Icon name="chevron-up" />
            </button>
            <span>{i18n.t("print-preview-position", { index: previewIndex + 1, total: pages.length })}</span>
            <button type="button" class="icon" disabled={previewIndex >= pages.length - 1} onclick={() => previewIndex++}
              aria-label={i18n.t("next-page")}>
              <Icon name="chevron-down" />
            </button>
          </div>
          <p class="muted small">
            {i18n.t("print-scale", { percent: Math.round((placement?.scale ?? 1) * 100) })} · {sheetsLabel()}
          </p>
        {/if}
      </section>
    </div>

    <footer class="actions">
      <button type="button" class="link" disabled={!pages || pages.length === 0}
        onclick={() => pages && onsystemprint(pages)}>
        {i18n.t("print-system-dialog")}
      </button>
      <span class="grow"></span>
      <button type="button" onclick={oncancel}>{i18n.t("cancel")}</button>
      <button type="submit" class="primary" disabled={!printer || !pages || pages.length === 0}>{i18n.t("print-button")}</button>
    </footer>
  </form>
</dialog>

<style>
  dialog {
    width: min(860px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.25);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  form {
    display: flex;
    flex-direction: column;
    max-height: calc(100vh - 34px);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-block: 16px 8px;
    padding-inline: 20px;
    color: var(--accent);
  }
  h2 {
    margin: 0;
    font-size: 17px;
    color: var(--text);
  }
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 20px;
    padding-inline: 20px;
    overflow: auto;
  }
  @media (max-width: 760px) {
    .body {
      grid-template-columns: 1fr;
    }
  }
  .settings {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
  section,
  fieldset {
    margin: 0;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  legend {
    padding-inline: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .row > label:first-child {
    min-width: 6.5em;
    color: var(--muted);
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .row select {
    flex: 1;
    min-width: 0;
  }
  input[type="radio"],
  input[type="checkbox"] {
    accent-color: var(--accent);
  }
  .number,
  .range {
    font: inherit;
    color: inherit;
    background: var(--field);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding-block: 5px;
    padding-inline: 8px;
  }
  .number {
    width: 5em;
  }
  .range {
    flex: 1;
    min-width: 0;
  }
  .range[aria-invalid="true"] {
    border-color: var(--error-fg);
  }
  .error {
    margin: 0;
    color: var(--error-fg);
    font-size: 13px;
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
    padding-block: 5px;
    font-size: 13px;
  }
  .segmented button[aria-checked="true"] {
    background: var(--accent);
    color: var(--accent-text);
  }
  .preview {
    align-items: center;
    background: var(--canvas);
  }
  .box {
    display: grid;
    place-items: center;
  }
  .sheet {
    position: relative;
    background: white;
    box-shadow: var(--shadow-page);
    overflow: hidden;
  }
  .placed {
    position: absolute;
  }
  .placed :global(.page) {
    box-shadow: none;
  }
  .gray {
    filter: grayscale(1);
  }
  .pager {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    margin: 0;
    font-size: 12px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 20px;
  }
  .grow {
    flex: 1;
  }
  .link {
    border: none;
    background: none;
    color: var(--accent);
    padding-inline: 0;
  }
</style>
