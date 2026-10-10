import { describe, expect, it } from "vitest";
import {
  deleteItems,
  duplicateItems,
  initialPlan,
  insertAfterSelection,
  isUnchanged,
  moveItems,
  newItem,
  type PlanItem,
  rangeOfKeys,
  rotateItems,
  toSlots,
} from "./plan";

/** The original page numbers, in order (blank pages as "b"). */
function pages(items: PlanItem[]) {
  return items.map((i) => (i.source.kind === "page" ? i.source.index : "b"));
}

describe("organize plan", () => {
  it("starts unchanged", () => {
    const plan = initialPlan(4);
    expect(pages(plan)).toEqual([0, 1, 2, 3]);
    expect(isUnchanged(plan, 4)).toBe(true);
  });

  it("moves selected pages before a target, keeping their order", () => {
    const plan = initialPlan(6);
    const keys = new Set([plan[1].key, plan[3].key]);
    expect(pages(moveItems(plan, keys, 0))).toEqual([1, 3, 0, 2, 4, 5]);
    expect(pages(moveItems(plan, keys, 6))).toEqual([0, 2, 4, 5, 1, 3]);
    // Before page 4 (index 4): the moving pages leave their places first.
    expect(pages(moveItems(plan, keys, 4))).toEqual([0, 2, 1, 3, 4, 5]);
    // Dropping a page where it is changes nothing.
    expect(isUnchanged(moveItems(plan, new Set([plan[2].key]), 2), 6)).toBe(true);
    expect(isUnchanged(moveItems(plan, new Set([plan[2].key]), 3), 6)).toBe(true);
  });

  it("rotates, deletes and duplicates", () => {
    const plan = initialPlan(3);
    const first = new Set([plan[0].key]);
    const turned = rotateItems(plan, first, -1);
    expect(turned[0].turns).toBe(3);
    expect(isUnchanged(rotateItems(turned, first, 1), 3)).toBe(true);
    expect(pages(deleteItems(plan, first))).toEqual([1, 2]);
    const [copied, copies] = duplicateItems(plan, new Set([plan[0].key, plan[1].key]));
    expect(pages(copied)).toEqual([0, 1, 0, 1, 2]);
    expect(copies).toHaveLength(2);
    expect(new Set(copied.map((i) => i.key)).size).toBe(5);
  });

  it("inserts after the selection or at the end", () => {
    const plan = initialPlan(3);
    const blank = newItem({ kind: "blank", width: 100, height: 100 });
    expect(pages(insertAfterSelection(plan, new Set([plan[0].key]), [blank]))).toEqual([0, "b", 1, 2]);
    expect(pages(insertAfterSelection(plan, new Set(), [blank]))).toEqual([0, 1, 2, "b"]);
  });

  it("selects ranges either way", () => {
    const plan = initialPlan(5);
    expect(rangeOfKeys(plan, plan[3].key, plan[1].key)).toEqual(new Set([plan[1].key, plan[2].key, plan[3].key]));
  });

  it("becomes the engine's slots", () => {
    const start = initialPlan(2);
    const plan = rotateItems(start, new Set([start[1].key]), 1);
    expect(toSlots(plan)).toEqual([
      { source: { kind: "page", index: 0 }, turns: 0 },
      { source: { kind: "page", index: 1 }, turns: 1 },
    ]);
  });
});
