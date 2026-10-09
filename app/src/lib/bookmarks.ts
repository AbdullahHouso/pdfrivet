// Adding and changing bookmarks (the PDF outline). The engine keeps the
// edited tree and writes it into the file on save; every change is one undo step.

import { i18n } from "./i18n.svelte";
import { type Bookmark, copyTree, insertAt, newBookmark, placeForPage, withKeys } from "./outlineTree";
import { getOutline, getText, setOutline } from "./pdf";
import type { Tab } from "./tabs.svelte";
import { isEmpty, ordered, toTextRange } from "./textSelect";

/** Loads the bookmarks once (later calls wait for the same load). */
const loading = new WeakMap<Tab, Promise<Bookmark[]>>();
export function loadBookmarks(tab: Tab): Promise<Bookmark[]> {
  if (tab.outline) return Promise.resolve(tab.outline);
  let request = loading.get(tab);
  if (!request) {
    request = getOutline(tab.docId)
      .then((items) => (tab.outline ??= withKeys(items)))
      .catch(() => (tab.outline ??= []));
    loading.set(tab, request);
  }
  return request;
}

/**
 * Changes the bookmarks: `change` edits a copy of the tree (and returns false
 * if there was nothing to do). Recorded for undo; marks the tab as changed.
 */
export async function editBookmarks(tab: Tab, change: (items: Bookmark[]) => boolean | undefined): Promise<boolean> {
  const before = await loadBookmarks(tab);
  const after = copyTree(before);
  if (change(after) === false) return false;
  await setOutline(tab.docId, after);
  tab.outline = after;
  tab.dirty = true;
  tab.history.record({ kind: "outline", before, after });
  return true;
}

/** Short enough to read in the sidebar. */
const MAX_TITLE = 80;

/**
 * Bookmarks `page`. Its title is the selected text on that page if there is
 * some, otherwise "Page N". Returns the new bookmark's key.
 */
export async function addBookmark(tab: Tab, page: number, parentKey?: number): Promise<number> {
  const title = (await selectedTitle(tab, page)) ?? i18n.t("page-n", { page: page + 1 });
  const bookmark = newBookmark(title, page);
  await editBookmarks(tab, (items) => {
    const parent = parentKey === undefined ? undefined : findByKey(items, parentKey);
    if (parent) parent.children.push(bookmark);
    else insertAt(items, placeForPage(items, page), bookmark);
  });
  return bookmark.key;
}

function findByKey(items: Bookmark[], key: number): Bookmark | undefined {
  for (const item of items) {
    if (item.key === key) return item;
    const inner = findByKey(item.children, key);
    if (inner) return inner;
  }
}

async function selectedTitle(tab: Tab, page: number): Promise<string | null> {
  const sel = tab.selection;
  if (!sel || isEmpty(sel) || !tab.info.canCopy || ordered(sel)[0].page !== page) return null;
  try {
    const text = (await getText(tab.docId, toTextRange(sel))).replace(/\s+/g, " ").trim();
    if (!text) return null;
    return text.length > MAX_TITLE ? `${text.slice(0, MAX_TITLE - 1).trimEnd()}…` : text;
  } catch {
    return null;
  }
}
