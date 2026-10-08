// Searching one document. The engine searches a batch of pages at a time
// (rivet-core/src/search.rs); this keeps asking for the next batch, so results
// appear while a long document is still being searched, pages keep rendering
// in between, and a new search simply stops the old one.

import type { SearchHit } from "./bindings/SearchHit";
import { searchDocument } from "./pdf";

/** Searching stops after this many results. */
export const MAX_HITS = 5000;

/** A result to highlight on a page. */
export interface SearchMark {
  start: number;
  end: number;
  current: boolean;
}

/** Runs the batches of one search. Exported for tests. */
export async function runSearch(
  find: (first: number) => Promise<{ hits: SearchHit[]; nextPage: number | null }>,
  onbatch: (hits: SearchHit[], nextPage: number | null) => void,
  stopped: () => boolean,
): Promise<void> {
  let next: number | null = 0;
  let found = 0;
  while (next !== null) {
    const batch = await find(next);
    if (stopped()) return;
    const hits = batch.hits.slice(0, MAX_HITS - found);
    found += hits.length;
    next = found >= MAX_HITS ? null : batch.nextPage;
    onbatch(hits, next);
  }
}

export class DocSearch {
  readonly docId: number;
  query = $state("");
  matchCase = $state(false);
  wholeWord = $state(false);
  /** The text that was searched for (the box may already hold something new). */
  searched = $state("");
  hits = $state.raw<SearchHit[]>([]);
  /** Index of the highlighted result in `hits`, or -1. */
  current = $state(-1);
  running = $state(false);
  /** Pages searched so far (for the progress shown while searching). */
  progress = $state(0);
  /** Results by page: page → indexes into `hits`. */
  byPage = $derived.by(() => {
    const map = new Map<number, number[]>();
    this.hits.forEach((hit, i) => {
      const list = map.get(hit.page);
      if (list) list.push(i);
      else map.set(hit.page, [i]);
    });
    return map;
  });
  #generation = 0;

  constructor(docId: number) {
    this.docId = docId;
  }

  /**
   * Searches for the current query. The first result on or after `fromPage`
   * becomes the current one, so searching starts where you are reading.
   */
  async run(fromPage: number): Promise<void> {
    const generation = ++this.#generation;
    const text = this.query.trim();
    this.searched = text;
    this.hits = [];
    this.current = -1;
    this.progress = 0;
    if (!text) {
      this.running = false;
      return;
    }
    this.running = true;
    const query = { text, matchCase: this.matchCase, wholeWord: this.wholeWord };
    try {
      await runSearch(
        (first) => searchDocument(this.docId, query, first),
        (hits, next) => {
          this.hits = [...this.hits, ...hits];
          this.progress = next ?? Number.POSITIVE_INFINITY;
          if (this.current < 0) {
            const ahead = this.hits.findIndex((h) => h.page >= fromPage);
            if (ahead >= 0) this.current = ahead;
          }
        },
        () => generation !== this.#generation,
      );
    } finally {
      if (generation === this.#generation) {
        this.running = false;
        // Nothing after where you were reading: start from the top.
        if (this.current < 0 && this.hits.length > 0) this.current = 0;
      }
    }
  }

  /** Moves to the next (+1) or previous (-1) result, wrapping around. */
  step(direction: 1 | -1) {
    const count = this.hits.length;
    if (count === 0) return;
    this.current = this.current < 0 ? 0 : (this.current + direction + count) % count;
  }

  /** The results on one page, for highlighting. */
  marksOn(page: number): SearchMark[] {
    const indexes = this.byPage.get(page);
    if (!indexes) return [];
    return indexes.map((i) => ({ start: this.hits[i].start, end: this.hits[i].end, current: i === this.current }));
  }

  /** Stops searching and removes the results. */
  clear() {
    this.#generation++;
    this.running = false;
    this.searched = "";
    this.hits = [];
    this.current = -1;
  }
}
