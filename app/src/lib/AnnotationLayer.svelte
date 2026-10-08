<script lang="ts">
// Annotations on one page: drawing new ones with the Annotate tools, and
// selecting, moving, resizing, recolouring and deleting existing ones.
// PDFium draws the annotations into the page image; this layer only shows
// what is being drawn or dragged, the selection frame and its menu.
import { untrack } from "svelte";
import {
  type AnnotTool,
  add,
  annotate,
  isMarkupTool,
  lighter,
  PALETTE,
  placeSignature,
  remove,
  toHex,
  update,
} from "./annotate.svelte";
import {
  canMove,
  dragHandle,
  fromPx,
  type Handle,
  hits,
  keepsSize,
  moved,
  type PageBox,
  pxToRect,
  rectFrom,
  rectToPx,
  resized,
  simplify,
  toPx,
  topmostAt,
} from "./annotGeometry";
import type { Annotation } from "./bindings/Annotation";
import type { Color } from "./bindings/Color";
import type { PagePoint } from "./bindings/PagePoint";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import type { Degrees } from "./layout";
import { getAnnotations, type RivetError, toRivetError } from "./pdf";
import { settings } from "./settings.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  index: number;
  /** Size of the page on screen, in px. */
  width: number;
  height: number;
  /** Screen px per PDF point (for line widths). */
  scale: number;
  rotation: Degrees;
  onerror?: (e: RivetError) => void;
}
let { tab, index, width, height, scale, rotation, onerror }: Props = $props();

let box = $derived<PageBox>({ width, height, rotation });
let layer: HTMLDivElement;

// Load the page's annotations, again whenever the page changes.
$effect(() => {
  tab.pageRevision(index);
  let cancelled = false;
  getAnnotations(tab.docId, index)
    .then((list) => {
      if (!cancelled) tab.annotations.set(index, list);
    })
    .catch(() => {});
  return () => {
    cancelled = true;
  };
});
$effect(() => () => untrack(() => tab.annotations.delete(index)));

let list = $derived(tab.annotations.get(index) ?? []);
let tool = $derived<AnnotTool | null>(annotate.open && settings.tool === "select" ? annotate.tool : null);
/** The layer takes the pointer for drawing tools (text markup works through text selection). */
let drawingTool = $derived(tool !== null && !isMarkupTool(tool));
let selected = $derived(
  tab.selectedAnnotation?.page === index ? (list.find((a) => a.id === tab.selectedAnnotation?.id) ?? null) : null,
);

function fail(e: unknown) {
  onerror?.(toRivetError(e));
}

/** Pointer position in px on this page. */
function pointer(e: PointerEvent | MouseEvent) {
  const r = layer.getBoundingClientRect();
  return { x: e.clientX - r.left, y: e.clientY - r.top };
}

function newAnnotation(t: AnnotTool, kind: Annotation["kind"], rect: Annotation["rect"]): Annotation {
  const style = annotate.style(t);
  return {
    id: "",
    kind,
    rect,
    color: style.color,
    opacity: style.opacity,
    width: style.width,
    contents: "",
    author: "",
    modified: null,
    editable: true,
  };
}

// --- Drawing -------------------------------------------------------------

type Draft =
  | { tool: "pen"; points: { x: number; y: number }[] }
  | { tool: "rectangle" | "ellipse" | "line" | "arrow"; from: { x: number; y: number }; to: { x: number; y: number } };
let draft = $state<Draft | null>(null);
let erased = new Set<string>();

function onLayerDown(e: PointerEvent) {
  if (!tool || e.button !== 0) return;
  e.preventDefault();
  e.stopPropagation();
  const p = pointer(e);
  tab.selectedAnnotation = null;
  if (tool === "note") {
    placeNote(p);
    return;
  }
  if (tool === "signature") {
    putSignature(p);
    return;
  }
  layer.setPointerCapture(e.pointerId);
  if (tool === "eraser") {
    erased = new Set();
    erase(p);
  } else if (tool === "pen") {
    draft = { tool, points: [p] };
  } else if (tool === "rectangle" || tool === "ellipse" || tool === "line" || tool === "arrow") {
    draft = { tool, from: p, to: p };
  }
}

