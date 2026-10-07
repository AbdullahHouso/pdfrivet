import { describe, expect, it } from "vitest";
import { choosePages } from "./printing";

describe("choosePages", () => {
  it("chooses all, current or a range", () => {
    expect(choosePages("all", 0, 4, "", "all", false)).toEqual([0, 1, 2, 3]);
    expect(choosePages("current", 2, 4, "", "all", false)).toEqual([2]);
    expect(choosePages("custom", 0, 10, "2-4", "all", false)).toEqual([1, 2, 3]);
  });

  it("keeps odd or even page numbers and can reverse", () => {
    expect(choosePages("all", 0, 5, "", "odd", false)).toEqual([0, 2, 4]);
    expect(choosePages("all", 0, 5, "", "even", true)).toEqual([3, 1]);
  });

  it("reports invalid ranges", () => {
    expect(choosePages("custom", 0, 3, "7", "all", false)).toBeNull();
  });
});
