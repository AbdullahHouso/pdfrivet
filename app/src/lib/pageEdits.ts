// Changes to a document's pages (rearranging, rotating, inserting, deleting).
// Each is one undo step. Afterwards everything the UI keeps per page index is
// read again (see `Tab.reload`).

import type { DocInfo } from "./bindings/DocInfo";
import type { PageSlot } from "./bindings/PageSlot";
import { commentsOf } from "./comments.svelte";
import type { PagesStep, RotateStep } from "./history.svelte";
import { forgetBookmarks } from "./outlineLoad";
import { arrangePages, flattenDocument, rotatePages, swapSnapshot } from "./pdf";
import type { Tab } from "./tabs.svelte";

/** The pages changed: the tab shows the document as it is now. */
function refresh(tab: Tab, info: DocInfo) {
  tab.reload(info);
  forgetBookmarks(tab);
  commentsOf(tab).reset();
  tab.dirty = true;
}

/** Rearranges the pages (see pages.rs); one undo step. */
export async function arrange(tab: Tab, slots: PageSlot[]) {
  const { snapshot, info } = await arrangePages(tab.docId, slots);
  tab.history.record({ kind: "pages", snapshot });
  refresh(tab, info);
}

/** Flattens annotations and form fields into the pages; one undo step. */
export async function flatten(tab: Tab) {
  const { snapshot, info } = await flattenDocument(tab.docId);
  tab.history.record({ kind: "pages", snapshot });
  refresh(tab, info);
}

/** Turns pages by quarter turns (clockwise; negative: counter-clockwise); one undo step. */
export async function rotate(tab: Tab, pages: number[], turns: number) {
  const info = await rotatePages(tab.docId, pages, turns);
  tab.history.record({ kind: "rotate", pages, turns });
  refresh(tab, info);
}

/** Applies a page step of the history, backwards (undo) or forwards (redo). */
export async function applyPageStep(tab: Tab, step: PagesStep | RotateStep, forwards: boolean) {
  if (step.kind === "rotate") {
    refresh(tab, await rotatePages(tab.docId, step.pages, forwards ? step.turns : -step.turns));
    return;
  }
  // Undo and redo both swap: the state that was shown is kept for going back.
  const { snapshot, info } = await swapSnapshot(tab.docId, step.snapshot);
  step.snapshot = snapshot;
  refresh(tab, info);
}