function onLayerMove(e: PointerEvent) {
  if (tool === "signature") ghostAt = pointer(e);
  if (!layer.hasPointerCapture(e.pointerId)) return;
  const p = pointer(e);
  if (tool === "eraser") erase(p);
  else if (draft?.tool === "pen") draft = { ...draft, points: [...draft.points, p] };
  else if (draft) draft = { ...draft, to: e.shiftKey ? constrain(draft.from, p, draft.tool) : p };
}

/** Shift: squares and circles, and lines at 45° steps. */
function constrain(from: { x: number; y: number }, p: { x: number; y: number }, t: string) {
  const dx = p.x - from.x;
  const dy = p.y - from.y;
  if (t === "rectangle" || t === "ellipse") {
    const size = Math.max(Math.abs(dx), Math.abs(dy));
    return { x: from.x + Math.sign(dx) * size, y: from.y + Math.sign(dy) * size };
  }
  const angle = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
  const length = Math.hypot(dx, dy);
  return { x: from.x + length * Math.cos(angle), y: from.y + length * Math.sin(angle) };
}

async function onLayerUp(e: PointerEvent) {
  if (!layer.hasPointerCapture(e.pointerId)) return;
  layer.releasePointerCapture(e.pointerId);
  const d = draft;
  draft = null;
  if (!d || !tool) return;
  try {
    if (d.tool === "pen") {
      const points = simplify(d.points, 0.6).map((p) => fromPx(p.x, p.y, box));
      if (points.length === 1) points.push({ x: points[0].x + 0.0005, y: points[0].y });
      await add(tab, index, newAnnotation("pen", { kind: "ink", strokes: [points] }, rectFrom(points[0], points[0])));
      return;
    }
    // Ignore clicks: a shape needs some size.
    if (Math.hypot(d.to.x - d.from.x, d.to.y - d.from.y) < 4) return;
    const from = fromPx(d.from.x, d.from.y, box);
    const to = fromPx(d.to.x, d.to.y, box);
    if (d.tool === "line" || d.tool === "arrow") {
      await add(
        tab,
        index,
        newAnnotation(d.tool, { kind: "line", from, to, arrow: d.tool === "arrow" }, rectFrom(from, to)),
      );
    } else {
      const style = annotate.style(d.tool);
      const fill = style.fill ? lighter(style.color) : null;
      const kind: Annotation["kind"] = d.tool === "rectangle" ? { kind: "square", fill } : { kind: "circle", fill };
      await add(tab, index, newAnnotation(d.tool, kind, rectFrom(from, to)));
    }
  } catch (err) {
    fail(err);
  }
}

/** The eraser removes drawings and shapes it touches (text markup and notes are deleted from their menu). */
function erase(p: { x: number; y: number }) {
  const target = [...list]
    .reverse()
    .find(
      (a) =>
        a.editable &&
        ["ink", "line", "square", "circle"].includes(a.kind.kind) &&
        !erased.has(a.id) &&
        hits(a, p.x, p.y, box, 8),
    );
  if (!target) return;
  erased.add(target.id);
  remove(tab, index, target).catch(fail);
}

/** Size of a note's icon, in PDF points. */
const NOTE_SIZE = 22;

async function placeNote(p: { x: number; y: number }) {
  const half = (NOTE_SIZE * scale) / 2;
  const rect = pxToRect({ left: p.x - half, top: p.y - half, width: 2 * half, height: 2 * half }, box);
  try {
    const note = await add(tab, index, newAnnotation("note", { kind: "note" }, rect));
    tab.selectedAnnotation = { page: index, id: note.id };
    editing = note.id;
    // After a note is placed, go back to selecting (like most PDF editors).
    annotate.tool = null;
  } catch (err) {
    fail(err);
  }
}

// --- Signatures ------------------------------------------------------------

/** Where the signature being placed follows the pointer (px), or null off the page. */
let ghostAt = $state<{ x: number; y: number } | null>(null);

/** The size a signature is placed at: about 150 pt wide, at most 60 pt tall, never wider than half the page. */
function signatureBox(at: { x: number; y: number }) {
  const sig = annotate.signature;
  if (!sig) return null;
  const aspect = sig.height / sig.width;
  let w = Math.min(150 * scale, width * 0.5);
  if (w * aspect > 60 * scale) w = (60 * scale) / aspect;
  const h = w * aspect;
  return { left: at.x - w / 2, top: at.y - h / 2, width: w, height: h };
}

