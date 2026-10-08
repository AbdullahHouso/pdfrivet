<script lang="ts">
// Draws the selected text of one page. Also loads the page's text as soon as
// the page is shown, so selecting with the pointer can start without waiting.
import { type Degrees, rotateRect } from "./layout";
import { loadPageText } from "./pageText";
import { type PageText, selectionRects } from "./textSelect";

interface Props {
  docId: number;
  index: number;
  rotation: Degrees;
  /** Selected characters on this page: [start, end), or null. */
  selected: [number, number] | null;
}
let { docId, index, rotation, selected }: Props = $props();

let text = $state.raw<PageText | null>(null);
$effect(() => {
  let cancelled = false;
  loadPageText(docId, index)
    .then((t) => {
      if (!cancelled) text = t;
    })
    .catch(() => {});
  return () => {
    cancelled = true;
  };
});

let rects = $derived(text && selected ? selectionRects(text, selected[0], selected[1]) : []);
</script>

{#each rects as rect, i (i)}
  {@const r = rotateRect(rect, rotation)}
  <div
    class="selection"
    style:left="{r.left * 100}%"
    style:top="{r.top * 100}%"
    style:width="{(r.right - r.left) * 100}%"
    style:height="{(r.bottom - r.top) * 100}%"
  ></div>
{/each}

<style>
  .selection {
    position: absolute;
    background: var(--text-selection);
    pointer-events: none;
  }
</style>
