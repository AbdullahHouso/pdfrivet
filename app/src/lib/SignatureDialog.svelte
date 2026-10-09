<script lang="ts">
// Making a signature: draw it (mouse, pen or finger), or take it from a photo
// or scan, with the paper made transparent. It can be kept for next time.
import type { Color } from "./bindings/Color";
import { i18n } from "./i18n.svelte";
import { dialogOut } from "./motion";
import { contentBounds, fitWithin, normalizeStrokes, removeBackground, suggestThreshold } from "./signatureImage";
import { newSignatureId, type Signature } from "./signatures.svelte";

interface Props {
  /** The new signature, and whether to keep it for later. */
  onuse: (signature: Signature, keep: boolean) => void;
  oncancel: () => void;
}
let { onuse, oncancel }: Props = $props();

let dialog: HTMLDialogElement;
$effect(() => {
  dialog.showModal();
  return () => dialog.close();
});

let mode = $state<"draw" | "image">("draw");
let keep = $state(true);

// --- Drawing ---------------------------------------------------------------

const INKS: { color: Color; label: string }[] = [
  { color: { r: 20, g: 20, b: 24 }, label: "signature-ink-black" },
  { color: { r: 24, g: 52, b: 140 }, label: "signature-ink-blue" },
  { color: { r: 170, g: 30, b: 30 }, label: "signature-ink-red" },
];
const PAD_WIDTH = 520;
const PAD_HEIGHT = 200;
const LINE_WIDTH = 2.6;

let ink = $state(INKS[0].color);
let strokes = $state.raw<{ x: number; y: number }[][]>([]);
let pad: HTMLCanvasElement | undefined = $state();
let drawing: number | null = null;

const css = (c: Color) => `rgb(${c.r} ${c.g} ${c.b})`;

function redraw() {
  if (!pad) return;
  const ctx = pad.getContext("2d");
  if (!ctx) return;
  const dpr = window.devicePixelRatio;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, PAD_WIDTH, PAD_HEIGHT);
  ctx.strokeStyle = css(ink);
  ctx.lineWidth = LINE_WIDTH;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  for (const s of strokes) {
    ctx.beginPath();
    ctx.moveTo(s[0].x, s[0].y);
    for (const p of s.slice(1)) ctx.lineTo(p.x, p.y);
    if (s.length === 1) ctx.lineTo(s[0].x + 0.1, s[0].y);
    ctx.stroke();
  }
}

$effect(() => {
  if (!pad) return;
  pad.width = PAD_WIDTH * window.devicePixelRatio;
  pad.height = PAD_HEIGHT * window.devicePixelRatio;
  redraw();
});
$effect(() => {
  ink;
  strokes;
  redraw();
});

function padPoint(e: PointerEvent) {
  const r = (pad as HTMLCanvasElement).getBoundingClientRect();
  return { x: ((e.clientX - r.left) / r.width) * PAD_WIDTH, y: ((e.clientY - r.top) / r.height) * PAD_HEIGHT };
}

function onPadDown(e: PointerEvent) {
  if (e.button !== 0 && e.pointerType === "mouse") return;
  e.preventDefault();
  pad?.setPointerCapture(e.pointerId);
  drawing = e.pointerId;
  strokes = [...strokes, [padPoint(e)]];
}

function onPadMove(e: PointerEvent) {
  if (drawing !== e.pointerId) return;
  // Pens and touch screens report points between frames too: use them all for smooth curves.
  const events = e.getCoalescedEvents?.() ?? [e];
  const last = strokes[strokes.length - 1];
  strokes = [...strokes.slice(0, -1), [...last, ...events.map(padPoint)]];
}

function onPadUp(e: PointerEvent) {
  if (drawing === e.pointerId) drawing = null;
}

// --- From a picture ----------------------------------------------------------

/** Largest side of a picture signature, in px. */
const MAX_SIDE = 1200;
let source = $state.raw<ImageData | null>(null);
let threshold = $state(190);
let preview: HTMLCanvasElement | undefined = $state();
let cleaned = $state.raw<{ data: ImageData; png: string } | null>(null);
let pictureError = $state(false);

