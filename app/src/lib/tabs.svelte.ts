// Open documents ("tabs"). Each tab keeps its own view state, so switching
// tabs brings you back exactly where you were. Only the active tab's pages
// are rendered; background tabs keep their state, not their pixels.

import { SvelteMap } from "svelte/reactivity";
import type { Annotation } from "./bindings/Annotation";
import type { DocInfo } from "./bindings/DocInfo";
import type { PageRect } from "./bindings/PageRect";

/** An annotation lifted off a page's image (see Tab.liftArea). */
export interface Lifted {
  /** Its pixels as drawn on the page (the rest transparent). */
  image: HTMLCanvasElement;
  /** Where `image` sits, as fractions of the page as shown (after view rotation). */
  box: { left: number; top: number; width: number; height: number };
  /** Paints the area without it onto the page. */
  apply: () => void;
}
export type Lifter = (rect: PageRect, hide: () => Promise<void>) => Promise<Lifted | null>;

import { History } from "./history.svelte";
import type { Degrees } from "./layout";
import type { Bookmark } from "./outlineTree";
import { forgetPages } from "./pageCanvasCache";
import { forgetPageText } from "./pageText";
import { closeDocument, dropSnapshots } from "./pdf";
import type { FileView, ZoomMode } from "./recent";
import { DocSearch } from "./search.svelte";
import type { PageTone } from "./settings.svelte";
import type { TextSelection } from "./textSelect";

export type PageLayout = "single" | "double";

/**
 * A tab's view, handed to another window when tabs move between windows
 * (see docWindows.ts). The document itself stays open in the engine, so
 * nothing is reopened and unsaved changes are kept.
 */
export interface TabState {
  docId: number;
  path: string;
  page: number;
  zoom: number;
  zoomMode: ZoomMode;
  rotation: Degrees;
  pageLayout: PageLayout;
  continuous: boolean;
  pagesRtl: boolean;
  pageTone: PageTone;
  dirty: boolean;
}

export class Tab {
  readonly id: number;
  readonly docId: number;
  /** Changes after "Save as". */
  path = $state("");
  /** Replaced when the title changes in Document properties. */
  info = $state<DocInfo>() as DocInfo;
  page = $state(0);
  zoom = $state(1);
  zoomMode = $state<ZoomMode>("fit-width");
  rotation = $state<Degrees>(0);
  /** One page per row, or two side by side. */
  pageLayout = $state<PageLayout>("single");
  /** Scroll through all pages, or show one page (or spread) at a time. */
  continuous = $state(true);
  /** Two-page spreads start on the right (Arabic, Hebrew… documents). */
  pagesRtl = $state(false);
  /** Colour laid over the pages (see settings.svelte.ts). */
  pageTone = $state<PageTone>("original");
  /** Scroll position to restore when the tab becomes active again. */
  scrollTop = 0;
  scrollLeft = 0;
  /** Bookmarks (the outline), once loaded (see bookmarks.ts). Replaced on every change. */
  outline = $state.raw<Bookmark[] | null>(null);
  /** True when there are changes (e.g. filled-in form fields) that aren't saved. */
  dirty = $state(false);
  /** Goes up after every change, so pages showing it re-render. */
  revision = $state(0);
  /** Selected text (not kept when the tab moves to another window). */
  selection = $state.raw<TextSelection | null>(null);
  /** Searching this document (Ctrl+F). */
  readonly search: DocSearch;
  /** The find bar is open. */
  findOpen = $state(false);
  /** Organize pages is shown instead of the pages (see OrganizeView). */
  organizing = $state(false);
  /** Undo and redo of annotation, bookmark and page changes. */
  readonly history = new History((steps) => {
    // Page steps keep a whole earlier state in the engine; free it once unreachable.
    const kept = steps.flatMap((s) => (s.kind === "pages" ? [s.snapshot] : []));
    if (kept.length) dropSnapshots(kept).catch(() => {});
  });
  /** Goes up per page when its annotations change, so only that page re-renders. */
  readonly pageRevisions = new SvelteMap<number, number>();
  /** Annotations of the pages on screen, without replies (loaded by AnnotationLayer, used to click them). */
  readonly annotations = new SvelteMap<number, Annotation[]>();
  /** Areas marked for redaction, applied when the document is saved. */
  redactions = $state.raw<{ id: number; page: number; rect: PageRect }[]>([]);
  /** The annotation selected for moving, restyling or deleting. */
  selectedAnnotation = $state<{ page: number; id: string } | null>(null);

  constructor(id: number, docId: number, path: string, info: DocInfo) {
    this.id = id;
    this.docId = docId;
    this.path = path;
    this.info = info;
    this.pagesRtl = info.rtl;
    this.search = new DocSearch(docId);
  }

