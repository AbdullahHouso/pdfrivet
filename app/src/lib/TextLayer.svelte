<script lang="ts">
// Draws the selected text and search results of one page. Also loads the page's text as soon as
// the page is shown, so selecting with the pointer can start without waiting.
import { type Degrees, rotateRect } from "./layout";
import { loadPageText } from "./pageText";
import type { SearchMark } from "./search.svelte";
import { type PageText, selectionRects } from "./textSelect";

interface Props {
  docId: number;
  index: number;
  rotation: Degrees;
  /** Selected characters on this page: [start, end), or null. */
  selected: [number, number] | null;
  /** Search results on this page. */
  hits?: SearchMark[];
}
let { docId, index, rotation, selected, hits = [] }: Props = $props();

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
let hitRects = $derived(
  text
    ? hits.flatMap((hit) =>
        selectionRects(text as PageText, hit.start, hit.end).map((r) => ({ r, current: hit.current })),
      )
    : [],
);
</script>

{#each hitRects as { r: rect, current }, i (i)}
  {@const r = rotateRect(rect, rotation)}
  <div
    class="hit"
    class:current
    style:left="{r.left * 100}%"
    style:top="{r.top * 100}%"
    style:width="{(r.right - r.left) * 100}%"
    style:height="{(r.bottom - r.top) * 100}%"
  ></div>
{/each}

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
  .selection,
  .hit {
    position: absolute;
    background: var(--text-selection);
    pointer-events: none;
  }
  .hit {
    background: var(--search-hit);
  }
  .hit.current {
    background: var(--search-current);
    outline: 1px solid var(--search-current-edge);
  }
</style>
