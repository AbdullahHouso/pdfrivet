// Undo and redo for annotation and bookmark changes, per tab. Each step
// remembers the annotation before and after the change: added (before =
// null), deleted (after = null) or changed (both); or the whole bookmark tree
// before and after. Undoing applies the step backwards.
// Changes made in one gesture (everything one eraser stroke touched) form a
// group that is undone and redone together.

import type { Annotation } from "./bindings/Annotation";
import type { Bookmark } from "./outlineTree";

export interface AnnotationStep {
  kind?: undefined;
  page: number;
  before: Annotation | null;
  after: Annotation | null;
}

export interface OutlineStep {
  kind: "outline";
  before: Bookmark[];
  after: Bookmark[];
}

export type Step = AnnotationStep | OutlineStep;

/** How many steps are kept. */
const LIMIT = 200;

export class History {
  // Replaced (not changed in place), so the Undo and Redo buttons follow them.
  #done = $state.raw<Step[][]>([]);
  #undone = $state.raw<Step[][]>([]);
  /** While a group is open, changes join its last entry. */
  #grouping = false;
  #groupHasEntry = false;

  get canUndo(): boolean {
    return this.#done.length > 0;
  }

  get canRedo(): boolean {
    return this.#undone.length > 0;
  }

  /** Records a change that was just made (and forgets what was undone). */
  record(step: Step) {
    if (this.#grouping && this.#groupHasEntry) {
      const last = this.#done.at(-1) ?? [];
      this.#done = [...this.#done.slice(0, -1), [...last, step]];
    } else {
      this.#done = [...this.#done, [step]].slice(-LIMIT);
      this.#groupHasEntry = this.#grouping;
    }
    this.#undone = [];
  }

  /** Changes recorded until `endGroup` are undone and redone together. */
  beginGroup() {
    this.#grouping = true;
    this.#groupHasEntry = false;
  }

  endGroup() {
    this.#grouping = false;
  }

  /** The steps to undo (in the order they were made), moved to the redo list. Apply them backwards, last first. */
  undo(): Step[] | undefined {
    const step = this.#done.at(-1);
    if (step) {
      this.#done = this.#done.slice(0, -1);
      this.#undone = [...this.#undone, step];
    }
    return step;
  }

  /** The steps to redo, moved back to the undo list. Apply them forwards, in order. */
  redo(): Step[] | undefined {
    const step = this.#undone.at(-1);
    if (step) {
      this.#undone = this.#undone.slice(0, -1);
      this.#done = [...this.#done, step];
    }
    return step;
  }

  /**
   * An annotation got a new id (re-added under another name, or named the
   * first time it was changed): later steps must find it by the new one.
   */
  rename(oldId: string, newId: string) {
    if (oldId === newId) return;
    for (const step of [...this.#done, ...this.#undone].flat()) {
      if (step.kind === "outline") continue;
      if (step.before?.id === oldId) step.before = { ...step.before, id: newId };
      if (step.after?.id === oldId) step.after = { ...step.after, id: newId };
    }
  }

  clear() {
    this.#done = [];
    this.#undone = [];
  }
}
