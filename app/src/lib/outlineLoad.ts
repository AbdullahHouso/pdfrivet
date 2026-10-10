// Loading a tab's bookmarks (kept apart from bookmarks.ts, which needs the
// translations, so page changes can drop them without loading those).

import { type Bookmark, withKeys } from "./outlineTree";
import { getOutline } from "./pdf";
import type { Tab } from "./tabs.svelte";

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

/** The document's pages changed: the bookmarks are read again when next needed. */
export function forgetBookmarks(tab: Tab) {
  loading.delete(tab);
  tab.outline = null;
}
