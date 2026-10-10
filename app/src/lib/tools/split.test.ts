import { describe, expect, it } from "vitest";
import { partFileNames, safeFileName, splitAtBookmarks, splitEvery, splitRanges } from "./split";

const pages = (parts: { pages: number[] }[]) => parts.map((p) => p.pages);

describe("split", () => {
  it("every N pages", () => {
    expect(pages(splitEvery(5, 2))).toEqual([[0, 1], [2, 3], [4]]);
    expect(pages(splitEvery(3, 1))).toEqual([[0], [1], [2]]);
    expect(pages(splitEvery(3, 0))).toEqual([[0], [1], [2]]);
  });

  it("by ranges, in Western or Arabic digits", () => {
    expect(pages(splitRanges("1-2, 4, 3-3", 5) ?? [])).toEqual([[0, 1], [3], [2]]);
    expect(pages(splitRanges("١-٢، ٥", 5) ?? [])).toEqual([[0, 1], [4]]);
    expect(pages(splitRanges("3-1", 5) ?? [])).toEqual([[2, 1, 0]]);
    expect(splitRanges("1-9", 5)).toBeNull();
    expect(splitRanges("one", 5)).toBeNull();
    expect(splitRanges("", 5)).toBeNull();
  });

  it("at bookmarks", () => {
    const parts = splitAtBookmarks(
      [
        { title: "Two", page: 4 },
        { title: "One", page: 1 },
        { title: "Nowhere", page: null },
      ],
      7,
    );
    expect(pages(parts)).toEqual([[0], [1, 2, 3], [4, 5, 6]]);
    expect(parts.map((p) => p.title)).toEqual([undefined, "One", "Two"]);
    expect(splitAtBookmarks([], 3)).toEqual([]);
  });

  it("makes safe, unique file names", () => {
    expect(safeFileName('Chapter 1: "Intro"/Start?.')).toBe("Chapter 1 Intro Start");
    expect(safeFileName("الفصل الأول")).toBe("الفصل الأول");
    const parts = [{ pages: [0] }, { pages: [1], title: "A/B" }, { pages: [2], title: "A:B" }];
    expect(partFileNames("doc", parts, new Set(["doc-1.pdf"]))).toEqual([
      "doc-1 (2).pdf",
      "doc - A B.pdf",
      "doc - A B (2).pdf",
    ]);
    const many = Array.from({ length: 12 }, (_, i) => ({ pages: [i] }));
    expect(partFileNames("d", many, new Set())[0]).toBe("d-01.pdf");
  });
});