let fileInput: HTMLInputElement | undefined = $state();
let fileName = $state("");

async function choosePicture(e: Event) {
  const file = (e.currentTarget as HTMLInputElement).files?.[0];
  if (!file) return;
  fileName = file.name;
  pictureError = false;
  try {
    const bitmap = await createImageBitmap(file);
    const size = fitWithin(bitmap.width, bitmap.height, MAX_SIDE);
    const canvas = document.createElement("canvas");
    canvas.width = size.width;
    canvas.height = size.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no canvas");
    ctx.drawImage(bitmap, 0, 0, size.width, size.height);
    bitmap.close();
    source = ctx.getImageData(0, 0, size.width, size.height);
    threshold = suggestThreshold(source.data);
  } catch {
    pictureError = true;
    source = null;
  }
}

// Remove the background and crop whenever the picture or the threshold changes.
$effect(() => {
  if (!source) {
    cleaned = null;
    return;
  }
  const data = new Uint8ClampedArray(source.data);
  removeBackground(data, threshold);
  const bounds = contentBounds(data, source.width, source.height);
  if (!bounds) {
    cleaned = null;
    return;
  }
  const full = document.createElement("canvas");
  full.width = source.width;
  full.height = source.height;
  full.getContext("2d")?.putImageData(new ImageData(data, source.width, source.height), 0, 0);
  const cropped = document.createElement("canvas");
  cropped.width = bounds.width;
  cropped.height = bounds.height;
  const ctx = cropped.getContext("2d");
  if (!ctx) return;
  ctx.drawImage(full, bounds.x, bounds.y, bounds.width, bounds.height, 0, 0, bounds.width, bounds.height);
  cleaned = { data: ctx.getImageData(0, 0, bounds.width, bounds.height), png: cropped.toDataURL("image/png") };
});

$effect(() => {
  if (!preview || !cleaned) return;
  preview.width = cleaned.data.width;
  preview.height = cleaned.data.height;
  preview.getContext("2d")?.putImageData(cleaned.data, 0, 0);
});

// --- Done ---------------------------------------------------------------------

let ready = $derived(mode === "draw" ? strokes.length > 0 : cleaned !== null);

function use() {
  if (mode === "draw") {
    const n = normalizeStrokes(strokes);
    // Leave room for the line's width around the strokes.
    const pad = LINE_WIDTH;
    onuse(
      {
        id: newSignatureId(),
        kind: "ink",
        strokes: n.strokes.map((s) => s.map((p) => ({ x: p.x + pad, y: p.y + pad }))),
        width: n.width + 2 * pad,
        height: n.height + 2 * pad,
        color: ink,
        lineWidth: LINE_WIDTH,
      },
      keep,
    );
  } else if (cleaned) {
    onuse(
      { id: newSignatureId(), kind: "image", png: cleaned.png, width: cleaned.data.width, height: cleaned.data.height },
      keep,
    );
  }
}
</script>

<dialog
  out:dialogOut|global
  bind:this={dialog}
  aria-labelledby="signature-title"
  oncancel={(e) => {
    e.preventDefault();
    oncancel();
  }}
