// Recent files list: pure logic (storage lives in settings.svelte.ts).

import type { PageTone } from "./settings.svelte";
import type { PageLayout } from "./tabs.svelte";

export interface RecentFile {
  path: string;
  title: string;
  /** Milliseconds since 1970. */
  lastOpened: number;
  /** Where you left off. */
  page: number;
  zoom: number;
  zoomMode: ZoomMode;
  /** Page order chosen for this file, if changed from what PDFRivet detected. */
  pagesRtl?: boolean;
  // How you last viewed this file. Missing in entries saved by older versions,
  // which then open with the defaults from Settings.
  pageLayout?: PageLayout;
  continuous?: boolean;
  pageTone?: PageTone;
}

/** What a file remembers about how you viewed it. */
export type FileView = Pick<
  RecentFile,
  "page" | "zoom" | "zoomMode" | "pagesRtl" | "pageLayout" | "continuous" | "pageTone"
>;

export type ZoomMode = "fit-width" | "fit-page" | "custom";

export const MAX_RECENT = 20;

function samePath(a: string, b: string): boolean {
  // Windows paths are case-insensitive; others are not, but a false match there is harmless.
  return a.toLowerCase() === b.toLowerCase();
}

/** Adds or updates an entry and moves it to the top. */
export function touchRecent(list: RecentFile[], entry: RecentFile): RecentFile[] {
  return [entry, ...list.filter((r) => !samePath(r.path, entry.path))].slice(0, MAX_RECENT);
}

/** Updates where you left off and how you viewed the file, without changing the order. */
export function updateView(list: RecentFile[], path: string, view: FileView): RecentFile[] {
  return list.map((r) => (samePath(r.path, path) ? { ...r, ...view } : r));
}

export function removeRecent(list: RecentFile[], path: string): RecentFile[] {
  return list.filter((r) => !samePath(r.path, path));
}

export function findRecent(list: RecentFile[], path: string): RecentFile | undefined {
  return list.find((r) => samePath(r.path, path));
}
