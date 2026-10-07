<script lang="ts">
// Asks which pages to print before anything is prepared, so printing one page
// of a 1,440-page document is instant. Printer, copies and paper are chosen
// afterwards in the system print dialog.
import { untrack } from "svelte";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { parsePageRange } from "./pageRange";

interface Props {
  pageCount: number;
  /** The page you're on (0-based). */
  current: number;
  onprint: (pages: number[]) => void;
  oncancel: () => void;
}
let { pageCount, current, onprint, oncancel }: Props = $props();

/** Above this many pages we mention that preparing takes a while. */
const MANY_PAGES = 100;

let dialog: HTMLDialogElement;
// Small documents default to all pages, long ones to the page you're on.
let choice = $state<"all" | "current" | "custom">(untrack(() => pageCount) <= MANY_PAGES ? "all" : "current");
let rangeText = $state("");
let rangeInput: HTMLInputElement | undefined = $state();

let pages = $derived.by((): number[] | null => {
  if (choice === "all") return Array.from({ length: pageCount }, (_, i) => i);
  if (choice === "current") return [current];
  return parsePageRange(rangeText, pageCount);
});
let invalid = $derived(choice === "custom" && rangeText.trim() !== "" && pages === null);

$effect(() => {
  dialog.showModal();
  return () => dialog.close();
});

function submit(e: Event) {
  e.preventDefault();
  if (pages && pages.length > 0) onprint(pages);
}
</script>

<dialog bind:this={dialog} aria-labelledby="print-title" oncancel={(e) => { e.preventDefault(); oncancel(); }}>
  <form onsubmit={submit}>
    <div class="head">
      <Icon name="print" />
      <h2 id="print-title">{i18n.t("print-title")}</h2>
    </div>

    <fieldset>
      <legend>{i18n.t("print-pages")}</legend>
      <label>
        <input type="radio" bind:group={choice} value="current" />
        {i18n.t("print-current-page", { page: current + 1 })}
      </label>
      <label>
        <input type="radio" bind:group={choice} value="all" />
        {i18n.t("print-all-pages", { count: pageCount })}
      </label>
      <label class="custom">
        <input type="radio" bind:group={choice} value="custom" onchange={() => rangeInput?.focus()} />
        {i18n.t("print-custom-pages")}
        <input
          bind:this={rangeInput}
          class="range"
          type="text"
          dir="ltr"
          placeholder={i18n.t("print-range-example")}
          bind:value={rangeText}
          onfocus={() => (choice = "custom")}
          aria-invalid={invalid}
          aria-describedby="print-hint"
        />
      </label>
    </fieldset>

    <p id="print-hint" class="hint" class:error={invalid} aria-live="polite">
      {#if invalid}
        {i18n.t("print-range-invalid", { count: pageCount })}
      {:else if pages && pages.length > MANY_PAGES}
        {i18n.t("print-many-pages", { count: pages.length })}
      {:else if pages}
        {i18n.t("print-page-count", { count: pages.length })}
      {/if}
    </p>

    <div class="actions">
      <button type="button" onclick={oncancel}>{i18n.t("cancel")}</button>
      <button type="submit" class="primary" disabled={!pages || pages.length === 0}>{i18n.t("print-continue")}</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    width: min(420px, calc(100vw - 32px));
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.25);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--accent);
  }
  h2 {
    margin: 0;
    font-size: 17px;
    color: var(--text);
  }
  fieldset {
    border: none;
    margin-block: 16px 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  legend {
    font-size: 12px;
    color: var(--muted);
    margin-block-end: 6px;
    padding: 0;
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  input[type="radio"] {
    accent-color: var(--accent);
  }
  .range {
    flex: 1;
    min-width: 0;
    font: inherit;
    color: inherit;
    background: var(--canvas);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding-block: 5px;
    padding-inline: 8px;
  }
  .range[aria-invalid="true"] {
    border-color: var(--error-fg);
  }
  .hint {
    min-height: 1.4em;
    margin-block: 12px 0;
    font-size: 13px;
    color: var(--muted);
  }
  .hint.error {
    color: var(--error-fg);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-block-start: 16px;
  }
</style>
