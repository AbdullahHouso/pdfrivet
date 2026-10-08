import { describe, expect, it, vi } from "vitest";
import type { SearchHit } from "./bindings/SearchHit";

const searchDocument = vi.fn();
vi.mock("./pdf", () => ({ searchDocument: (...a: unknown[]) => searchDocument(...a) }));

const { DocSearch, MAX_HITS, runSearch } = await import("./search.svelte");

const hit = (page: number, start = 0): SearchHit => ({
  page,
  start,
  end: start + 3,
  before: "",
  text: "abc",
  after: "",
});

/** A fake engine: `pages` pages, one result on each page in `withHits`, 2 pages per batch. */
function engine(pages: number, withHits: number[]) {
  return async (_doc: number, _query: unknown, first: number) => {
    const last = Math.min(first + 2, pages);
    const hits = withHits.filter((p) => p >= first && p < last).map((p) => hit(p));
    return { hits, nextPage: last < pages ? last : null };
  };
}

describe("runSearch", () => {
  it("asks for batches until the last page", async () => {
    const firsts: number[] = [];
    const found: SearchHit[] = [];
    await runSearch(
      async (first) => {
        firsts.push(first);
        return engine(5, [0, 3, 4])(0, null, first);
      },
      (hits) => found.push(...hits),
      () => false,
    );
    expect(firsts).toEqual([0, 2, 4]);
    expect(found.map((h) => h.page)).toEqual([0, 3, 4]);
  });

  it("stops when a newer search starts", async () => {
    let calls = 0;
    await runSearch(
      async () => {
        calls++;
        return { hits: [hit(0)], nextPage: 1 };
      },
      () => {},
      () => calls >= 2,
    );
    expect(calls).toBe(2);
  });

  it("stops at the result limit", async () => {
    let total = 0;
    await runSearch(
      async (first) => ({ hits: Array.from({ length: 1000 }, (_, i) => hit(first, i)), nextPage: first + 1 }),
      (hits) => (total += hits.length),
      () => false,
    );
    expect(total).toBe(MAX_HITS);
  });
});

describe("DocSearch", () => {
  it("starts at the first result from the page you're reading", async () => {
    searchDocument.mockImplementation(engine(10, [1, 4, 7]));
    const search = new DocSearch(1);
    search.query = "abc";
    await search.run(5);
    expect(search.hits.map((h) => h.page)).toEqual([1, 4, 7]);
    expect(search.hits[search.current].page).toBe(7);
  });

  it("wraps around when stepping", async () => {
    searchDocument.mockImplementation(engine(10, [1, 4, 7]));
    const search = new DocSearch(1);
    search.query = "abc";
    await search.run(9);
    // Nothing after page 9: starts from the top.
    expect(search.current).toBe(0);
    search.step(-1);
    expect(search.current).toBe(2);
    search.step(1);
    expect(search.current).toBe(0);
  });

  it("groups results by page for highlighting", async () => {
    searchDocument.mockImplementation(async () => ({ hits: [hit(2, 0), hit(2, 10), hit(3)], nextPage: null }));
    const search = new DocSearch(1);
    search.query = "abc";
    await search.run(0);
    expect(search.marksOn(2)).toEqual([
      { start: 0, end: 3, current: true },
      { start: 10, end: 13, current: false },
    ]);
    expect(search.marksOn(5)).toEqual([]);
    search.clear();
    expect(search.marksOn(2)).toEqual([]);
  });

  it("does nothing for an empty query", async () => {
    searchDocument.mockClear();
    const search = new DocSearch(1);
    search.query = "   ";
    await search.run(0);
    expect(searchDocument).not.toHaveBeenCalled();
    expect(search.running).toBe(false);
  });
});
