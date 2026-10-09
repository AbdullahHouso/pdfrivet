// Editing the bookmark tree (the PDF outline). Pure functions on plain
// objects, unit-tested. An entry is found by its path: its index at each
// level, from the top. Every bookmark has a `key` that stays the same while
// it is moved or renamed (for the UI; the engine ignores it).

import type { OutlineItem } from "./bindings/OutlineItem";

export interface Bookmark extends OutlineItem {
  key: number;
  children: Bookmark[];
}

export type Path = number[];

let nextKey = 1;

/** Gives every entry (and its children) a key. */
export function withKeys(items: OutlineItem[]): Bookmark[] {
  return items.map((item) => ({ ...item, key: nextKey++, children: withKeys(item.children) }));
}

/** A new bookmark (not from the file) pointing to `page`. */
export function newBookmark(title: string, page: number): Bookmark {
  return { title, page, children: [], origin: null, key: nextKey++ };
}

/** A deep copy (so a change can be compared with, or undone to, what was before). */
export function copyTree(items: Bookmark[]): Bookmark[] {
  return items.map((item) => ({ ...item, children: copyTree(item.children) }));
}

export function at(items: Bookmark[], path: Path): Bookmark | undefined {
  let list = items;
  let item: Bookmark | undefined;
  for (const i of path) {
    item = list[i];
    if (!item) return undefined;
    list = item.children;
  }
  return item;
}

/** The list holding the entry at `path`, and its index in it. */
function siblingsOf(items: Bookmark[], path: Path): [Bookmark[], number] {
  const parent = path.length > 1 ? at(items, path.slice(0, -1)) : undefined;
  return [parent ? parent.children : items, path[path.length - 1]];
}

export function pathOf(items: Bookmark[], key: number, prefix: Path = []): Path | null {
  for (let i = 0; i < items.length; i++) {
    if (items[i].key === key) return [...prefix, i];
    const inner = pathOf(items[i].children, key, [...prefix, i]);
    if (inner) return inner;
  }
  return null;
}

/** Takes the entry at `path` out of the tree (with its children); returns it. */
export function removeAt(items: Bookmark[], path: Path): Bookmark | undefined {
  const [list, i] = siblingsOf(items, path);
  return list.splice(i, 1)[0];
}

export function insertAt(items: Bookmark[], path: Path, item: Bookmark) {
  const [list, i] = siblingsOf(items, path);
  list.splice(i, 0, item);
}

/** Where a new top-level bookmark for `page` goes: after the last one at or before that page. */
export function placeForPage(items: Bookmark[], page: number): Path {
  let index = 0;
  items.forEach((item, i) => {
    if (item.page !== null && item.page <= page) index = i + 1;
  });
  return [index];
}

export type Drop = "before" | "after" | "inside";

/**
 * Moves the entry with `key` next to (or into, as its last child) the entry
 * with `target`. Returns false if the move makes no sense (onto itself or
 * into its own children).
 */
export function move(items: Bookmark[], key: number, target: number, where: Drop): boolean {
  if (key === target) return false;
  const from = pathOf(items, key);
  const to = pathOf(items, target);
  if (!from || !to) return false;
  // Into one of its own children.
  if (to.length > from.length && from.every((v, i) => to[i] === v)) return false;
  const item = removeAt(items, from);
  if (!item) return false;
  const targetPath = pathOf(items, target) as Path;
  if (where === "inside") {
    (at(items, targetPath) as Bookmark).children.push(item);
  } else {
    const last = targetPath.length - 1;
    insertAt(items, [...targetPath.slice(0, last), targetPath[last] + (where === "after" ? 1 : 0)], item);
  }
  return true;
}

/** Moves an entry up or down among its siblings. */
export function shift(items: Bookmark[], key: number, by: -1 | 1): boolean {
  const path = pathOf(items, key);
  if (!path) return false;
  const [list, i] = siblingsOf(items, path);
  const j = i + by;
  if (j < 0 || j >= list.length) return false;
  [list[i], list[j]] = [list[j], list[i]];
  return true;
}

/** Makes an entry the last child of the sibling above it. */
export function indent(items: Bookmark[], key: number): boolean {
  const path = pathOf(items, key);
  if (!path || path[path.length - 1] === 0) return false;
  const [list, i] = siblingsOf(items, path);
  const [item] = list.splice(i, 1);
  list[i - 1].children.push(item);
  return true;
}

/** Moves an entry out of its parent, to just after it. */
export function outdent(items: Bookmark[], key: number): boolean {
  const path = pathOf(items, key);
  if (!path || path.length < 2) return false;
  const item = removeAt(items, path);
  if (!item) return false;
  const parent = path.slice(0, -1);
  insertAt(items, [...parent.slice(0, -1), parent[parent.length - 1] + 1], item);
  return true;
}

/** Every entry in display order, with its path (for walking the tree with the keyboard). */
export function flatten(items: Bookmark[], expanded: (key: number) => boolean, prefix: Path = []): Path[] {
  const out: Path[] = [];
  items.forEach((item, i) => {
    const path = [...prefix, i];
    out.push(path);
    if (item.children.length && expanded(item.key)) out.push(...flatten(item.children, expanded, path));
  });
  return out;
}
