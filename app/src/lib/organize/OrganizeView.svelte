<script lang="ts">
// Organize pages: every page as a big thumbnail, to put in order (drag),
// turn, delete, duplicate, or add blank pages and pages from other files.
// Changes are planned here (with their own undo) and applied in one go on
// Done, which is then a single undo step of the document (see pageEdits.ts).
import { open } from "@tauri-apps/plugin-dialog";
import { untrack } from "svelte";
import type { DocInfo } from "../bindings/DocInfo";
import type { Choice } from "../ConfirmDialog.svelte";
import ContextMenu, { type MenuItem } from "../ContextMenu.svelte";
import Icon from "../Icon.svelte";
import { i18n } from "../i18n.svelte";
import { isTyping, modalOpen, shortcutKey } from "../keys";
import { type Degrees, rotatedSize } from "../layout";
import PageView from "../PageView.svelte";
import { arrange } from "../pageEdits";
import { closeDocument, openDocument, type RivetError, toRivetError } from "../pdf";
import type { Tab } from "../tabs.svelte";
import {
  deleteItems,
  duplicateItems,
  initialPlan,
  insertAfterSelection,
  isUnchanged,
  moveItems,
  newItem,
  type PlanItem,
  rangeOfKeys,
  rotateItems,
  sourceDoc,
  toSlots,
} from "./plan";

interface Props {
  tab: Tab;
  /** Leaves Organize pages (after Done or Cancel). */
  onclose: () => void;
  onerror: (e: RivetError) => void;
  onask: (title: string, message: string, choices: Choice[]) => Promise<string>;
  /** Extract these pages of the document (0-based) into a new file. */
  onextract: (pages: number[]) => void;
}
let { tab, onclose, onerror, onask, onextract }: Props = $props();

// --- The plan and its undo ---------------------------------------------------

// (The view is created per tab, so reading the page count once is intended.)
let items = $state.raw<PlanItem[]>(untrack(() => initialPlan(tab.info.pageCount)));
let past = $state.raw<PlanItem[][]>([]);
let future = $state.raw<PlanItem[][]>([]);
let selected = $state.raw(new Set<number>());
let anchor: number | null = null;
let busy = $state(false);
let notice = $state("");
let changed = $derived(!isUnchanged(items, tab.info.pageCount));

/** Other files whose pages were inserted (open in the engine until we leave). */
let others = $state.raw(new Map<number, { info: DocInfo; name: string }>());

function change(next: PlanItem[]) {
  if (next.length === 0) {
    notice = i18n.t("organize-last-page");
    return;
  }
  notice = "";
  past = [...past, items].slice(-100);
  future = [];
  items = next;
}

function undo() {
  const previous = past.at(-1);
  if (!previous) return;
  past = past.slice(0, -1);
  future = [...future, items];
  items = previous;
}

function redo() {
  const next = future.at(-1);
  if (!next) return;
  future = future.slice(0, -1);
  past = [...past, items];
  items = next;
}

// --- Actions -------------------------------------------------------------------

function rotate(turns: number) {
  if (selected.size) change(rotateItems(items, selected, turns));
}

function remove() {
  if (selected.size) {
    change(deleteItems(items, selected));
    if (items.length) selected = new Set();
  }
}

function duplicate() {
  if (!selected.size) return;
  const [next, copies] = duplicateItems(items, selected);
  change(next);
  selected = new Set(copies.map((c) => c.key));
}

function insertBlank() {
  // The size of the page it follows (or the first page).
  let near = items[0];
  for (const item of items) if (selected.has(item.key)) near = item;
  const size = sizeOf(near);
  const blank = newItem({ kind: "blank", width: size.width, height: size.height });
  change(insertAfterSelection(items, selected, [blank]));
  selected = new Set([blank.key]);
}

async function insertFromFile() {
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (typeof path !== "string") return;
  try {
    const { docId, info } = await openDocument(path);
    others = new Map(others).set(docId, { info, name: path.split(/[\\/]/).at(-1) ?? path });
    const added = Array.from({ length: info.pageCount }, (_, index) => newItem({ kind: "other", doc: docId, index }));
    change(insertAfterSelection(items, selected, added));
    selected = new Set(added.map((a) => a.key));
  } catch (e) {
    onerror(toRivetError(e));
  }
}

