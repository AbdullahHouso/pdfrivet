// The comments panel's data: every annotation of a document (they all can
// carry a comment), grouped into threads with their replies. Loaded in the
// background a batch of pages at a time, and read again for any page whose
// annotations change.

import { SvelteMap } from "svelte/reactivity";
import type { Annotation } from "./bindings/Annotation";
import type { FractionRect } from "./layout";
import { getAnnotations, getAnnotationsFrom } from "./pdf";
import type { Tab } from "./tabs.svelte";
import { boxAt, codeAt, hasBox, type PageText } from "./textSelect";

/** What kind of annotation, as the panel shows and filters them. */
export type CommentKind = "highlight" | "underline" | "strikeout" | "ink" | "shape" | "note" | "stamp" | "other";

export function commentKind(a: Annotation): CommentKind {
  switch (a.kind.kind) {
    case "markup":
      return a.kind.style === "squiggly" ? "underline" : a.kind.style;
    case "ink":
      return "ink";
    case "square":
    case "circle":
    case "line":
      return "shape";
    case "note":
      return "note";
    case "stamp":
      return "stamp";
    default:
      return "other";
  }
}

/** An annotation with the replies to its comment (oldest first). */
export interface Thread {
  page: number;
  annotation: Annotation;
  replies: Annotation[];
}

const time = (a: Annotation) => (a.modified ? Date.parse(a.modified) : 0) || 0;

/** Threads in reading order: by page, then from the top of the page. */
export function buildThreads(pages: Iterable<[number, Annotation[]]>): Thread[] {
  const threads: Thread[] = [];
  for (const [page, list] of pages) {
    const byId = new Map<string, Thread>();
    const onPage: Thread[] = [];
    for (const a of list) {
      if (a.replyTo) continue;
      const thread = { page, annotation: a, replies: [] };
      byId.set(a.id, thread);
      onPage.push(thread);
    }
    for (const a of list) {
      if (!a.replyTo) continue;
      const parent = byId.get(a.replyTo);
      // A reply whose annotation is gone shows on its own.
      if (parent) parent.replies.push(a);
      else onPage.push({ page, annotation: a, replies: [] });
    }
    for (const t of onPage) t.replies.sort((x, y) => time(x) - time(y));
    onPage.sort(
      (x, y) => x.annotation.rect.top - y.annotation.rect.top || x.annotation.rect.left - y.annotation.rect.left,
    );
    threads.push(...onPage);
  }
  return threads.sort((x, y) => x.page - y.page);
}

export interface CommentFilter {
  /** Kinds to show (empty: all). */
  kinds: Set<CommentKind>;
  /** Authors to show (empty: all). */
  authors: Set<string>;
  /** Text to look for in comments, replies and authors. */
  text: string;
  /** Only annotations that have a comment or replies. */
  withText: boolean;
}

export function emptyFilter(): CommentFilter {
  return { kinds: new Set(), authors: new Set(), text: "", withText: false };
}

export function isFiltering(f: CommentFilter): boolean {
  return f.kinds.size > 0 || f.authors.size > 0 || f.text.trim() !== "" || f.withText;
}

export function filterThreads(threads: Thread[], f: CommentFilter): Thread[] {
  const needle = f.text.trim().toLocaleLowerCase();
  return threads.filter(({ annotation: a, replies }) => {
    if (f.kinds.size && !f.kinds.has(commentKind(a))) return false;
    if (f.authors.size && ![a, ...replies].some((x) => f.authors.has(x.author))) return false;
    if (f.withText && !a.contents.trim() && replies.length === 0) return false;
    if (needle) {
      const haystack = [a, ...replies].map((x) => `${x.contents}\n${x.author}`).join("\n");
      if (!haystack.toLocaleLowerCase().includes(needle)) return false;
    }
    return true;
  });
}

/** Everyone who wrote something, for the author filter. */
export function authorsOf(threads: Thread[]): string[] {
  const names = new Set<string>();
  for (const t of threads) for (const a of [t.annotation, ...t.replies]) if (a.author) names.add(a.author);
  return [...names].sort((a, b) => a.localeCompare(b));
}

/** The text a highlight, underline or strikeout marks: the characters whose centre is inside one of its quads. */
export function markedText(text: PageText, quads: FractionRect[]): string {
  let out = "";
  let started = false;
  for (let i = 0; i < text.count; i++) {
    if (!hasBox(text, i)) {
      if (started) out += " ";
      continue;
    }
    const b = boxAt(text, i);
    const x = (b.left + b.right) / 2;
    const y = (b.top + b.bottom) / 2;
    if (quads.some((q) => x >= q.left && x <= q.right && y >= q.top && y <= q.bottom)) {
      out += String.fromCodePoint(codeAt(text, i));
      started = true;
    } else if (started) {
      out += " ";
    }
  }
  return out.replace(/\s+/g, " ").trim();
}

/** Every annotation of a document, kept up to date while the panel is open. */
export class DocComments {
  readonly pages = new SvelteMap<number, Annotation[]>();
  /** Still reading pages for the first time. */
  loading = $state(false);
  readonly #tab: Tab;
  /** The page revision each page was read at. */
  readonly #readAt = new Map<number, number>();
  #started = false;

  constructor(tab: Tab) {
    this.#tab = tab;
  }

  /** Reads every page (once), a batch at a time. */
  async load() {
    if (this.#started) return;
    this.#started = true;
    this.loading = true;
    try {
      let next: number | null = 0;
      while (next !== null) {
        const from: number = next;
        const batch = await getAnnotationsFrom(this.#tab.docId, from);
        const end = batch.nextPage ?? this.#tab.info.pageCount;
        const found = new Map(batch.pages.map((p) => [p.page, p.annotations]));
        for (let page = from; page < end; page++) {
          // A page changed while this batch was read is read again by `sync`.
          if (this.#readAt.has(page)) continue;
          this.#readAt.set(page, -1);
          const list = found.get(page);
          if (list) this.pages.set(page, list);
        }
        next = batch.nextPage;
      }
      this.sync();
    } catch (e) {
      console.warn("[comments] could not read annotations", e);
    } finally {
      this.loading = false;
    }
  }

  /** Reads again the pages whose annotations changed. Reactive: call it from an effect. */
  sync() {
    for (const [page, revision] of this.#tab.pageRevisions) {
      if (this.#readAt.get(page) === revision) continue;
      this.#readAt.set(page, revision);
      getAnnotations(this.#tab.docId, page)
        .then((list) => {
          if (this.#readAt.get(page) !== revision) return;
          if (list.length) this.pages.set(page, list);
          else this.pages.delete(page);
        })
        .catch(() => {});
    }
  }
}

const stores = new WeakMap<Tab, DocComments>();

/** The comments of a tab's document (one store per tab, kept while it's open). */
export function commentsOf(tab: Tab): DocComments {
  let store = stores.get(tab);
  if (!store) {
    store = new DocComments(tab);
    stores.set(tab, store);
  }
  return store;
}
