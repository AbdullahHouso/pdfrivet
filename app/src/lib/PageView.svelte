<script lang="ts">
// One rendered PDF page. Renders at the screen's real pixel density so text
// stays sharp, and re-renders shortly after the size changes (while zooming
// the old pixels are stretched, so nothing flickers).
import type { Degrees } from "./layout";
import { type RivetError, renderPage, toRivetError } from "./pdf";

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
}
let { docId, index, width, height, widthPt, rotation, thumbnail = false, onerror }: Props = $props();

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
  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