/** Selected pages can be extracted as they are in the document (not turned or added here). */
let extractable = $derived(
  selected.size > 0 &&
    items.every((item) => !selected.has(item.key) || (item.source.kind === "page" && item.turns === 0)),
);

function extract() {
  const pages = items.flatMap((item) =>
    selected.has(item.key) && item.source.kind === "page" ? [item.source.index] : [],
  );
  if (pages.length) onextract(pages);
}

async function done() {
  if (!changed) return leave();
  busy = true;
  try {
    await arrange(tab, toSlots(items));
    await leave();
  } catch (e) {
    onerror(toRivetError(e));
  } finally {
    busy = false;
  }
}

async function cancel() {
  if (changed) {
    const answer = await onask(i18n.t("organize-discard-title"), i18n.t("organize-discard-text"), [
      { id: "discard", label: i18n.t("organize-discard"), primary: true },
      { id: "keep", label: i18n.t("organize-keep") },
    ]);
    if (answer !== "discard") return;
  }
  await leave();
}

async function leave() {
  for (const doc of others.keys()) await closeDocument(doc).catch(() => {});
  others = new Map();
  onclose();
}

// --- Selection -------------------------------------------------------------------

function select(e: MouseEvent, key: number) {
  if (e.shiftKey && anchor !== null) {
    selected = rangeOfKeys(items, anchor, key);
    return;
  }
  if (e.ctrlKey || e.metaKey) {
    const next = new Set(selected);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    selected = next;
  } else if (!selected.has(key)) {
    selected = new Set([key]);
  }
  anchor = key;
}

function onkeydown(e: KeyboardEvent) {
  if (modalOpen() || isTyping(e.target) || busy) return;
  const mod = e.ctrlKey || e.metaKey;
  const key = shortcutKey(e);
  if (mod && key === "a") selected = new Set(items.map((i) => i.key));
  else if (mod && !e.shiftKey && key === "z") undo();
  else if (mod && (key === "y" || (e.shiftKey && key === "z"))) redo();
  else if (e.key === "Delete" || e.key === "Backspace") remove();
  else if (e.key === "Escape" && selected.size) selected = new Set();
  else if (e.key === "Escape") cancel();
  else return;
  e.preventDefault();
}

// --- Layout (only the rows on screen are drawn) ------------------------------------

/** Remembered while the app runs. */
let size = $state(sizeSetting.value);
$effect(() => {
  sizeSetting.value = size;
});
const GAP = 20;
const PAD = 24;
const LABEL = 26;

let scroller: HTMLElement;
let content: HTMLElement;
let width = $state(0);
let height = $state(0);
let scrollTop = $state(0);

let boxW = $derived(size);
let boxH = $derived(Math.round(size * 1.3));
let rowH = $derived(boxH + LABEL);
let cols = $derived(Math.max(1, Math.floor((width - 2 * PAD + GAP) / (boxW + GAP))));
let rows = $derived(Math.ceil(items.length / cols));
let totalHeight = $derived(2 * PAD + rows * rowH + Math.max(0, rows - 1) * GAP);
/** Space before the first column, so the grid is centred. */
let start = $derived(Math.max(PAD, (width - (cols * boxW + (cols - 1) * GAP)) / 2));
let visible = $derived.by(() => {
  const first = Math.max(0, Math.floor((scrollTop - PAD) / (rowH + GAP)) - 1);
  const last = Math.min(rows - 1, Math.ceil((scrollTop + height) / (rowH + GAP)) + 1);
  return items.slice(first * cols, (last + 1) * cols).map((item, i) => ({ item, index: first * cols + i }));
});

function cellX(index: number) {
  return start + (index % cols) * (boxW + GAP);
}
function cellY(index: number) {
  return PAD + Math.floor(index / cols) * (rowH + GAP);
}

