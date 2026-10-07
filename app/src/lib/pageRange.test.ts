import { describe, expect, it } from "vitest";
import { parsePageRange } from "./pageRange";

describe("parsePageRange", () => {
  it("parses single pages and ranges", () => {
    expect(parsePageRange("1-3, 8, 11-12", 20)).toEqual([0, 1, 2, 7, 10, 11]);
  });

  it("accepts Arabic digits, the Arabic comma and reversed ranges", () => {
    expect(parsePageRange("٣-١، ٥", 10)).toEqual([0, 1, 2, 4]);
  });

  it("removes duplicates and sorts", () => {
    expect(parsePageRange("5 2 2-3", 10)).toEqual([1, 2, 4]);
  });

  it("rejects text and pages outside the document", () => {
    expect(parsePageRange("", 10)).toBeNull();
    expect(parsePageRange("abc", 10)).toBeNull();
    expect(parsePageRange("0", 10)).toBeNull();
    expect(parsePageRange("9-11", 10)).toBeNull();
  });
});