>
  <h2 id="signature-title">{i18n.t("signature-new-title")}</h2>

  <div class="switcher" role="tablist">
    <button role="tab" aria-selected={mode === "draw"} onclick={() => (mode = "draw")}>{i18n.t("signature-draw")}</button>
    <button role="tab" aria-selected={mode === "image"} onclick={() => (mode = "image")}>
      {i18n.t("signature-image")}
    </button>
  </div>

  {#if mode === "draw"}
    <div class="pad-wrap">
      <canvas
        class="pad"
        bind:this={pad}
        style:aspect-ratio="{PAD_WIDTH} / {PAD_HEIGHT}"
        aria-label={i18n.t("signature-draw-here")}
        onpointerdown={onPadDown}
        onpointermove={onPadMove}
        onpointerup={onPadUp}
        onpointercancel={onPadUp}
      ></canvas>
      {#if strokes.length === 0}
        <p class="placeholder" aria-hidden="true">{i18n.t("signature-draw-here")}</p>
      {/if}
      <span class="baseline" aria-hidden="true"></span>
    </div>
    <div class="row">
      <div class="inks" role="radiogroup" aria-label={i18n.t("signature-ink")}>
        {#each INKS as option (option.label)}
          <button class="ink" role="radio" aria-checked={css(option.color) === css(ink)}
            style:background={css(option.color)} aria-label={i18n.t(option.label)} title={i18n.t(option.label)}
            onclick={() => (ink = option.color)}></button>
        {/each}
      </div>
      <button disabled={strokes.length === 0} onclick={() => (strokes = [])}>{i18n.t("signature-clear")}</button>
    </div>
  {:else}
    <div class="file">
      <!-- The browser's own file button can't be translated or themed, so it's hidden behind ours. -->
      <input bind:this={fileInput} type="file" accept="image/png,image/jpeg,image/webp,image/bmp" hidden
        onchange={choosePicture} />
      <button onclick={() => fileInput?.click()}>{i18n.t("signature-choose-button")}</button>
      <span dir="auto">{fileName || i18n.t("signature-choose-picture")}</span>
    </div>
    {#if pictureError}
      <p class="error">{i18n.t("signature-picture-error")}</p>
    {/if}
    {#if source}
      <div class="preview" class:empty={!cleaned}>
        {#if cleaned}
          <canvas bind:this={preview}></canvas>
        {:else}
          <p>{i18n.t("signature-picture-empty")}</p>
        {/if}
      </div>
      <label class="threshold">
        <span>{i18n.t("signature-background")}</span>
        <input type="range" min="40" max="250" step="1" bind:value={threshold} />
      </label>
      <p class="hint">{i18n.t("signature-background-hint")}</p>
    {/if}
  {/if}

  <label class="keep">
    <input type="checkbox" bind:checked={keep} />
    {i18n.t("signature-keep")}
  </label>

  <div class="actions">
    <button onclick={oncancel}>{i18n.t("cancel")}</button>
    <button class="primary" disabled={!ready} onclick={use}>{i18n.t("signature-use")}</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(580px, calc(100vw - 32px));
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
  h2 {
    margin: 0 0 12px;
    font-size: 17px;
  }
  .switcher {
    display: flex;
    gap: 4px;
    margin-block-end: 12px;
  }
  .switcher button {
    flex: 1;
    border-color: transparent;
  }
  .switcher button[aria-selected="true"] {
    background: var(--hover);
    border-color: var(--border);
    font-weight: 600;
  }
  /* The pad and the picture preview are paper: always white, in every theme. */
  .pad-wrap {
    position: relative;
  }
  .pad {
    display: block;
    width: 100%;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: #ffffff;
    cursor: crosshair;
    touch-action: none;
  }
  .placeholder {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    color: #9aa1ad;
    pointer-events: none;
  }
  .baseline {
    position: absolute;
    inset-inline: 32px;
    inset-block-end: 28%;
    border-block-end: 1px dashed #c9ccd3;
    pointer-events: none;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-block-start: 10px;
  }
  .inks {
    display: flex;
    gap: 8px;
  }
  .ink {
    width: 24px;
    height: 24px;
    padding: 0;
    border-radius: 50%;
  }
  .ink[aria-checked="true"] {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-block-end: 10px;
  }
  .file span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    color: var(--muted);
  }
  .preview {
    display: grid;
    place-items: center;
    min-height: 140px;
    max-height: 240px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    /* A light checkerboard shows which parts are transparent. */
    background: repeating-conic-gradient(#ffffff 0 25%, #eceef1 0 50%) 0 0 / 16px 16px;
    overflow: hidden;
  }
  .preview canvas {
    max-width: 100%;
    max-height: 210px;
  }
  .preview.empty p {
    color: #5f6470;
  }
  .threshold {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-block-start: 10px;
    font-size: 13px;
  }
  .threshold input {
    flex: 1;
    accent-color: var(--accent);
  }
  .hint,
  .error {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .error {
    color: var(--error-fg);
  }
  .keep {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-block-start: 14px;
    font-size: 13px;
  }
  .keep input {
    accent-color: var(--accent);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-block-start: 16px;
  }
</style>