/** Page size in points as it will be (with its extra turns). */
function sizeOf(item: PlanItem): { width: number; height: number } {
  let s: { width: number; height: number };
  if (item.source.kind === "blank") s = { width: item.source.width, height: item.source.height };
  else if (item.source.kind === "page") s = tab.info.pageSizes[item.source.index];
  else s = others.get(item.source.doc)?.info.pageSizes[item.source.index] ?? { width: 595, height: 842 };
  return rotatedSize(s, ((item.turns % 4) * 90) as Degrees);
}

/** The thumbnail's size, fitted into the box. */
function fitted(item: PlanItem) {
  const s = sizeOf(item);
  const scale = Math.min(boxW / s.width, boxH / s.height);
  return {
    width: Math.max(1, Math.round(s.width * scale)),
    height: Math.max(1, Math.round(s.height * scale)),
    pt: s.width,
  };
}

// --- Dragging pages into a new place ---------------------------------------------------

const DRAG_THRESHOLD = 5;
let press: { x: number; y: number; key: number; id: number } | null = null;
let dragging = $state(false);
let dropIndex = $state<number | null>(null);
let pointer = $state({ x: 0, y: 0 });
let autoScroll = 0;

function onpointerdown(e: PointerEvent, key: number) {
  if (e.button !== 0 || busy) return;
  select(e, key);
  press = { x: e.clientX, y: e.clientY, key, id: e.pointerId };
}

function onpointermove(e: PointerEvent) {
  if (!press || e.pointerId !== press.id) return;
  if (!dragging) {
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < DRAG_THRESHOLD) return;
    dragging = true;
    scroller.setPointerCapture(e.pointerId);
  }
  pointer = { x: e.clientX, y: e.clientY };
  dropIndex = indexAt(e.clientX, e.clientY);
  // Near the top or bottom edge, the view scrolls.
  const box = scroller.getBoundingClientRect();
  autoScroll = e.clientY < box.top + 48 ? -1 : e.clientY > box.bottom - 48 ? 1 : 0;
  if (autoScroll) requestAnimationFrame(scrollWhileDragging);
}

function scrollWhileDragging() {
  if (!dragging || !autoScroll) return;
  scroller.scrollTop += autoScroll * 14;
  dropIndex = indexAt(pointer.x, pointer.y);
  requestAnimationFrame(scrollWhileDragging);
}

function onpointerup(e: PointerEvent) {
  if (!press || e.pointerId !== press.id) return;
  if (dragging) {
    if (scroller.hasPointerCapture(e.pointerId)) scroller.releasePointerCapture(e.pointerId);
    if (dropIndex !== null) {
      const next = moveItems(items, selected, dropIndex);
      if (next.some((item, i) => item !== items[i])) change(next);
    }
  } else if (!(e.ctrlKey || e.metaKey || e.shiftKey)) {
    // A plain click on a page that was part of a selection selects just it.
    selected = new Set([press.key]);
  }
  press = null;
  dragging = false;
  dropIndex = null;
  autoScroll = 0;
}

/** Where dropped pages would go: before the page at this index (length: at the end). */
function indexAt(clientX: number, clientY: number): number {
  const box = content.getBoundingClientRect();
  const rtl = getComputedStyle(scroller).direction === "rtl";
  const x = (rtl ? box.right - clientX : clientX - box.left) - start;
  const y = clientY - box.top - PAD;
  const pos = x / (boxW + GAP);
  const col = Math.floor(pos);
  const insertCol = Math.max(0, Math.min(cols, pos - col < boxW / 2 / (boxW + GAP) ? col : col + 1));
  const row = Math.max(0, Math.min(rows - 1, Math.floor((y + GAP / 2) / (rowH + GAP))));
  return Math.max(0, Math.min(items.length, row * cols + insertCol));
}

/** Where the insertion line is drawn for `dropIndex`. */
let marker = $derived.by(() => {
  if (dropIndex === null || items.length === 0) return null;
  const at = Math.min(dropIndex, items.length - 1);
  const end = dropIndex === items.length;
  return { x: end ? cellX(at) + boxW + GAP / 2 : cellX(at) - GAP / 2, y: cellY(at) };
});

// --- Right-click menu -------------------------------------------------------------------

let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

