<script lang="ts">
// Every annotation in the document with its comment and replies, page by
// page, on the end side of the window. Click one to show it on its page;
// write, change and answer comments here. Filter by kind, author or text.

import { remove, reply, setComment } from "./annotate.svelte";
import type { Annotation } from "./bindings/Annotation";
import CommentCard from "./CommentCard.svelte";
import {
  authorsOf,
  buildThreads,
  type CommentKind,
  commentsOf,
  emptyFilter,
  filterThreads,
  isFiltering,
  type Thread,
} from "./comments.svelte";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import type { FractionRect } from "./layout";
import { type RivetError, toRivetError } from "./pdf";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  width: number;
  /** Shows an area of a page. */
  onreveal: (page: number, rect: FractionRect) => void;
  onclose: () => void;
  onerror?: (e: RivetError) => void;
}
let { tab, width, onreveal, onclose, onerror }: Props = $props();

let store = $derived(commentsOf(tab));
$effect(() => {
  store.load();
});
// Pages whose annotations change are read again.
$effect(() => {
  store.sync();
});

let threads = $derived(buildThreads(store.pages));
let filter = $state(emptyFilter());
let shown = $derived(filterThreads(threads, filter));
let authors = $derived(authorsOf(threads));
let canEdit = $derived(tab.info.canAnnotate);
let canReply = $derived(tab.info.canReply);

const KINDS: {
  kind: CommentKind;
  icon: "highlight" | "underline" | "strikeout" | "pen" | "rectangle" | "note" | "text" | "signature";
}[] = [
  { kind: "highlight", icon: "highlight" },
  { kind: "underline", icon: "underline" },
  { kind: "strikeout", icon: "strikeout" },
  { kind: "ink", icon: "pen" },
  { kind: "shape", icon: "rectangle" },
  { kind: "note", icon: "note" },
  { kind: "text", icon: "text" },
  { kind: "stamp", icon: "signature" },
];

function toggleKind(kind: CommentKind) {
  const kinds = new Set(filter.kinds);
  if (kinds.has(kind)) kinds.delete(kind);
  else kinds.add(kind);
  filter = { ...filter, kinds };
}

function select(t: Thread) {
  const target = t.annotation;
  // Replies aren't on the page; their comment is.
  const shownOnPage = target.replyTo ? null : target;
  tab.selectedAnnotation = shownOnPage ? { page: t.page, id: shownOnPage.id } : null;
  onreveal(t.page, target.rect);
}

async function run<T>(work: () => Promise<T>): Promise<T | undefined> {
  try {
    return await work();
  } catch (e) {
    onerror?.(toRivetError(e));
  }
}

const pageAspect = (page: number) => {
  const size = tab.info.pageSizes[page];
  return size ? size.width / size.height : 1;
};

/** Pages in the list, for their headings. */
function startsPage(i: number): boolean {
  return i === 0 || shown[i - 1].page !== shown[i].page;
}
</script>

<aside class="comments" style:width="{width}px" aria-label={i18n.t("comments")}>
  <header>
    <h2>{i18n.t("comments")}</h2>
    {#if threads.length}
      <span class="count">{(isFiltering(filter) ? shown.length : threads.length).toLocaleString(i18n.locale)}</span>
    {/if}
    <button class="icon close" onclick={onclose} aria-label={i18n.t("close")} title={i18n.t("close")}>
      <Icon name="close" />
    </button>
  </header>

  <div class="filters">
    <label class="search">
      <Icon name="search" />
      <input type="search" dir="auto" placeholder={i18n.t("comments-search")} aria-label={i18n.t("comments-search")}
        value={filter.text} oninput={(e) => (filter = { ...filter, text: e.currentTarget.value })} />
    </label>
    <div class="row">
      <div class="kinds" role="group" aria-label={i18n.t("comments-kinds")}>
        {#each KINDS as k (k.kind)}
          <button class="icon" aria-pressed={filter.kinds.has(k.kind)} onclick={() => toggleKind(k.kind)}
            aria-label={i18n.t(`comment-kind-${k.kind}`)} title={i18n.t(`comment-kind-${k.kind}`)}>
            <Icon name={k.icon} />
          </button>
        {/each}
      </div>
      {#if authors.length > 1}
        <select aria-label={i18n.t("comments-author")}
          value={filter.authors.size === 1 ? [...filter.authors][0] : ""}
          onchange={(e) => {
            const name = e.currentTarget.value;
            filter = { ...filter, authors: new Set(name ? [name] : []) };
          }}>
          <option value="">{i18n.t("comments-all-authors")}</option>
          {#each authors as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </select>
      {/if}
    </div>
  </div>

  <div class="list">
    {#if threads.length === 0}
      <p class="empty">{store.loading ? i18n.t("comments-loading") : i18n.t("comments-empty")}</p>
    {:else if shown.length === 0}
      <p class="empty">
        {i18n.t("comments-none-match")}
        <button class="link" onclick={() => (filter = emptyFilter())}>{i18n.t("comments-clear-filter")}</button>
      </p>
    {:else}
      {#each shown as thread, i (`${thread.page}/${thread.annotation.id}`)}
        {#if startsPage(i)}
          <h3>{i18n.t("page-n", { page: thread.page + 1 })}</h3>
        {/if}
        <CommentCard
          {thread}
          docId={tab.docId}
          aspect={pageAspect(thread.page)}
          selected={tab.selectedAnnotation?.page === thread.page && tab.selectedAnnotation.id === thread.annotation.id}
          {canEdit}
          {canReply}
          onselect={() => select(thread)}
          oncomment={(a: Annotation, text: string) => run(() => setComment(tab, thread.page, a, text))}
          onreply={(text: string) => run(() => reply(tab, thread.page, thread.annotation, text))}
          ondelete={(a: Annotation) => run(() => remove(tab, thread.page, a))}
        />
      {/each}
      {#if store.loading}
        <p class="empty">{i18n.t("comments-loading")}</p>
      {/if}
    {/if}
  </div>
</aside>

<style>
  .comments {
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--chrome);
    border-inline-start: 1px solid var(--border);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-block: 8px;
    padding-inline: 12px 6px;
    border-block-end: 1px solid var(--border);
  }
  h2 {
    margin: 0;
    font-size: 14px;
  }
  .count {
    padding-inline: 6px;
    border-radius: 10px;
    background: var(--hover);
    font-size: 12px;
    color: var(--muted);
  }
  .close {
    margin-inline-start: auto;
    border: none;
  }
  .filters {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border-block-end: 1px solid var(--border);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-inline: 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--muted);
  }
  .search :global(.icon) {
    flex: none;
    width: 15px;
    height: 15px;
  }
  .search input {
    flex: 1;
    min-width: 0;
    padding-block: 5px;
    border: none;
    background: none;
    color: var(--text);
    font-size: 13px;
    outline: none;
  }
  .search:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .row {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
  }
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
  }
  .kinds button {
    width: 28px;
    height: 28px;
    border-color: transparent;
  }
  .kinds button :global(svg) {
    width: 16px;
    height: 16px;
  }
  .kinds button[aria-pressed="true"] {
    background: var(--accent);
    color: var(--accent-text);
  }
  select {
    width: 100%;
    font-size: 12px;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
  }
  h3 {
    margin-block: 6px 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }
  .empty {
    margin: 8px;
    color: var(--muted);
    font-size: 13px;
  }
  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-size: inherit;
    text-decoration: underline;
  }
</style>
