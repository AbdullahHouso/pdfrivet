<script lang="ts">
// Extract pages: which pages to take out into a new file, and whether to
// delete them from this document afterwards.
import { untrack } from "svelte";
import { i18n } from "../i18n.svelte";
import { parsePageRange } from "../pageRange";
import ToolDialog from "./ToolDialog.svelte";

interface Props {
  pageCount: number;
  /** Pre-filled range (1-based text), e.g. the current page. */
  initial: string;
  onrun: (pages: number[], deleteAfter: boolean) => void;
  oncancel: () => void;
}
let { pageCount, initial, onrun, oncancel }: Props = $props();

// Starts from the given range; the user edits it from there.
let range = $state(untrack(() => initial));
let deleteAfter = $state(false);
let pages = $derived(parsePageRange(range, pageCount));
// Deleting every page would leave an empty document.
let canDelete = $derived(!!pages && pages.length < pageCount);

function run() {
  if (pages) onrun(pages, deleteAfter && canDelete);
}
</script>

<ToolDialog title={i18n.t("extract-title")} icon="extract" {oncancel} onsubmit={run}>
  <label class="field">
    {i18n.t("extract-pages-label")}
    <!-- svelte-ignore a11y_autofocus -->
    <input type="text" bind:value={range} autofocus aria-invalid={range.trim() !== "" && !pages}
      placeholder={i18n.t("extract-pages-hint")} dir="ltr" />
    <span class="hint">{i18n.t("extract-pages-hint")}</span>
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={deleteAfter} disabled={!canDelete} />
    {i18n.t("extract-delete-after")}
  </label>
  {#snippet actions()}
    <button type="button" onclick={oncancel}>{i18n.t("cancel")}</button>
    <button type="submit" class="primary" disabled={!pages}>{i18n.t("extract-run")}</button>
  {/snippet}
</ToolDialog>
