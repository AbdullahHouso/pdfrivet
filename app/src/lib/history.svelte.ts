// Undo and redo for annotation and bookmark changes, per tab. Each step
// remembers the annotation before and after the change: added (before =
// null), deleted (after = null) or changed (both); or the whole bookmark tree
// before and after. Undoing applies the step backwards.
// Changes made in one gesture (everything one eraser stroke touched) form a
// group that is undone and redone together. Page changes (rearranging,
// rotating) are steps too, so one Ctrl+Z goes back through everything in order.

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

/**
 * Pages were rearranged, deleted or inserted: undo and redo swap the document
 * with a state the engine keeps (`snapshot`, updated on every swap).
 */
export interface PagesStep {
  kind: "pages";
  snapshot: number;
}

/** Pages were turned (undone by turning them back). */
export interface RotateStep {
  kind: "rotate";
  pages: number[];
  turns: number;
}

export type Step = AnnotationStep | OutlineStep | PagesStep | RotateStep;

/** How many steps are kept. */
const LIMIT = 200;

export class History {
  /** Called with steps that can no longer be undone or redone (to free what they keep). */
  readonly #forget: (steps: Step[]) => void;

  constructor(forget: (steps: Step[]) => void = () => {}) {
    this.#forget = forget;
  }

  // Replaced (not changed in place), so the Undo and Redo buttons follow them.
  #done = $state.raw<Step[][]>([]);
  #undone = $state.raw<Step[][]>([]);
  /** While a group is open, changes join its last entry. */
  #grouping = false;
  #groupHasEntry = false;
  #depth = 0;

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
      const done = [...this.#done, [step]];
      this.#forget(done.slice(0, -LIMIT).flat());
      this.#done = done.slice(-LIMIT);
      this.#groupHasEntry = this.#grouping;
    }
    this.#forget(this.#undone.flat());
    this.#undone = [];
  }

  /**
   * Changes recorded until `endGroup` are undone and redone together. Groups
   * can be nested (deleting a comment with its replies during an eraser stroke):
   * everything joins the outermost one.
   */
  beginGroup() {
    if (this.#depth++ === 0) {
      this.#grouping = true;
      this.#groupHasEntry = false;
    }
  }

  endGroup() {
    if (this.#depth > 0 && --this.#depth === 0) this.#grouping = false;
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
      if (step.kind) continue;
      if (step.before?.id === oldId) step.before = { ...step.before, id: newId };
      if (step.after?.id === oldId) step.after = { ...step.after, id: newId };
    }
  }

  clear() {
    this.#forget([...this.#done, ...this.#undone].flat());
    this.#done = [];
    this.#undone = [];
  }
}
