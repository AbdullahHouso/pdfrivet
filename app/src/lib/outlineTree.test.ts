import { describe, expect, it } from "vitest";
import {
  type Bookmark,
  copyTree,
  indent,
  move,
  newBookmark,
  outdent,
  pathOf,
  placeForPage,
  shift,
  withKeys,
} from "./outlineTree";

/** A, B (with B1, B2), C — pointing to pages 0, 4 (5, 6), 9. */
function tree(): Bookmark[] {
  const leaf = (title: string, page: number) => ({ title, page, children: [], origin: null });
  return withKeys([leaf("A", 0), { ...leaf("B", 4), children: [leaf("B1", 5), leaf("B2", 6)] }, leaf("C", 9)]);
}

const titles = (items: Bookmark[]): unknown[] =>
  items.map((i) => (i.children.length ? [i.title, titles(i.children)] : i.title));

const key = (items: Bookmark[], title: string): number => {
  const find = (list: Bookmark[]): Bookmark | undefined =>
    list.map((i) => (i.title === title ? i : find(i.children))).find(Boolean);
  return (find(items) as Bookmark).key;
};

describe("bookmark tree", () => {
  it("places a new bookmark in page order", () => {
    const t = tree();
    expect(placeForPage(t, 0)).toEqual([1]);
    expect(placeForPage(t, 5)).toEqual([2]);
    expect(placeForPage(t, 20)).toEqual([3]);
    expect(newBookmark("x", 3).origin).toBeNull();
  });

  it("moves before, after and inside other entries", () => {
    const t = tree();
    expect(move(t, key(t, "C"), key(t, "A"), "before")).toBe(true);
    expect(titles(t)).toEqual(["C", "A", ["B", ["B1", "B2"]]]);
    expect(move(t, key(t, "B1"), key(t, "C"), "inside")).toBe(true);
    expect(titles(t)).toEqual([["C", ["B1"]], "A", ["B", ["B2"]]]);
    expect(move(t, key(t, "A"), key(t, "B2"), "after")).toBe(true);
    expect(titles(t)).toEqual([
      ["C", ["B1"]],
      ["B", ["B2", "A"]],
    ]);
  });

  it("won't move an entry into itself", () => {
    const t = tree();
    expect(move(t, key(t, "B"), key(t, "B1"), "inside")).toBe(false);
    expect(move(t, key(t, "B"), key(t, "B"), "after")).toBe(false);
    expect(titles(t)).toEqual(titles(tree()));
  });

  it("shifts, indents and outdents with the keyboard", () => {
    const t = tree();
    expect(shift(t, key(t, "A"), -1)).toBe(false);
    expect(shift(t, key(t, "A"), 1)).toBe(true);
    expect(titles(t)).toEqual([["B", ["B1", "B2"]], "A", "C"]);
    expect(indent(t, key(t, "A"))).toBe(true);
    expect(titles(t)).toEqual([["B", ["B1", "B2", "A"]], "C"]);
    expect(outdent(t, key(t, "B1"))).toBe(true);
    expect(titles(t)).toEqual([["B", ["B2", "A"]], "B1", "C"]);
    expect(outdent(t, key(t, "C"))).toBe(false);
    expect(pathOf(t, key(t, "A"))).toEqual([0, 1]);
  });

  it("copies deeply", () => {
    const t = tree();
    const copy = copyTree(t);
    copy[1].children[0].title = "changed";
    expect(t[1].children[0].title).toBe("B1");
  });
});
