// The arrangement edited in Organize pages: a list of pages, each from this
// document, another one, or blank, with extra quarter turns. Nothing changes
// in the document until Done sends it to the engine (see pages.rs).

import type { PageSlot } from "../bindings/PageSlot";
import type { PageSource } from "../bindings/PageSource";

export interface PlanItem {
  /** Stays with the page while it moves (for selection and drawing). */
  key: number;
  source: PageSource;
  /** Quarter turns clockwise, on top of the page's own rotation. */
  turns: number;
}

let nextKey = 1;

export function newItem(source: PageSource, turns = 0): PlanItem {
  return { key: nextKey++, source, turns };
}

/** The document's pages as they are. */
export function initialPlan(pageCount: number): PlanItem[] {
  return Array.from({ length: pageCount }, (_, index) => newItem({ kind: "page", index }));
}

/** Nothing would change: every page in its place, unturned. */
export function isUnchanged(items: PlanItem[], pageCount: number): boolean {
  return (
    items.length === pageCount &&
    items.every((item, i) => item.source.kind === "page" && item.source.index === i && item.turns % 4 === 0)
  );
}

/**
 * Moves the selected pages (keeping their order) so they land before the page
 * at `to` in the current list (`to` = length: at the end).
 */
export function moveItems(items: PlanItem[], keys: Set<number>, to: number): PlanItem[] {
  const moving = items.filter((item) => keys.has(item.key));
  if (moving.length === 0) return items;
  // Where `to` is once the moving pages are taken out.
  const before = items.slice(0, to).filter((item) => !keys.has(item.key)).length;
  const rest = items.filter((item) => !keys.has(item.key));
  return [...rest.slice(0, before), ...moving, ...rest.slice(before)];
}

export function rotateItems(items: PlanItem[], keys: Set<number>, turns: number): PlanItem[] {
  return items.map((item) => (keys.has(item.key) ? { ...item, turns: (((item.turns + turns) % 4) + 4) % 4 } : item));
}

export function deleteItems(items: PlanItem[], keys: Set<number>): PlanItem[] {
  return items.filter((item) => !keys.has(item.key));
}

/** Copies of the selected pages, placed right after the last selected one. Returns the copies too. */
export function duplicateItems(items: PlanItem[], keys: Set<number>): [PlanItem[], PlanItem[]] {
  const copies = items.filter((item) => keys.has(item.key)).map((item) => newItem(item.source, item.turns));
  return [insertAfterSelection(items, keys, copies), copies];
}

/** Puts `added` after the last selected page (or at the end when nothing is selected). */
export function insertAfterSelection(items: PlanItem[], keys: Set<number>, added: PlanItem[]): PlanItem[] {
  let last = -1;
  items.forEach((item, i) => {
    if (keys.has(item.key)) last = i;
  });
  const at = last < 0 ? items.length : last + 1;
  return [...items.slice(0, at), ...added, ...items.slice(at)];
}

/** Keys from `anchor` to `key` (inclusive), in list order. */
export function rangeOfKeys(items: PlanItem[], anchor: number, key: number): Set<number> {
  const a = items.findIndex((item) => item.key === anchor);
  const b = items.findIndex((item) => item.key === key);
  if (a < 0 || b < 0) return new Set([key]);
  const [from, to] = a < b ? [a, b] : [b, a];
  return new Set(items.slice(from, to + 1).map((item) => item.key));
}

export function toSlots(items: PlanItem[]): PageSlot[] {
  return items.map((item) => ({ source: item.source, turns: item.turns }));
}

/** Which page of the document (or another file) an item shows, for drawing it. */
export function sourceDoc(item: PlanItem, docId: number): { doc: number; index: number } | null {
  if (item.source.kind === "page") return { doc: docId, index: item.source.index };
  if (item.source.kind === "other") return { doc: item.source.doc, index: item.source.index };
  return null;
}
