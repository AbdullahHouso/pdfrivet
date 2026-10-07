import { describe, expect, it } from "vitest";
import { findRecent, MAX_RECENT, type RecentFile, removeRecent, touchRecent, updatePosition } from "./recent";

const file = (path: string, lastOpened = 0): RecentFile => ({
  path,
  title: path,
  lastOpened,
  page: 0,
  zoom: 1,
  zoomMode: "fit-width",
});

describe("recent files", () => {
  it("puts the newest file on top without duplicates", () => {
    let list = touchRecent([], file("a.pdf"));
    list = touchRecent(list, file("b.pdf"));
    list = touchRecent(list, file("A.PDF", 5));
    expect(list.map((r) => r.path)).toEqual(["A.PDF", "b.pdf"]);
  });

  it("keeps at most MAX_RECENT entries", () => {
    let list: RecentFile[] = [];
    for (let i = 0; i < MAX_RECENT + 5; i++) list = touchRecent(list, file(`${i}.pdf`));
    expect(list).toHaveLength(MAX_RECENT);
    expect(list[0].path).toBe(`${MAX_RECENT + 4}.pdf`);
  });

  it("remembers the position without reordering", () => {
    const list = updatePosition([file("a.pdf"), file("b.pdf")], "b.pdf", { page: 7, zoom: 1.5, zoomMode: "custom" });
    expect(list.map((r) => r.path)).toEqual(["a.pdf", "b.pdf"]);
    expect(findRecent(list, "b.pdf")?.page).toBe(7);
  });

  it("removes entries", () => {
    expect(removeRecent([file("a.pdf"), file("b.pdf")], "a.pdf").map((r) => r.path)).toEqual(["b.pdf"]);
  });
});