let ghost = $derived(tool === "signature" && ghostAt ? signatureBox(ghostAt) : null);

async function putSignature(p: { x: number; y: number }) {
  const sig = annotate.signature;
  const r = signatureBox(p);
  if (!sig || !r) return;
  try {
    const placed = await placeSignature(tab, index, sig, pxToRect(r, box), r, (x, y) => fromPx(x, y, box), scale);
    annotate.tool = null;
    ghostAt = null;
    // Selected, so it can be moved or resized right away.
    tab.selectedAnnotation = { page: index, id: placed.id };
  } catch (err) {
    fail(err);
  }
}

// --- Selecting, moving and resizing -------------------------------------

type Drag = {
  pointerId: number;
  start: { x: number; y: number };
  original: Annotation;
  handle: Handle | "move" | "from" | "to";
  preview: Annotation;
  movedEnough: boolean;
};
let drag = $state<Drag | null>(null);

/** Pointer down on a movable annotation (Select mode): select it and start dragging. */
function onShapeDown(e: PointerEvent, a: Annotation) {
  if (e.button !== 0 || drawingTool) return;
  e.preventDefault();
  e.stopPropagation();
  tab.selection = null;
  tab.selectedAnnotation = { page: index, id: a.id };
  if (!canMove(a)) return;
  startDrag(e, a, "move");
}

function startDrag(e: PointerEvent, a: Annotation, handle: Drag["handle"]) {
  e.preventDefault();
  e.stopPropagation();
  layer.setPointerCapture(e.pointerId);
  drag = { pointerId: e.pointerId, start: pointer(e), original: a, handle, preview: a, movedEnough: false };
}

function onDragMove(e: PointerEvent) {
  if (!drag || e.pointerId !== drag.pointerId) return;
  const p = pointer(e);
  const dx = p.x - drag.start.x;
  const dy = p.y - drag.start.y;
  if (!drag.movedEnough && Math.hypot(dx, dy) < 3) return;
  drag.movedEnough = true;
  const a = drag.original;
  if (drag.handle === "move") {
    const from = fromPx(drag.start.x, drag.start.y, box);
    const to = fromPx(p.x, p.y, box);
    drag.preview = moved(a, to.x - from.x, to.y - from.y);
  } else if ((drag.handle === "from" || drag.handle === "to") && a.kind.kind === "line") {
    const point = fromPx(p.x, p.y, box);
    const kind = { ...a.kind, [drag.handle]: point };
    drag.preview = { ...a, kind, rect: rectFrom(kind.from, kind.to) };
  } else if (drag.handle !== "from" && drag.handle !== "to") {
    const r = dragHandle(rectToPx(a.rect, box), drag.handle, dx, dy);
    drag.preview = resized(a, pxToRect(r, box));
  }
}

async function onDragUp(e: PointerEvent) {
  if (!drag || e.pointerId !== drag.pointerId) return;
  layer.releasePointerCapture(e.pointerId);
  const { original, preview, movedEnough } = drag;
  drag = null;
  if (!movedEnough) return;
  try {
    await update(tab, index, original, preview);
  } catch (err) {
    fail(err);
  }
}

function onPointerDown(e: PointerEvent) {
  if (drawingTool) onLayerDown(e);
}

function onPointerMove(e: PointerEvent) {
  if (drag) onDragMove(e);
  else onLayerMove(e);
}

function onPointerUp(e: PointerEvent) {
  if (drag) onDragUp(e);
  else onLayerUp(e);
}

// --- The selected annotation's menu --------------------------------------

/** The id of the annotation whose comment is being edited. */
let editing = $state<string | null>(null);
let commentText = $state("");
$effect(() => {
  if (editing && selected?.id === editing) commentText = untrack(() => selected?.contents ?? "");
});
// Selecting a note opens its text (once per selection, so closing it stays closed).
let lastSelected: string | null = null;
$effect(() => {
  const id = selected?.id ?? null;
  if (id === lastSelected) return;
  lastSelected = id;
  if (!selected) editing = null;
  else if (selected.kind.kind === "note" && selected.editable) editing = selected.id;
});