function showMenu(e: MouseEvent, key: number) {
  e.preventDefault();
  if (!selected.has(key)) selected = new Set([key]);
  menu = {
    x: e.clientX,
    y: e.clientY,
    items: [
      { label: i18n.t("rotate-clockwise"), action: () => rotate(1) },
      { label: i18n.t("rotate-counterclockwise"), action: () => rotate(-1) },
      { label: i18n.t("duplicate-pages"), action: duplicate },
      { label: i18n.t("insert-blank-page"), action: insertBlank },
      { label: i18n.t("extract-pages"), disabled: !extractable, hint: i18n.t("extract-apply-first"), action: extract },
      { label: i18n.t("delete-pages"), shortcut: "Delete", action: remove },
    ],
  };
}

function label(index: number): string {
  return (index + 1).toLocaleString(i18n.locale);
}

function hint(item: PlanItem): string {
  if (item.source.kind === "other") {
    return i18n.t("organize-inserted-from", { file: others.get(item.source.doc)?.name ?? "" });
  }
  if (item.source.kind === "blank") return i18n.t("organize-blank-page");
  return "";
}
</script>

<script lang="ts" module>
/** Thumbnail size, kept between visits to Organize pages. */
const sizeSetting = { value: 170 };
</script>

<svelte:window {onkeydown} />

