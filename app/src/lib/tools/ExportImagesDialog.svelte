<script lang="ts">
// Export as images: pages saved as PNG or JPEG files at a chosen resolution,
// one at a time (with progress; Stop ends after the current page).
import { open } from "@tauri-apps/plugin-dialog";
import { untrack } from "svelte";
import type { ImageFormat } from "../bindings/ImageFormat";
import { i18n } from "../i18n.svelte";
import { parsePageRange } from "../pageRange";
import { exportPageImage, type RivetError, toRivetError } from "../pdf";
import type { Tab } from "../tabs.svelte";
import { folderOf, freePaths, pageImageNames, separatorOf } from "./files";
import ToolDialog from "./ToolDialog.svelte";

interface Props {
  tab: Tab;
  onfinished: (count: number, first: string) => void;
  onerror: (e: RivetError) => void;
  oncancel: () => void;
}
let { tab, onfinished, onerror, oncancel }: Props = $props();

let range = $state("");
let format = $state<ImageFormat>("png");
let quality = $state(85);
let dpi = $state(150);
let folder = $state(untrack(() => folderOf(tab.path)));
let progress = $state<number | null>(null);
let stopping = false;

let pages = $derived(
  range.trim() === ""
    ? Array.from({ length: tab.info.pageCount }, (_, i) => i)
    : parsePageRange(range, tab.info.pageCount),
);

async function chooseFolder() {
  const picked = await open({ directory: true, multiple: false, defaultPath: folder || undefined });
  if (typeof picked === "string") folder = picked;
}

async function run() {
  if (!pages || !folder || progress !== null) return;
  progress = 0;
  stopping = false;
  try {
    const base = tab.fileName.replace(/\.pdf$/i, "");
    const ext = format === "png" ? "png" : "jpg";
    const paths = await freePaths(folder, pageImageNames(base, pages, tab.info.pageCount, ext), separatorOf(tab.path));
    let done = 0;
    for (const [i, page] of pages.entries()) {
      if (stopping) break;
      await exportPageImage(tab.docId, page, dpi, format, quality, paths[i]);
      done++;
      progress = done / pages.length;
    }
    if (done > 0) onfinished(done, paths[0]);
  } catch (e) {
    onerror(toRivetError(e));
  } finally {
    progress = null;
  }
}
</script>

<ToolDialog title={i18n.t("export-title")} icon="image" oncancel={() => (progress === null ? oncancel() : (stopping = true))}
  onsubmit={run} {progress}>
  <fieldset disabled={progress !== null}>
    <label class="field">
      {i18n.t("extract-pages-label")}
      <input type="text" bind:value={range} dir="ltr" placeholder={i18n.t("merge-all-pages", { count: tab.info.pageCount })}
        aria-invalid={range.trim() !== "" && !pages} />
    </label>
    <div class="row">
      <label class="field">
        {i18n.t("export-format")}
        <select bind:value={format}>
          <option value="png">PNG</option>
          <option value="jpeg">JPEG</option>
        </select>
      </label>
      <label class="field">
        {i18n.t("export-resolution")}
        <select bind:value={dpi}>
          {#each [72, 96, 150, 200, 300, 600] as value (value)}
            <option {value}>{i18n.t("export-dpi", { dpi: value })}</option>
          {/each}
        </select>
      </label>
    </div>
    {#if format === "jpeg"}
      <label class="field">
        {i18n.t("export-quality", { quality })}
        <input type="range" min="30" max="100" step="5" bind:value={quality} />
      </label>
    {/if}
    <p class="hint">{format === "png" ? i18n.t("export-png-hint") : i18n.t("export-jpeg-hint")}</p>
    <div class="folder">
      <span class="label">{i18n.t("split-folder")}</span>
      <bdi class="path" dir="ltr" title={folder}>{folder}</bdi>
      <button type="button" onclick={chooseFolder}>{i18n.t("split-choose-folder")}</button>
    </div>
  </fieldset>
  {#snippet actions()}
    {#if progress !== null}
      <button type="button" onclick={() => (stopping = true)}>{i18n.t("export-stop")}</button>
    {:else}
      <button type="button" onclick={oncancel}>{i18n.t("cancel")}</button>
      <button type="submit" class="primary" disabled={!pages || !folder}>{i18n.t("export-run")}</button>
    {/if}
  {/snippet}
</ToolDialog>

<style>
  fieldset {
    min-width: 0;
    border: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  select {
    width: 100%;
  }
  input[type="range"] {
    accent-color: var(--accent);
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .folder .label {
    flex: none;
  }
  .folder .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font-size: 13px;
  }
  p.hint {
    margin: 0;
  }
</style>
