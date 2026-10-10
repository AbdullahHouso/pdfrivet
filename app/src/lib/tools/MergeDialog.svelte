<script lang="ts">
// Merge PDFs: a list of files (the open document first, when there is one),
// in order, each with all or some of its pages. The pages are copied into a
// new document one file at a time (with progress), then bookmarks and form
// fields are finished in Rust (merge.rs) and the file is written.
import { open, save } from "@tauri-apps/plugin-dialog";
import { untrack } from "svelte";
import Icon from "../Icon.svelte";
import { i18n } from "../i18n.svelte";
import { parsePageRange } from "../pageRange";
import {
  closeDocument,
  finishMerge,
  importPages,
  newDocument,
  openDocument,
  type RivetError,
  toRivetError,
} from "../pdf";
import ToolDialog from "./ToolDialog.svelte";

interface Props {
  /** The open document, listed first (it stays open afterwards). */
  current?: MergeSource;
  /** The merged file was written; `open`: show it in a new tab. */
  onfinished: (path: string, open: boolean) => void;
  onerror: (e: RivetError) => void;
  oncancel: () => void;
}
let { current, onfinished, onerror, oncancel }: Props = $props();

interface Item extends MergeSource {
  id: number;
  /** Pages to take, as typed ("" = all). */
  range: string;
  /** Opened by this dialog (closed again when it's done). */
  owned: boolean;
  /** Needs a password before its pages can be read. */
  locked: boolean;
  password: string;
  wrongPassword: boolean;
}

let nextId = 1;
let items = $state<Item[]>(
  untrack(() =>
    current
      ? [{ ...current, id: nextId++, range: "", owned: false, locked: false, password: "", wrongPassword: false }]
      : [],
  ),
);
let bookmarkFiles = $state(true);
let openAfter = $state(true);
let progress = $state<number | null>(null);
let busy = $derived(progress !== null);

const fileName = (path: string) => path.split(/[\\/]/).at(-1) ?? path;

function pagesOf(item: Item): number[] | null {
  if (item.range.trim() === "") return Array.from({ length: item.pageCount }, (_, i) => i);
  return parsePageRange(item.range, item.pageCount);
}

let ready = $derived(items.length >= 2 && items.every((item) => !item.locked && pagesOf(item)));

