<script lang="ts">
// Shows one PDF page. It re-renders when the page or zoom changes,
// at the screen's real pixel density so text stays sharp.
import type { PageSize } from "./bindings/PageSize";
import { isRivetError, type RivetError, renderPage } from "./pdf";

interface Props {
  docId: number;
  page: number;
  size: PageSize;
  zoom: number;
  onerror?: (e: RivetError) => void;
}
let { docId, page, size, zoom, onerror }: Props = $props();

// 100% zoom shows the page at its physical size on a 96 DPI screen.
const CSS_PX_PER_PT = 96 / 72;
let canvas: HTMLCanvasElement;
let cssWidth = $derived(size.width * zoom * CSS_PX_PER_PT);
let cssHeight = $derived(size.height * zoom * CSS_PX_PER_PT);

$effect(() => {
  const scale = zoom * CSS_PX_PER_PT * window.devicePixelRatio;
  const controller = new AbortController();
  renderPage(docId, page, scale, controller.signal)
    .then(({ width, height, data }) => {
      canvas.width = width;
      canvas.height = height;
      canvas.getContext("2d")?.putImageData(data, 0, 0);
    })
    .catch((e) => {
      if (controller.signal.aborted) return;
      onerror?.(isRivetError(e) ? e : { code: "internal", detail: String(e) });
    });
  // A newer render request cancels this one.
  return () => controller.abort();
});
</script>

<canvas bind:this={canvas} style:width="{cssWidth}px" style:height="{cssHeight}px"></canvas>

<style>
  canvas {
    display: block;
    background: white;
    box-shadow: var(--shadow-page);
  }
</style>
