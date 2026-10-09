<script lang="ts">
// One rendered PDF page. Renders at the screen's real pixel density so text
// stays sharp, and re-renders shortly after the size changes (while zooming
// the old pixels are stretched, so nothing flickers).
import { openUrl } from "@tauri-apps/plugin-opener";
import { onDestroy } from "svelte";
import AnnotationLayer from "./AnnotationLayer.svelte";
import type { LinkTarget } from "./bindings/LinkTarget";
import type { PageLink } from "./bindings/PageLink";
import FormLayer from "./FormLayer.svelte";
import { i18n } from "./i18n.svelte";
import { type Degrees, rotateRect } from "./layout";
import { keep, take } from "./pageCanvasCache";
import { getLinks, type PagePixels, type RivetError, renderPage, toRivetError } from "./pdf";
import type { SearchMark } from "./search.svelte";
import TextLayer from "./TextLayer.svelte";
import type { Tab } from "./tabs.svelte";

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
  /** Goes up when the document changes; the page then re-renders. */
  revision?: number;
  /** Called after the user changed a form field on this page. */
  onfieldchange?: () => void;
  /** Selected characters on this page: [start, end), or null. */
  selected?: [number, number] | null;
  /** Search results on this page. */
  hits?: SearchMark[];
  /** The tab, for drawing and editing annotations (not for thumbnails). */
  tab?: Tab;
}
let {
  docId,
  index,
  width,
  height,
  widthPt,
  rotation,
  thumbnail = false,
  onerror,
  ongotopage,
  revision = 0,
  onfieldchange,
  selected = null,
  hits = [],
  tab,
}: Props = $props();

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

// Leaving the screen: keep the pixels for a while (see pageCanvasCache.ts).
onDestroy(() => {
  if (!thumbnail && rendered && paintedScale > 0 && paintedRotation !== null) {
    keep(docId, index, paintedRotation, paintedRevision, canvas, paintedScale);
  }
});
// The scale that is currently painted, to avoid re-rendering for nothing.
let paintedScale = 0;
let paintedRotation: Degrees | null = null;
let paintedRevision = -1;

function paint(pixels: { width: number; height: number; data: ImageData }) {
  canvas.width = pixels.width;
  canvas.height = pixels.height;
  canvas.getContext("2d")?.putImageData(pixels.data, 0, 0);
  rendered = true;
}

$effect(() => {
  const scale = (width * window.devicePixelRatio) / widthPt;
  const rot = rotation;
  const rev = revision;
  const sameSize = Math.abs(scale - paintedScale) < 0.01 && rot === paintedRotation;
  if (sameSize && rev === paintedRevision) return;

  // Shown a moment ago (or prepared ahead): paint those pixels now, in this
  // frame, so the empty page never shows.
  if (!rendered && !thumbnail) {
    const kept = take(docId, index, rot, rev, scale);
    if (kept) {
      canvas.width = kept.canvas.width;
      canvas.height = kept.canvas.height;
      canvas.getContext("2d")?.drawImage(kept.canvas, 0, 0);
      rendered = true;
      paintedScale = kept.scale;
      paintedRotation = rot;
      paintedRevision = rev;
      tab?.markPainted(index, rev);
      return;
    }
  }

  const controller = new AbortController();
  const signal = controller.signal;
  const run = async () => {
    try {
      // First time: if the sharp page was rendered before (scrolling back to it),
      // show it straight away. Otherwise show a quick low-resolution preview
      // while the sharp page renders.
      let pixels: PagePixels | null = null;
      if (!rendered && !thumbnail) {
        pixels = await renderPage(docId, index, { scale, rotation: rot, cachedOnly: true, signal });
        if (!pixels && scale > 0.4 && !signal.aborted) {
          const preview = await renderPage(docId, index, { scale: 0.25, rotation: rot, signal });
          if (preview && !signal.aborted) paint(preview);
        }
      }
      pixels ??= await renderPage(docId, index, { scale, rotation: rot, thumbnail, signal });
      if (!pixels || signal.aborted) return;
      paint(pixels);
      paintedScale = scale;
      paintedRotation = rot;
      paintedRevision = rev;
      // Previews of an edit wait for this (see Tab.whenPainted).
      if (!thumbnail) tab?.markPainted(index, rev);
    } catch (e) {
      if (!signal.aborted) onerror?.(toRivetError(e));
    }
  };
  // Render right away the first time and after an edit; while zooming, wait until it settles.
  const timer = setTimeout(run, rendered && !sameSize ? 120 : 0);
  return () => {
    clearTimeout(timer);
    controller.abort();
  };
});
</script>

<div class="page" class:loading={!rendered} style:width="{width}px" style:height="{height}px">
  <canvas bind:this={canvas}></canvas>
  {#if !thumbnail}
    <TextLayer {docId} {index} {rotation} {selected} {hits} />
  {/if}
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
  {#if tab && !thumbnail}
    <AnnotationLayer {tab} {index} {width} {height} scale={width / widthPt} {rotation} {onerror} />
  {/if}
  {#if !thumbnail && onfieldchange}
    <FormLayer {docId} {index} {rotation} pageHeight={height} {revision} onchanged={onfieldchange} {onerror} />
  {/if}
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