async function addFiles() {
  const picked = await open({
    multiple: true,
    directory: false,
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (!picked) return;
  for (const path of Array.isArray(picked) ? picked : [picked]) {
    const item: Item = {
      id: nextId++,
      docId: -1,
      path,
      name: fileName(path),
      pageCount: 0,
      range: "",
      owned: true,
      locked: false,
      password: "",
      wrongPassword: false,
    };
    try {
      const { docId, info } = await openDocument(path);
      items.push({ ...item, docId, pageCount: info.pageCount });
    } catch (e) {
      const err = toRivetError(e);
      if (err.code === "password-required") items.push({ ...item, locked: true });
      else onerror(err);
    }
  }
}

async function unlock(item: Item) {
  try {
    const { docId, info } = await openDocument(item.path, item.password);
    item.docId = docId;
    item.pageCount = info.pageCount;
    item.locked = false;
    item.wrongPassword = false;
  } catch (e) {
    const err = toRivetError(e);
    if (err.code === "wrong-password" || err.code === "password-required") item.wrongPassword = true;
    else onerror(err);
  }
}

function move(index: number, by: -1 | 1) {
  const to = index + by;
  if (to < 0 || to >= items.length) return;
  [items[index], items[to]] = [items[to], items[index]];
}

async function remove(index: number) {
  const [item] = items.splice(index, 1);
  if (item.owned && item.docId >= 0) await closeDocument(item.docId).catch(() => {});
}

/** Closes the files this dialog opened. */
async function closeOwned() {
  for (const item of items) if (item.owned && item.docId >= 0) await closeDocument(item.docId).catch(() => {});
}

async function cancel() {
  if (busy) return;
  await closeOwned();
  oncancel();
}

async function run() {
  if (!ready || busy) return;
  const first = items[0].path;
  const folder = first.slice(0, first.length - fileName(first).length);
  const target = await save({
    defaultPath: `${folder}${i18n.t("merge-name")}.pdf`,
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (!target) return;
  const path = /\.pdf$/i.test(target) ? target : `${target}.pdf`;
  progress = 0;
  let merged: number | null = null;
  try {
    merged = (await newDocument()).docId;
    const parts = [];
    let start = 0;
    for (const [i, item] of items.entries()) {
      const pages = pagesOf(item) ?? [];
      await importPages(merged, item.docId, pages, start);
      parts.push({ doc: item.docId, pages, start, title: item.name.replace(/\.pdf$/i, "") });
      start += pages.length;
      progress = (i + 1) / (items.length + 1);
    }
    await finishMerge(merged, parts, bookmarkFiles, path);
    progress = 1;
    await closeOwned();
    onfinished(path, openAfter);
  } catch (e) {
    onerror(toRivetError(e));
  } finally {
    if (merged !== null) await closeDocument(merged).catch(() => {});
    progress = null;
  }
}
</script>

<script lang="ts" module>
export interface MergeSource {
  docId: number;
  path: string;
  name: string;
  pageCount: number;
}
</script>

<ToolDialog title={i18n.t("merge-title")} icon="merge" oncancel={cancel} onsubmit={run} {progress} wide>
  {#if items.length === 0}
    <p class="hint">{i18n.t("merge-empty")}</p>
  {:else}
    <ol class="files">
      {#each items as item, i (item.id)}
        <li>
          <span class="number">{(i + 1).toLocaleString(i18n.locale)}</span>
          <div class="file">
            <bdi class="name" title={item.path}>{item.name}</bdi>
            {#if item.locked}
              <div class="unlock">
                <input type="password" bind:value={item.password} placeholder={i18n.t("password")}
                  aria-label={i18n.t("password")} aria-invalid={item.wrongPassword} disabled={busy}
                  onkeydown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      unlock(item);
                    }
                  }} />
                <button type="button" onclick={() => unlock(item)} disabled={!item.password || busy}>
                  {i18n.t("merge-unlock")}
                </button>
              </div>
              {#if item.wrongPassword}<p class="error">{i18n.t("error-wrong-password")}</p>{/if}
            {:else}
              <input type="text" class="range" bind:value={item.range} dir="ltr" disabled={busy}
                placeholder={i18n.t("merge-all-pages", { count: item.pageCount })}
                aria-label={i18n.t("merge-pages-of", { name: item.name })}
                aria-invalid={item.range.trim() !== "" && !pagesOf(item)} />
            {/if}
          </div>
          <div class="row-actions">
            <button type="button" class="icon" onclick={() => move(i, -1)} disabled={i === 0 || busy}
              aria-label={i18n.t("merge-move-up")} title={i18n.t("merge-move-up")}>
              <Icon name="chevron-up" />
            </button>
            <button type="button" class="icon" onclick={() => move(i, 1)} disabled={i === items.length - 1 || busy}
              aria-label={i18n.t("merge-move-down")} title={i18n.t("merge-move-down")}>
              <Icon name="chevron-down" />
            </button>
            <button type="button" class="icon" onclick={() => remove(i)} disabled={busy}
              aria-label={i18n.t("merge-remove")} title={i18n.t("merge-remove")}>
              <Icon name="close" />
            </button>
          </div>
        </li>
      {/each}
    </ol>
  {/if}
  <div class="add">
    <button type="button" onclick={addFiles} disabled={busy}>
      <Icon name="plus" />{i18n.t("merge-add")}
    </button>
    <span class="hint">{i18n.t("merge-range-hint")}</span>
  </div>
  <label class="check">
    <input type="checkbox" bind:checked={bookmarkFiles} disabled={busy} />
    {i18n.t("merge-bookmark-files")}
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={openAfter} disabled={busy} />
    {i18n.t("merge-open-after")}
  </label>
  {#snippet actions()}
    <button type="button" onclick={cancel} disabled={busy}>{i18n.t("cancel")}</button>
    <button type="submit" class="primary" disabled={!ready || busy}>{i18n.t("merge-run")}</button>
  {/snippet}
</ToolDialog>

<style>
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 46vh;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-block: 6px;
    padding-inline: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--field);
  }
  .number {
    flex: none;
    min-width: 18px;
    color: var(--muted);
    font-size: 12px;
    text-align: center;
  }
  .file {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file input.range,
  .unlock input {
    padding-block: 4px;
    font-size: 13px;
  }
  .unlock {
    display: flex;
    gap: 6px;
  }
  .row-actions {
    display: flex;
    gap: 2px;
  }
  .row-actions button {
    border: none;
  }
  .add {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .add button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
