// Recent files list: pure logic (storage lives in settings.svelte.ts).

export interface RecentFile {
  path: string;
  title: string;
  /** Milliseconds since 1970. */
  lastOpened: number;
  /** Where you left off. */
  page: number;
  zoom: number;
  zoomMode: ZoomMode;
}

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

/** Updates where you left off, without changing the order. */
export function updatePosition(
  list: RecentFile[],
  path: string,
  position: Pick<RecentFile, "page" | "zoom" | "zoomMode">,
): RecentFile[] {
  return list.map((r) => (samePath(r.path, path) ? { ...r, ...position } : r));
}

export function removeRecent(list: RecentFile[], path: string): RecentFile[] {
  return list.filter((r) => !samePath(r.path, path));
}

export function findRecent(list: RecentFile[], path: string): RecentFile | undefined {
  return list.find((r) => samePath(r.path, path));
}
