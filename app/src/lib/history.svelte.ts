// Undo and redo for annotation changes, per tab. Each step remembers the
// annotation before and after the change: added (before = null), deleted
// (after = null) or changed (both). Undoing applies the step backwards.

import type { Annotation } from "./bindings/Annotation";

export interface Step {
  page: number;
  before: Annotation | null;
  after: Annotation | null;
}

/** How many steps are kept. */
const LIMIT = 200;

export class History {
  // Replaced (not changed in place), so the Undo and Redo buttons follow them.
  #done = $state.raw<Step[]>([]);
  #undone = $state.raw<Step[]>([]);

  get canUndo(): boolean {
    return this.#done.length > 0;
  }

  get canRedo(): boolean {
    return this.#undone.length > 0;
  }

  /** Records a change that was just made (and forgets what was undone). */
  record(step: Step) {
    this.#done = [...this.#done, step].slice(-LIMIT);
    this.#undone = [];
  }

  /** The step to undo, moved to the redo list. Apply it backwards. */
  undo(): Step | undefined {
    const step = this.#done.at(-1);
    if (step) {
      this.#done = this.#done.slice(0, -1);
      this.#undone = [...this.#undone, step];
    }
    return step;
  }

  /** The step to redo, moved back to the undo list. Apply it forwards. */
  redo(): Step | undefined {
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
    for (const step of [...this.#done, ...this.#undone]) {
      if (step.before?.id === oldId) step.before = { ...step.before, id: newId };
      if (step.after?.id === oldId) step.after = { ...step.after, id: newId };
    }
  }

  clear() {
    this.#done = [];
    this.#undone = [];
  }
}