async function recolor(a: Annotation, color: Color) {
  const kind = a.kind;
  let next: Annotation = { ...a, color };
  // A filled shape keeps a lighter fill of its new colour.
  if ((kind.kind === "square" || kind.kind === "circle") && kind.fill) {
    next = { ...next, kind: { ...kind, fill: lighter(color) } };
  }
  try {
    await update(tab, index, a, next);
  } catch (err) {
    fail(err);
  }
}

async function saveComment(a: Annotation) {
  editing = null;
  if (commentText === a.contents) return;
  try {
    await update(tab, index, a, { ...a, contents: commentText });
  } catch (err) {
    fail(err);
  }
}

/** px of the selection frame and its handles. */
let frame = $derived.by(() => {
  const a = drag?.preview ?? selected;
  if (!a) return null;
  return { a, r: rectToPx(a.rect, box) };
});

const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
function handlePos(h: Handle, r: { left: number; top: number; width: number; height: number }) {
  const x = h.includes("w") ? r.left : h.includes("e") ? r.left + r.width : r.left + r.width / 2;
  const y = h.includes("n") ? r.top : h.includes("s") ? r.top + r.height : r.top + r.height / 2;
  return { x, y };
}

/** Width of the menu, measured, to keep it inside the page. */
let menuWidth = $state(260);

/**
 * Where the menu goes: above the annotation, or below it near the top of the
 * page. The comment box goes under the annotation (or under the menu).
 */
let menuPos = $derived.by(() => {
  if (!frame) return null;
  const { r } = frame;
  const below = r.top < 52;
  const y = below ? r.top + r.height + 8 : r.top - 44;
  return {
    x: Math.max(4, Math.min(r.left, width - menuWidth - 4)),
    y,
    commentX: Math.max(4, Math.min(r.left, width - 244)),
    commentY: below ? y + 44 : r.top + r.height + 8,
  };
});

// --- Drawing previews ------------------------------------------------------

const rgb = (c: Color) => toHex(c);

function strokePoints(points: PagePoint[]) {
  return points
    .map((p) => toPx(p, box))
    .map((p) => `${p.x},${p.y}`)
    .join(" ");
}

let draftStyle = $derived(tool ? annotate.style(tool) : null);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="annot-layer"
  class:drawing={drawingTool}
  class:eraser={tool === "eraser"}
  bind:this={layer}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onpointerleave={() => (ghostAt = null)}