  /** The revision each page last painted (see `whenPainted`). */
  readonly #painted = new Map<number, number>();
  #waiting: { page: number; revision: number; done: () => void }[] = [];

  /** Called by a page once it shows `revision`. */
  markPainted(page: number, revision: number) {
    this.#painted.set(page, revision);
    const ready = this.#waiting.filter((w) => w.page === page && w.revision <= revision);
    this.#waiting = this.#waiting.filter((w) => !ready.includes(w));
    for (const w of ready) w.done();
  }

  /**
   * Resolves once `page` shows `revision` (or after `timeout` ms, e.g. when it
   * scrolled away). Previews of a change stay up until then, so nothing flashes.
   */
  whenPainted(page: number, revision = this.pageRevision(page), timeout = 3000): Promise<void> {
    if ((this.#painted.get(page) ?? -1) >= revision) return Promise.resolve();
    return new Promise((resolve) => {
      const entry = { page, revision, done: resolve };
      this.#waiting.push(entry);
      setTimeout(() => {
        this.#waiting = this.#waiting.filter((w) => w !== entry);
        resolve();
      }, timeout);
    });
  }

  /** Pages on screen that can lift an annotation off their image (see PageView). */
  readonly #lifters = new Map<number, Lifter>();

  /** Called by a page on screen: how to lift an annotation off its image. */
  registerLifter(page: number, lift: Lifter) {
    this.#lifters.set(page, lift);
    return () => {
      if (this.#lifters.get(page) === lift) this.#lifters.delete(page);
    };
  }

  /**
   * Lifts an annotation off a page's image: `hide` hides it in the document,
   * then its area is rendered again without it. The result can take it off
   * the page at once (`apply`, no waiting for the whole page) and holds its
   * pixels as drawn (`image`), to show while it moves. Null if the page isn't
   * on screen (it was still hidden).
   */
  async liftArea(page: number, rect: PageRect, hide: () => Promise<void>): Promise<Lifted | null> {
    const lift = this.#lifters.get(page);
    if (!lift) {
      await hide();
      return null;
    }
    return lift(rect, hide);
  }

  /**
   * The document's pages changed (rearranged, rotated, inserted, deleted):
   * everything kept per page index is stale. Pages render again from scratch.
   */
  reload(info: DocInfo) {
    this.info = info;
    this.revision++;
    this.pageRevisions.clear();
    this.annotations.clear();
    this.#painted.clear();
    for (const w of this.#waiting) w.done();
    this.#waiting = [];
    this.selection = null;
    this.selectedAnnotation = null;
    this.search.clear();
    forgetPageText(this.docId);
    forgetPages(this.docId);
    this.page = Math.max(0, Math.min(this.page, info.pageCount - 1));
  }

  /** Marks a page as changed, so it and its thumbnail render again. */
  bumpPage(page: number) {
    this.pageRevisions.set(page, (this.pageRevisions.get(page) ?? 0) + 1);
  }

  /** The revision a page renders at (document-wide changes plus its own). */
  pageRevision(page: number): number {
    return this.revision + (this.pageRevisions.get(page) ?? 0);
  }

  get fileName(): string {
    return this.path.split(/[\\/]/).at(-1) ?? this.path;
  }

  /** What another window needs to show this tab exactly as it is. */
  state(): TabState {
    return {
      docId: this.docId,
      path: this.path,
      page: this.page,
      zoom: this.zoom,
      zoomMode: this.zoomMode,
      rotation: this.rotation,
      pageLayout: this.pageLayout,
      continuous: this.continuous,
      pagesRtl: this.pagesRtl,
      pageTone: this.pageTone,
      dirty: this.dirty,
    };
  }

  /** What the recent-files list remembers about this tab. */
  view(): FileView {
    const { page, zoom, zoomMode, pagesRtl, pageLayout, continuous, pageTone } = this;
    return { page, zoom, zoomMode, pagesRtl, pageLayout, continuous, pageTone };
  }

  /** Document title, or the file name when the PDF has no title. */
  get title(): string {
    return this.info.title || this.fileName;
  }
}

let nextId = 1;
let list = $state<Tab[]>([]);
let activeId = $state<number | null>(null);
/** The tab that was active before the current one (for "Side by side"). */
let previousId: number | null = null;

/**
 * Two tabs shown side by side (`start` is on the left, or on the right in
 * right-to-left languages). The active tab is the focused pane: the toolbar,
 * sidebar and shortcuts act on it.
 */
export interface Split {
  start: number;
  end: number;
  /** Scrolling one pane scrolls the other. */
  sync: boolean;
}
let split = $state<Split | null>(null);

function setActive(id: number | null) {
  if (id === activeId) return;
  if (activeId !== null) previousId = activeId;
  activeId = id;
}

/** Shows `id` in the focused pane when two tabs are side by side (the other pane stays). */
function showInFocusedPane(id: number) {
  if (split && id !== split.start && id !== split.end) {
    if (split.start === activeId) split.start = id;
    else split.end = id;
    split.sync = false;
  }
  setActive(id);
}

/** A tab is going away: side by side ends, and the other pane's tab stays shown. */
function leaveSplit(id: number): number | null {
  if (!split || (id !== split.start && id !== split.end)) return null;
  const other = id === split.start ? split.end : split.start;
  split = null;
  return other;
}

function samePath(a: string, b: string) {
  return a.toLowerCase() === b.toLowerCase();
}

export const tabs = {
  get list() {
    return list;
  },
  get active(): Tab | null {
    return list.find((t) => t.id === activeId) ?? null;
  },
  /** The tabs shown: two when side by side, else the active one (none on the start page). */
  get panes(): Tab[] {
    const ids = split ? [split.start, split.end] : activeId === null ? [] : [activeId];
    return ids.map((id) => list.find((t) => t.id === id)).filter((t) => t !== undefined);
  },
  get split(): Split | null {
    return split;
  },
  /** Shows tab `id` beside the active tab (the active one stays on the start side). */
  showBeside(id: number) {
    const active = activeId;
    if (active === null || id === active || !list.some((t) => t.id === id)) return;
    split = { start: active, end: id, sync: false };
  },
  /** The tab to show beside the active one when none is chosen: the one used before it. */
  get besideCandidate(): Tab | null {
    const others = list.filter((t) => t.id !== activeId);
    return others.find((t) => t.id === previousId) ?? others.at(-1) ?? null;
  },
  /** Back to one tab: the focused one. */
  endSplit() {
    split = null;
  },
  swapPanes() {
    if (split) split = { ...split, start: split.end, end: split.start };
  },
  setSync(on: boolean) {
    if (split) split.sync = on;
  },
  /** Makes one of the two panes the focused one. */
  focusPane(id: number) {
    if (split && (id === split.start || id === split.end)) setActive(id);
  },
  findByPath(path: string): Tab | undefined {
    return list.find((t) => samePath(t.path, path));
  },
  add(docId: number, path: string, info: DocInfo): Tab {
    const tab = new Tab(nextId++, docId, path, info);
    list = [...list, tab];
    showInFocusedPane(tab.id);
    return tab;
  },
  /** Shows a tab; side by side, it takes the place of the focused pane's tab. */
  activate(id: number) {
    if (list.some((t) => t.id === id)) showInFocusedPane(id);
  },
  /** The start page is shown as a tab of its own (the + button), next to the open documents. */
  get home(): boolean {
    return activeId === null && list.length > 0;
  },
  /** Shows the start page (recent files, open a file) without closing anything. */
  showHome() {
    split = null;
    setActive(null);
  },
  /** Leaves the start page for the last document. */
  closeHome() {
    if (activeId === null) setActive(list.at(-1)?.id ?? null);
  },
  /** Activates the next (+1) or previous (-1) tab, wrapping around. */
  cycle(direction: 1 | -1) {
    if (activeId === null) {
      setActive((direction > 0 ? list[0] : list.at(-1))?.id ?? null);
      return;
    }
    if (list.length < 2) return;
    const index = list.findIndex((t) => t.id === activeId);
    this.activate(list[(index + direction + list.length) % list.length].id);
  },
  /** Shows a tab handed over from another window (its document is already open). */
  adopt(state: TabState, info: DocInfo): Tab {
    const tab = this.add(state.docId, state.path, info);
    tab.page = Math.min(state.page, info.pageCount - 1);
    tab.zoom = state.zoom;
    tab.zoomMode = state.zoomMode;
    tab.rotation = state.rotation;
    tab.pageLayout = state.pageLayout;
    tab.continuous = state.continuous;
    tab.pagesRtl = state.pagesRtl;
    tab.pageTone = state.pageTone ?? "original";
    tab.dirty = state.dirty;
    return tab;
  },
  /** Removes a tab that moved to another window, leaving its document open. */
  detach(id: number) {
    const index = list.findIndex((t) => t.id === id);
    if (index < 0) return;
    list.splice(index, 1);
    list = [...list];
    const other = leaveSplit(id);
    if (activeId === id) setActive(other ?? list[Math.min(index, list.length - 1)]?.id ?? null);
  },
  /** Closes a tab right away (callers ask about unsaved changes first). */
  async close(id: number) {
    const index = list.findIndex((t) => t.id === id);
    if (index < 0) return;
    const [tab] = list.splice(index, 1);
    list = [...list];
    const other = leaveSplit(id);
    if (activeId === id) setActive(other ?? list[Math.min(index, list.length - 1)]?.id ?? null);
    tab.search.clear();
    forgetPageText(tab.docId);
    forgetPages(tab.docId);
    await closeDocument(tab.docId).catch(() => {});
  },
};
