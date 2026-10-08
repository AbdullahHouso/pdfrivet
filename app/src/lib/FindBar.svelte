<script lang="ts">
// Find in document (Ctrl+F): a small bar at the top of the pages. Typing
// searches as you go; Enter and Shift+Enter (or F3 / Shift+F3) step through
// the results, which are highlighted on the pages.
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  onclose: () => void;
  /** Opens the list of all results in the sidebar. */
  onshowall: () => void;
}
let { tab, onclose, onshowall }: Props = $props();
let search = $derived(tab.search);

let input: HTMLInputElement;

/** Puts the cursor in the box (Ctrl+F while the bar is already open). */
export function focus() {
  input?.focus();
  input?.select();
}

$effect(() => {
  focus();
});

// Search shortly after typing stops, and right away when an option changes.
let timer: ReturnType<typeof setTimeout> | undefined;
function searchSoon(delay = 250) {
  clearTimeout(timer);
  timer = setTimeout(() => search.run(tab.page), delay);
}
$effect(() => () => clearTimeout(timer));

function onKeyDown(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    if (search.query.trim() !== search.searched || (search.hits.length === 0 && !search.running)) {
      clearTimeout(timer);
      search.run(tab.page);
    } else {
      search.step(e.shiftKey ? -1 : 1);
    }
  } else if (e.key === "Escape") {
    e.preventDefault();
    onclose();
  }
}

let status = $derived.by(() => {
  if (!search.searched) return "";
  const total = search.hits.length;
  if (total === 0) return search.running ? i18n.t("search-searching") : i18n.t("search-no-results");
  const count = i18n.t("search-count", { current: search.current + 1, total });
  return search.running ? `${count}…` : count;
});
</script>

<div class="find-bar" role="search">
  <Icon name="search" />
  <input
    bind:this={input}
    bind:value={search.query}
    dir="auto"
    placeholder={i18n.t("search-placeholder")}
    aria-label={i18n.t("search-placeholder")}
    oninput={() => searchSoon()}
    onkeydown={onKeyDown}
  />
  <span class="status" aria-live="polite">{status}</span>
  <button class="icon toggle" aria-pressed={search.matchCase} title={i18n.t("search-match-case")}
    aria-label={i18n.t("search-match-case")}
    onclick={() => {
      search.matchCase = !search.matchCase;
      searchSoon(0);
    }}>
    <span class="glyph" dir="ltr">Aa</span>
  </button>
  <button class="icon toggle" aria-pressed={search.wholeWord} title={i18n.t("search-whole-word")}
    aria-label={i18n.t("search-whole-word")}
    onclick={() => {
      search.wholeWord = !search.wholeWord;
      searchSoon(0);
    }}>
    <span class="glyph word" dir="ltr">ab</span>
  </button>
  <span class="sep" aria-hidden="true"></span>
  <button class="icon" disabled={search.hits.length === 0} onclick={() => search.step(-1)}
    title={i18n.t("search-previous")} aria-label={i18n.t("search-previous")} data-shortcut="Shift+F3">
    <Icon name="chevron-up" />
  </button>
  <button class="icon" disabled={search.hits.length === 0} onclick={() => search.step(1)}
    title={i18n.t("search-next")} aria-label={i18n.t("search-next")} data-shortcut="F3">
    <Icon name="chevron-down" />
  </button>
  <button class="icon" disabled={search.hits.length === 0} onclick={onshowall}
    title={i18n.t("search-show-all")} aria-label={i18n.t("search-show-all")}>
    <Icon name="list" />
  </button>
  <button class="icon" onclick={onclose} title={i18n.t("close")} aria-label={i18n.t("close")} data-shortcut="Escape">
    <Icon name="close" />
  </button>
</div>

<style>
  .find-bar {
    position: absolute;
    inset-block-start: 10px;
    inset-inline-end: 24px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 4px;
    padding-block: 4px;
    padding-inline: 10px 4px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--muted);
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.16);
  }
  input {
    width: 220px;
    border: none;
    background: transparent;
    color: var(--text);
    padding-block: 6px;
    padding-inline: 4px;
    outline: none;
  }
  .status {
    min-width: 4.5em;
    font-size: 12px;
    white-space: nowrap;
    text-align: end;
    font-variant-numeric: tabular-nums;
  }
  .toggle[aria-pressed="true"] {
    color: var(--accent);
    background: var(--hover);
  }
  .glyph {
    font-size: 13px;
    font-weight: 600;
    line-height: 1;
  }
  /* "Whole word": the letters between two ticks, underlined. */
  .glyph.word {
    padding-inline: 2px;
    border-inline: 1.5px solid currentColor;
    border-block-end: 1.5px solid currentColor;
    border-radius: 1px;
  }
  .sep {
    width: 1px;
    height: 20px;
    margin-inline: 2px;
    background: var(--border);
  }
</style>
