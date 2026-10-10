<script lang="ts">
// Images to PDF: pictures in order, one per page, on pages the size of the
// picture or A4/Letter, with an optional margin.
import { open, save } from "@tauri-apps/plugin-dialog";
import { untrack } from "svelte";
import type { PagePaper } from "../bindings/PagePaper";
import Icon from "../Icon.svelte";
import { i18n } from "../i18n.svelte";
import { addImagePage, closeDocument, newDocument, type RivetError, saveDocument, toRivetError } from "../pdf";
import { fileNameOf, folderOf, separatorOf } from "./files";
import ToolDialog from "./ToolDialog.svelte";

interface Props {
  /** Pictures to start with (e.g. dropped on the window). */
  initial?: string[];
  onfinished: (path: string, open: boolean) => void;
  onerror: (e: RivetError) => void;
  oncancel: () => void;
}
let { initial = [], onfinished, onerror, oncancel }: Props = $props();

let nextId = 1;
let files = $state(untrack(() => initial.map((path) => ({ id: nextId++, path }))));
let paper = $state<PagePaper>("a4");
let margin = $state(0);
let openAfter = $state(true);
let progress = $state<number | null>(null);
let busy = $derived(progress !== null);

async function add() {
  const picked = await open({
    multiple: true,
    directory: false,
    filters: [{ name: i18n.t("images-files"), extensions: IMAGE_EXTENSIONS }],
  });
  if (!picked) return;
  for (const path of Array.isArray(picked) ? picked : [picked]) files.push({ id: nextId++, path });
}

function move(index: number, by: -1 | 1) {
  const to = index + by;
  if (to >= 0 && to < files.length) [files[index], files[to]] = [files[to], files[index]];
}

async function run() {
  if (!files.length || busy) return;
  const first = files[0].path;
  const target = await save({
    defaultPath: folderOf(first) + separatorOf(first) + fileNameOf(first).replace(/\.[^.]+$/, "") + ".pdf",
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (!target) return;
  const path = /\.pdf$/i.test(target) ? target : `${target}.pdf`;
  progress = 0;
  let doc: number | null = null;
  try {
    doc = (await newDocument()).docId;
    for (const [i, file] of files.entries()) {
      await addImagePage(doc, file.path, { paper, margin });
      progress = (i + 1) / (files.length + 1);
    }
    await saveDocument(doc, path);
    progress = 1;
    onfinished(path, openAfter);
  } catch (e) {
    onerror(toRivetError(e));
  } finally {
    if (doc !== null) await closeDocument(doc).catch(() => {});
    progress = null;
  }
}
</script>

<script lang="ts" module>
/** The picture files Images to PDF takes. */
export const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "webp", "bmp", "gif", "tif", "tiff"];

export function isImage(path: string): boolean {
  const ext = path.split(".").at(-1)?.toLowerCase() ?? "";
  return IMAGE_EXTENSIONS.includes(ext);
}
</script>

<ToolDialog title={i18n.t("images-title")} icon="image" oncancel={() => !busy && oncancel()} onsubmit={run} {progress} wide>
  {#if files.length === 0}
    <p class="hint">{i18n.t("images-empty")}</p>
  {:else}
    <ol class="files">
      {#each files as file, i (file.id)}
        <li>
          <span class="number">{(i + 1).toLocaleString(i18n.locale)}</span>
          <bdi class="name" title={file.path}>{fileNameOf(file.path)}</bdi>
          <button type="button" class="icon" onclick={() => move(i, -1)} disabled={i === 0 || busy}
            aria-label={i18n.t("merge-move-up")} title={i18n.t("merge-move-up")}><Icon name="chevron-up" /></button>
          <button type="button" class="icon" onclick={() => move(i, 1)} disabled={i === files.length - 1 || busy}
            aria-label={i18n.t("merge-move-down")} title={i18n.t("merge-move-down")}><Icon name="chevron-down" /></button>
          <button type="button" class="icon" onclick={() => files.splice(i, 1)} disabled={busy}
            aria-label={i18n.t("merge-remove")} title={i18n.t("merge-remove")}><Icon name="close" /></button>
        </li>
      {/each}
    </ol>
  {/if}
  <div>
    <button type="button" class="add" onclick={add} disabled={busy}><Icon name="plus" />{i18n.t("images-add")}</button>
  </div>
  <div class="row">
    <label class="field">
      {i18n.t("images-page-size")}
      <select bind:value={paper} disabled={busy}>
        <option value="a4">A4</option>
        <option value="letter">{i18n.t("images-letter")}</option>
        <option value="image">{i18n.t("images-same-size")}</option>
      </select>
    </label>
    <label class="field">
      {i18n.t("images-margin")}
      <select bind:value={margin} disabled={busy}>
        <option value={0}>{i18n.t("images-margin-none")}</option>
        <option value={18}>{i18n.t("images-margin-small")}</option>
        <option value={36}>{i18n.t("images-margin-large")}</option>
      </select>
    </label>
  </div>
  <label class="check">
    <input type="checkbox" bind:checked={openAfter} disabled={busy} />
    {i18n.t("images-open-after")}
  </label>
  {#snippet actions()}
    <button type="button" onclick={oncancel} disabled={busy}>{i18n.t("cancel")}</button>
    <button type="submit" class="primary" disabled={!files.length || busy}>{i18n.t("images-run")}</button>
  {/snippet}
</ToolDialog>

<style>
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 40vh;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-block: 4px;
    padding-inline: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--field);
  }
  li button {
    border: none;
  }
  .number {
    min-width: 18px;
    color: var(--muted);
    font-size: 12px;
    text-align: center;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  select {
    width: 100%;
  }
  p.hint {
    margin: 0;
  }
</style>
