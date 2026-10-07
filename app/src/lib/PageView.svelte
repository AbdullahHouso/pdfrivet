<script lang="ts">
// One rendered PDF page. Renders at the screen's real pixel density so text
// stays sharp, and re-renders shortly after the size changes (while zooming
// the old pixels are stretched, so nothing flickers).
import { openUrl } from "@tauri-apps/plugin-opener";
import type { LinkTarget } from "./bindings/LinkTarget";
import type { PageLink } from "./bindings/PageLink";
import { i18n } from "./i18n.svelte";
import { type Degrees, rotateRect } from "./layout";
import { getLinks, type RivetError, renderPage, toRivetError } from "./pdf";

interface Props {
  docId: number;
  index: number;
  /** Size on screen, in CSS px. */
  width: number;
  height: number;
  /** Page width in PDF points after rotation (to compute the render scale). */
  widthPt: number;
  rotation: Degrees;
  thumbnail?: boolean;
  onerror?: (e: RivetError) => void;
  /** Called when a link to another page in this document is clicked. */
  ongotopage?: (page: number) => void;
}
let { docId, index, width, height, widthPt, rotation, thumbnail = false, onerror, ongotopage }: Props = $props();

// Clickable links on this page (not for thumbnails).
let links = $state<PageLink[]>([]);
$effect(() => {
  if (thumbnail) return;
  let cancelled = false;
  getLinks(docId, index)
    .then((found) => {
      if (!cancelled) links = found;
    })
    .catch(() => {});
  return () => {
    cancelled = true;
  };
});

function follow(target: LinkTarget) {
  if (target.kind === "page") ongotopage?.(target.page);
  else openUrl(target.uri).catch((e) => onerror?.(toRivetError(e)));
}

function linkLabel(target: LinkTarget): string {
  return target.kind === "page" ? i18n.t("go-to-page-n", { page: target.page + 1 }) : target.uri;
}

let canvas: HTMLCanvasElement;
let rendered = $state(false);
// The scale that is currently painted, to avoid re-rendering for nothing.
let paintedScale = 0;
let paintedRotation: Degrees | null = null;

function paint(pixels: { width: number; height: number; data: ImageData }) {
  canvas.width = pixels.width;
  canvas.height = pixels.height;
  canvas.getContext("2d")?.putImageData(pixels.data, 0, 0);
  rendered = true;
}

$effect(() => {
  const scale = (width * window.devicePixelRatio) / widthPt;
  const rot = rotation;
  if (Math.abs(scale - paintedScale) < 0.01 && rot === paintedRotation) return;

  const controller = new AbortController();
  const signal = controller.signal;
  const run = async () => {
    try {
      // First time: show a quick low-resolution preview, then the sharp page.
      if (!rendered && !thumbnail && scale > 0.4) {
        const preview = await renderPage(docId, index, { scale: 0.25, rotation: rot, signal });
        if (preview && !signal.aborted) paint(preview);
      }
      const pixels = await renderPage(docId, index, { scale, rotation: rot, thumbnail, signal });
      if (!pixels || signal.aborted) return;
      paint(pixels);
      paintedScale = scale;
      paintedRotation = rot;
    } catch (e) {
      if (!signal.aborted) onerror?.(toRivetError(e));
    }
  };
  // Render right away the first time; while zooming, wait until it settles.
  const timer = setTimeout(run, rendered ? 120 : 0);
  return () => {
    clearTimeout(timer);
    controller.abort();
  };
});
</script>

<div class="page" class:loading={!rendered} style:width="{width}px" style:height="{height}px">
  <canvas bind:this={canvas}></canvas>
  {#each links as link, i (i)}
    {@const r = rotateRect(link, rotation)}
    <button
      class="link"
      style:left="{r.left * 100}%"
      style:top="{r.top * 100}%"
      style:width="{(r.right - r.left) * 100}%"
      style:height="{(r.bottom - r.top) * 100}%"
      title={linkLabel(link.target)}
      aria-label={linkLabel(link.target)}
      onclick={() => follow(link.target)}
    ></button>
  {/each}
</div>

<style>
  .page {
    position: relative;
    background: white;
    box-shadow: var(--shadow-page);
    overflow: hidden;
  }
  .page.loading {
    background: var(--page-placeholder);
  }
  .link {
    position: absolute;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: transparent;
    cursor: pointer;
  }
  .link:hover,
  .link:focus-visible {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