<section class="organize" aria-label={i18n.t("organize-pages")}>
  <div class="bar" role="toolbar" aria-label={i18n.t("organize-pages")}>
    <div class="actions">
      <button class="icon" onclick={() => rotate(-1)} disabled={!selected.size || busy}
        aria-label={i18n.t("rotate-counterclockwise")} title={i18n.t("rotate-counterclockwise")}>
        <Icon name="rotate-ccw" />
      </button>
      <button class="icon" onclick={() => rotate(1)} disabled={!selected.size || busy}
        aria-label={i18n.t("rotate-clockwise")} title={i18n.t("rotate-clockwise")}>
        <Icon name="rotate" />
      </button>
      <button class="icon" onclick={duplicate} disabled={!selected.size || busy}
        aria-label={i18n.t("duplicate-pages")} title={i18n.t("duplicate-pages")}>
        <Icon name="copy" />
      </button>
      <button class="icon" onclick={remove} disabled={!selected.size || busy}
        aria-label={i18n.t("delete-pages")} title={i18n.t("delete-pages")} data-shortcut="Delete">
        <Icon name="trash" />
      </button>
      <span class="sep" aria-hidden="true"></span>
      <button class="icon" onclick={insertBlank} disabled={busy}
        aria-label={i18n.t("insert-blank-page")} title={i18n.t("insert-blank-page")}>
        <Icon name="file" />
      </button>
      <button class="icon" onclick={insertFromFile} disabled={busy}
        aria-label={i18n.t("insert-from-file")} title={i18n.t("insert-from-file")}>
        <Icon name="file-plus" />
      </button>
      <button class="icon" onclick={extract} disabled={!extractable || busy}
        aria-label={i18n.t("extract-pages")} title={selected.size && !extractable ? i18n.t("extract-apply-first") : i18n.t("extract-pages")}>
        <Icon name="extract" />
      </button>
      <span class="sep" aria-hidden="true"></span>
      <button class="icon" onclick={undo} disabled={!past.length || busy}
        aria-label={i18n.t("undo")} title={i18n.t("undo")} data-shortcut="Ctrl+Z">
        <Icon name="undo" />
      </button>
      <button class="icon" onclick={redo} disabled={!future.length || busy}
        aria-label={i18n.t("redo")} title={i18n.t("redo")} data-shortcut="Ctrl+Y">
        <Icon name="redo" />
      </button>
    </div>
    <p class="status" aria-live="polite">
      {#if notice}
        <span class="notice">{notice}</span>
      {:else if selected.size}
        {i18n.t("organize-selected", { count: selected.size })}
      {:else}
        {i18n.t("organize-hint")}
      {/if}
    </p>
    <div class="end">
      <input type="range" min="90" max="320" step="10" bind:value={size} aria-label={i18n.t("organize-size")}
        title={i18n.t("organize-size")} />
      <button onclick={cancel} disabled={busy}>{i18n.t("cancel")}</button>
      <button class="primary" onclick={done} disabled={busy}>{i18n.t("organize-done")}</button>
    </div>
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="scroller" bind:this={scroller} bind:clientWidth={width} bind:clientHeight={height}
    onscroll={() => (scrollTop = scroller.scrollTop)} {onpointermove} {onpointerup} onpointercancel={onpointerup}
    onpointerdown={(e) => {
      if (e.target === scroller || e.target === content) selected = new Set();
    }}
    class:dragging>
    <div class="content" bind:this={content} style:height="{totalHeight}px">
      <!-- Cells are keyed by position, and a page's view is made anew when
           another page lands there: moving a drawn <canvas> in the DOM loses
           its pixels in WebKitGTK. Thumbnails come from the engine's cache. -->
      {#each visible as { item, index } (index)}
        {@const fit = fitted(item)}
        {@const source = sourceDoc(item, tab.docId)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="cell" class:selected={selected.has(item.key)} class:lifted={dragging && selected.has(item.key)}
          style:inset-inline-start="{cellX(index)}px" style:top="{cellY(index)}px"
          style:width="{boxW}px" style:height="{rowH}px"
          title={hint(item)}
          onpointerdown={(e) => onpointerdown(e, item.key)}
          oncontextmenu={(e) => showMenu(e, item.key)}>
          <div class="box" style:height="{boxH}px">
            <div class="page" style:width="{fit.width}px" style:height="{fit.height}px">
              {#key item.key}
                {#if source}
                  <PageView docId={source.doc} index={source.index} width={fit.width} height={fit.height}
                    widthPt={fit.pt} rotation={((item.turns % 4) * 90) as Degrees}
                    revision={source.doc === tab.docId ? tab.pageRevision(source.index) : 0} thumbnail />
                {/if}
              {/key}
            </div>
          </div>
          <span class="label">
            {#if item.source.kind !== "page"}<span class="added" aria-hidden="true">+</span>{/if}
            {label(index)}
          </span>
        </div>
      {/each}
      {#if dragging && marker}
        <div class="marker" style:inset-inline-start="{marker.x - 1.5}px" style:top="{marker.y}px" style:height="{boxH}px"></div>
      {/if}
    </div>
  </div>

  {#if dragging}
    <div class="drag-count" style:left="{pointer.x + 14}px" style:top="{pointer.y + 14}px">{selected.size}</div>
  {/if}
</section>

{#if menu}
  <ContextMenu {...menu} onclose={() => (menu = null)} />
{/if}

<style>
  .organize {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--canvas);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-block: 6px;
    padding-inline: 12px;
    border-block-end: 1px solid var(--border);
    background: var(--chrome);
  }
  .actions,
  .end {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .end {
    gap: 8px;
  }
  .end input {
    width: 110px;
    accent-color: var(--accent);
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border);
    margin-inline: 4px;
  }
  .status {
    flex: 1;
    min-width: 0;
    margin: 0;
    color: var(--muted);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: center;
  }
  .notice {
    color: var(--text);
  }
  .scroller {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    position: relative;
  }
  .scroller.dragging {
    cursor: grabbing;
  }
  .content {
    position: relative;
  }
  .cell {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: center;
    border-radius: 8px;
  }
  .box {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .page {
    position: relative;
    background: #fff;
    box-shadow: var(--shadow-page);
    outline: 3px solid transparent;
    outline-offset: 3px;
    border-radius: 2px;
  }
  .cell:hover .page {
    outline-color: var(--border);
  }
  .cell.selected .page {
    outline-color: var(--accent);
  }
  .cell.lifted {
    opacity: 0.45;
  }
  .label {
    height: 26px;
    line-height: 26px;
    font-size: 12px;
    color: var(--muted);
    display: inline-flex;
    gap: 4px;
  }
  .cell.selected .label {
    color: var(--text);
    font-weight: 600;
  }
  .added {
    color: var(--accent);
    font-weight: 700;
  }
  .marker {
    position: absolute;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
    pointer-events: none;
  }
  .drag-count {
    position: fixed;
    z-index: 10;
    min-width: 24px;
    height: 24px;
    padding-inline: 6px;
    border-radius: 12px;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 12px;
    font-weight: 600;
    line-height: 24px;
    text-align: center;
    pointer-events: none;
  }
</style>
