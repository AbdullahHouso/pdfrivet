// Open documents ("tabs"). Each tab keeps its own view state, so switching
// tabs brings you back exactly where you were. Only the active tab's pages
// are rendered; background tabs keep their state, not their pixels.

import type { DocInfo } from "./bindings/DocInfo";
import type { OutlineItem } from "./bindings/OutlineItem";
import type { Degrees } from "./layout";
import { closeDocument } from "./pdf";
import type { ZoomMode } from "./recent";

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
  /** Scroll position to restore when the tab becomes active again. */
  scrollTop = 0;
  scrollLeft = 0;
  outline = $state<OutlineItem[] | null>(null);
  /** True when there are changes (e.g. filled-in form fields) that aren't saved. */
  dirty = $state(false);
  /** Goes up after every change, so pages showing it re-render. */
  revision = $state(0);

  constructor(id: number, docId: number, path: string, info: DocInfo) {
    this.id = id;
    this.docId = docId;
    this.path = path;
    this.info = info;
    this.pagesRtl = info.rtl;
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
      dirty: this.dirty,
    };
  }

  /** Document title, or the file name when the PDF has no title. */
  get title(): string {
    return this.info.title || this.fileName;
  }
}

let nextId = 1;
let list = $state<Tab[]>([]);
let activeId = $state<number | null>(null);

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
  findByPath(path: string): Tab | undefined {
    return list.find((t) => samePath(t.path, path));
  },
  add(docId: number, path: string, info: DocInfo): Tab {
    const tab = new Tab(nextId++, docId, path, info);
    list = [...list, tab];
    activeId = tab.id;
    return tab;
  },
  activate(id: number) {
    if (list.some((t) => t.id === id)) activeId = id;
  },
  /** Activates the next (+1) or previous (-1) tab, wrapping around. */
  cycle(direction: 1 | -1) {
    if (list.length < 2) return;
    const index = list.findIndex((t) => t.id === activeId);
    activeId = list[(index + direction + list.length) % list.length].id;
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
    tab.dirty = state.dirty;
    return tab;
  },
  /** Removes a tab that moved to another window, leaving its document open. */
  detach(id: number) {
    const index = list.findIndex((t) => t.id === id);
    if (index < 0) return;
    list.splice(index, 1);
    list = [...list];
    if (activeId === id) activeId = list[Math.min(index, list.length - 1)]?.id ?? null;
  },
  /** Closes a tab right away (callers ask about unsaved changes first). */
  async close(id: number) {
    const index = list.findIndex((t) => t.id === id);
    if (index < 0) return;
    const [tab] = list.splice(index, 1);
    list = [...list];
    if (activeId === id) activeId = list[Math.min(index, list.length - 1)]?.id ?? null;
    await closeDocument(tab.docId).catch(() => {});
  },
};