>
  <svg {width} {height} aria-hidden="true">
    <!-- Invisible shapes to click: drawings, shapes and notes. Text markup is
         clicked through the text (see Viewer). -->
    {#if !drawingTool}
      {#each list as a (a.id)}
        {#if a.kind.kind === "ink"}
          {#each a.kind.strokes as stroke, i (i)}
            <polyline class="hit stroke" points={strokePoints(stroke)} onpointerdown={(e) => onShapeDown(e, a)} />
          {/each}
        {:else if a.kind.kind === "line"}
          <polyline class="hit stroke" points={strokePoints([a.kind.from, a.kind.to])}
            onpointerdown={(e) => onShapeDown(e, a)} />
        {:else if a.kind.kind === "square" || a.kind.kind === "circle" || a.kind.kind === "note" || a.kind.kind === "stamp"}
          {@const r = rectToPx(a.rect, box)}
          {#if a.kind.kind === "circle"}
            <ellipse class="hit" class:stroke={!a.kind.fill} class:fill={!!a.kind.fill}
              cx={r.left + r.width / 2} cy={r.top + r.height / 2} rx={r.width / 2} ry={r.height / 2}
              onpointerdown={(e) => onShapeDown(e, a)} />
          {:else}
            <rect class="hit" class:stroke={a.kind.kind === "square" && !a.kind.fill}
              class:fill={a.kind.kind !== "square" || !!a.kind.fill}
              x={r.left} y={r.top} width={r.width} height={r.height}
              onpointerdown={(e) => onShapeDown(e, a)} />
          {/if}
        {/if}
      {/each}
    {/if}

    <!-- What is being drawn. -->
    {#if draft && draftStyle}
      {@const color = rgb(draftStyle.color)}
      {@const w = Math.max(1, draftStyle.width * scale)}
      {#if draft.tool === "pen"}
        <polyline class="preview" points={draft.points.map((p) => `${p.x},${p.y}`).join(" ")}
          stroke={color} stroke-width={w} opacity={draftStyle.opacity} />
      {:else if draft.tool === "line" || draft.tool === "arrow"}
        <line class="preview" x1={draft.from.x} y1={draft.from.y} x2={draft.to.x} y2={draft.to.y}
          stroke={color} stroke-width={w} opacity={draftStyle.opacity} />
      {:else}
        {@const x = Math.min(draft.from.x, draft.to.x)}
        {@const y = Math.min(draft.from.y, draft.to.y)}
        {@const dw = Math.abs(draft.to.x - draft.from.x)}
        {@const dh = Math.abs(draft.to.y - draft.from.y)}
        {@const fill = draftStyle.fill ? rgb(lighter(draftStyle.color)) : "none"}
        {#if draft.tool === "rectangle"}
          <rect class="preview" {x} {y} width={dw} height={dh} stroke={color} stroke-width={w} {fill}
            opacity={draftStyle.opacity} />
        {:else}
          <ellipse class="preview" cx={x + dw / 2} cy={y + dh / 2} rx={dw / 2} ry={dh / 2} stroke={color}
            stroke-width={w} {fill} opacity={draftStyle.opacity} />
        {/if}
      {/if}
    {/if}

    <!-- The signature being placed, following the pointer. -->
    {#if ghost && annotate.signature}
      {@const sig = annotate.signature}
      {#if sig.kind === "image"}
        <image class="ghost" href={sig.png} x={ghost.left} y={ghost.top} width={ghost.width} height={ghost.height}
          preserveAspectRatio="none" />
      {:else}
        {@const f = ghost.width / sig.width}
        {#each sig.strokes as stroke, i (i)}
          <polyline class="preview ghost"
            points={stroke.map((p) => `${ghost.left + p.x * f},${ghost.top + p.y * f}`).join(" ")}
            stroke={toHex(sig.color)} stroke-width={sig.lineWidth * f} />
        {/each}
      {/if}
    {/if}

    <!-- Where a dragged annotation will go. -->
    {#if drag?.movedEnough}
      {@const p = drag.preview}
      {@const color = rgb(p.color)}
      {@const w = Math.max(1, p.width * scale)}
      {#if p.kind.kind === "ink"}
        {#each p.kind.strokes as stroke, i (i)}
          <polyline class="preview" points={strokePoints(stroke)} stroke={color} stroke-width={w} />
        {/each}
      {:else if p.kind.kind === "line"}
        <polyline class="preview" points={strokePoints([p.kind.from, p.kind.to])} stroke={color} stroke-width={w} />
      {:else}
        {@const r = rectToPx(p.rect, box)}
        {#if p.kind.kind === "circle"}
          <ellipse class="preview" cx={r.left + r.width / 2} cy={r.top + r.height / 2} rx={r.width / 2}
            ry={r.height / 2} stroke={color} stroke-width={w} fill={p.kind.fill ? rgb(p.kind.fill) : "none"} />
        {:else}
          <rect class="preview" x={r.left} y={r.top} width={r.width} height={r.height} stroke={color}
            stroke-width={w} fill={p.kind.kind === "square" && p.kind.fill ? rgb(p.kind.fill) : "none"} />
        {/if}
      {/if}
    {/if}

    <!-- The selection frame and its handles. -->
    {#if frame}
      {@const { a, r } = frame}
      <rect class="frame" x={r.left - 3} y={r.top - 3} width={r.width + 6} height={r.height + 6} />
      {#if canMove(a) && !drawingTool}
        {#if a.kind.kind === "line"}
          {#each [["from", a.kind.from], ["to", a.kind.to]] as const as [end, point] (end)}
            {@const p = toPx(point, box)}
            <circle class="handle" cx={p.x} cy={p.y} r="5" onpointerdown={(e) => startDrag(e, a, end)} />
          {/each}
        {:else if !keepsSize(a)}
          {#each HANDLES as h (h)}
            {@const p = handlePos(h, { left: r.left - 3, top: r.top - 3, width: r.width + 6, height: r.height + 6 })}
            <rect class="handle {h}" x={p.x - 4} y={p.y - 4} width="8" height="8"
              onpointerdown={(e) => startDrag(e, a, h)} />
          {/each}
        {/if}
      {/if}
    {/if}
  </svg>

  {#if selected && menuPos && !drag}
    {@const a = selected}
    <div class="annot-menu" style:left="{menuPos.x}px" style:top="{menuPos.y}px" bind:clientWidth={menuWidth}>
      {#if a.editable}
        {#each PALETTE as color (toHex(color))}
          <button class="swatch" style:background={toHex(color)} aria-pressed={toHex(color) === toHex(a.color)}
            aria-label={toHex(color)} onclick={() => recolor(a, color)}></button>
        {/each}
        <span class="sep" aria-hidden="true"></span>
        <button class="icon" title={i18n.t("annot-comment")} aria-label={i18n.t("annot-comment")}
          aria-pressed={editing === a.id} onclick={() => (editing = editing === a.id ? null : a.id)}>
          <Icon name="comment" />
        </button>
      {/if}
      <button class="icon" title={i18n.t("annot-delete")} aria-label={i18n.t("annot-delete")} data-shortcut="Delete"
        onclick={() => remove(tab, index, a).catch(fail)}>
        <Icon name="trash" />
      </button>
    </div>
    {#if editing === a.id}
      <textarea
        class="comment"
        style:left="{menuPos.commentX}px"
        style:top="{menuPos.commentY}px"
        dir="auto"
        placeholder={i18n.t("annot-comment-placeholder")}
        bind:value={commentText}
        {@attach (el) => el.focus({ preventScroll: true })}
        onblur={() => saveComment(a)}
        onkeydown={(e) => {
          if (e.key === "Escape") {
            commentText = a.contents;
            editing = null;
          }
          e.stopPropagation();
        }}
      ></textarea>
    {/if}
  {:else if selected && !selected.editable && selected.contents && menuPos}
    <p class="comment readonly" dir="auto" style:left="{menuPos.commentX}px" style:top="{menuPos.commentY}px">
      {selected.contents}
    </p>
  {/if}
</div>

<style>
  .annot-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .annot-layer.drawing {
    pointer-events: auto;
    cursor: crosshair;
    touch-action: none;
  }
  .annot-layer.eraser {
    cursor: cell;
  }
  .ghost {
    opacity: 0.7;
    pointer-events: none;
  }
  svg {
    position: absolute;
    inset: 0;
    overflow: visible;
  }
  .hit {
    fill: none;
    stroke: transparent;
    cursor: move;
    pointer-events: none;
  }
  .hit.stroke {
    stroke-width: 12;
    stroke-linecap: round;
    pointer-events: stroke;
  }
  .hit.fill {
    fill: transparent;
    pointer-events: all;
  }
  .preview {
    fill: none;
    stroke-linecap: round;
    stroke-linejoin: round;
    pointer-events: none;
  }
  .frame {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
    pointer-events: none;
  }
  .handle {
    fill: var(--surface);
    stroke: var(--accent);
    stroke-width: 1.5;
    pointer-events: all;
  }
  .handle.nw,
  .handle.se {
    cursor: nwse-resize;
  }
  .handle.ne,
  .handle.sw {
    cursor: nesw-resize;
  }
  .handle.n,
  .handle.s {
    cursor: ns-resize;
  }
  .handle.e,
  .handle.w {
    cursor: ew-resize;
  }
  circle.handle {
    cursor: move;
  }
  .annot-menu {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.18);
    pointer-events: auto;
    z-index: 2;
  }
  .swatch {
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid rgb(0 0 0 / 0.2);
  }
  .swatch[aria-pressed="true"] {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .annot-menu .icon {
    width: 28px;
    height: 28px;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border);
  }
  .comment {
    position: absolute;
    width: 240px;
    height: 96px;
    margin: 0;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.18);
    pointer-events: auto;
    resize: none;
    z-index: 2;
    font: inherit;
    font-size: 13px;
  }
  .comment.readonly {
    height: auto;
    max-height: 160px;
    overflow: auto;
    white-space: pre-wrap;
  }
</style>
