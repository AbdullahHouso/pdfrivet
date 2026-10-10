import { describe, expect, it } from "vitest";
import { folderOf, formatSize, pageImageNames, uniqueNames } from "./files";

describe("tool output files", () => {
  it("finds the folder", () => {
    expect(folderOf("C:\\Users\\a\\doc.pdf")).toBe("C:\\Users\\a");
    expect(folderOf("/home/a/doc.pdf")).toBe("/home/a");
  });

  it("never reuses a name", () => {
    expect(uniqueNames(["a.png", "A.png", "b.png"], new Set(["b.png"]))).toEqual(["a.png", "A (2).png", "b (2).png"]);
  });

  it("numbers page pictures so they sort", () => {
    expect(pageImageNames("doc", [0, 9, 119], 120, "png")).toEqual(["doc-p001.png", "doc-p010.png", "doc-p120.png"]);
  });
});

describe("file sizes", () => {
  it("reads like people write them", () => {
    expect(formatSize(12_400_000, "en")).toBe("12 MB");
    expect(formatSize(3_140_000, "en")).toBe("3.1 MB");
    expect(formatSize(830_000, "en")).toBe("830 kB");
    expect(formatSize(512, "en")).toBe("512 byte");
  });
});
