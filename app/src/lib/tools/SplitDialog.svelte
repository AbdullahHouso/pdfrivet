<script lang="ts">
// Split a document into several files: every few pages, by page ranges, or
// at its top-level bookmarks. Each file is written in turn (with progress)
// into a folder; existing files are never overwritten.
import { open } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import { i18n } from "../i18n.svelte";
import { loadBookmarks } from "../outlineLoad";
import { extractPages, filesExist, type RivetError, toRivetError } from "../pdf";
import type { Tab } from "../tabs.svelte";
import { partFileNames, type SplitPart, splitAtBookmarks, splitEvery, splitRanges } from "./split";
import ToolDialog from "./ToolDialog.svelte";

interface Props {
  tab: Tab;
  /** The files were written (`count` of them, the first at `first`). */
  onfinished: (count: number, first: string) => void;
  onerror: (e: RivetError) => void;
  oncancel: () => void;
}
let { tab, onfinished, onerror, oncancel }: Props = $props();

type Mode = "every" | "ranges" | "bookmarks";
let mode = $state<Mode>("every");
let size = $state(1);
let ranges = $state("");
let bookmarks = $state<{ title: string; page: number | null }[]>([]);
let progress = $state<number | null>(null);

const separator = $derived(tab.path.includes("\\") ? "\\" : "/");
let folder = $state("");
onMount(() => {
  folder = tab.path.slice(0, tab.path.length - tab.fileName.length).replace(/[\\/]$/, "");
  loadBookmarks(tab)
    .then((items) => (bookmarks = items.map((b) => ({ title: b.title, page: b.page }))))
    .catch(() => {});
});

let parts = $derived.by((): SplitPart[] | null => {
  const count = tab.info.pageCount;
  if (mode === "every") return size >= 1 ? splitEvery(count, size) : null;
  if (mode === "ranges") return splitRanges(ranges, count);
  const found = splitAtBookmarks(bookmarks, count);
  return found.length ? found : null;
});

async function chooseFolder() {
  const picked = await open({ directory: true, multiple: false, defaultPath: folder || undefined });
  if (typeof picked === "string") folder = picked;
}

/** Names for the parts that don't overwrite anything in the folder. */
async function names(list: SplitPart[]): Promise<string[]> {
  const base = tab.fileName.replace(/\.pdf$/i, "");
  const taken = new Set<string>();
  for (let tries = 0; tries < 8; tries++) {
    const candidates = partFileNames(base, list, taken);
    const exists = await filesExist(candidates.map((n) => folder + separator + n));
    const clash = candidates.filter((_, i) => exists[i]);
    if (clash.length === 0) return candidates;
    for (const n of clash) taken.add(n);
  }
  throw new Error("couldn't find free file names");
}

async function run() {
  if (!parts || !folder || progress !== null) return;
  progress = 0;
  try {
    const files = (await names(parts)).map((n) => folder + separator + n);
    for (const [i, part] of parts.entries()) {
      await extractPages(tab.docId, part.pages, files[i]);
      progress = (i + 1) / parts.length;
    }
    onfinished(parts.length, files[0]);
  } catch (e) {
    onerror(toRivetError(e));
  } finally {
    progress = null;
  }
}
</script>

<ToolDialog title={i18n.t("split-title")} icon="split" oncancel={() => progress === null && oncancel()} onsubmit={run} {progress}>
  <fieldset disabled={progress !== null}>
    <label class="check">
      <input type="radio" name="split-mode" value="every" bind:group={mode} />
      {i18n.t("split-mode-every")}
    </label>
    {#if mode === "every"}
      <label class="field sub">
        {i18n.t("split-pages-per-file")}
        <input type="number" min="1" max={tab.info.pageCount} bind:value={size} />
      </label>
    {/if}
    <label class="check">
      <input type="radio" name="split-mode" value="ranges" bind:group={mode} />
      {i18n.t("split-mode-ranges")}
    </label>
    {#if mode === "ranges"}
      <label class="field sub">
        <!-- svelte-ignore a11y_autofocus -->
        <input type="text" bind:value={ranges} dir="ltr" autofocus placeholder="1-3, 4-10, 11"
          aria-invalid={ranges.trim() !== "" && !parts} aria-label={i18n.t("split-mode-ranges")} />
        <span class="hint">{i18n.t("split-ranges-hint")}</span>
      </label>
    {/if}
    <label class="check">
      <input type="radio" name="split-mode" value="bookmarks" bind:group={mode} disabled={!splitAtBookmarks(bookmarks, tab.info.pageCount).length} />
      {i18n.t("split-mode-bookmarks")}
    </label>
    {#if !splitAtBookmarks(bookmarks, tab.info.pageCount).length}
      <span class="hint sub">{i18n.t("split-no-bookmarks")}</span>
    {/if}

    <div class="folder">
      <span class="label">{i18n.t("split-folder")}</span>
      <bdi class="path" dir="ltr" title={folder}>{folder}</bdi>
      <button type="button" onclick={chooseFolder}>{i18n.t("split-choose-folder")}</button>
    </div>
    {#if parts}
      <p class="hint">{i18n.t("split-count", { count: parts.length })}</p>
    {/if}
  </fieldset>
  {#snippet actions()}
    <button type="button" onclick={oncancel} disabled={progress !== null}>{i18n.t("cancel")}</button>
    <button type="submit" class="primary" disabled={!parts || !folder || progress !== null}>{i18n.t("split-run")}</button>
  {/snippet}
</ToolDialog>

<style>
  fieldset {
    /* Fieldsets are as wide as their content by default; the folder path would overflow. */
    min-width: 0;
    border: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .sub {
    margin-inline-start: 26px;
  }
  .sub input[type="number"] {
    width: 100px;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-block-start: 6px;
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
