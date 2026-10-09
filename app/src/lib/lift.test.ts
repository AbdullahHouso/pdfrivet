import { describe, expect, it } from "vitest";
import { liftedPixels } from "./lift";

const image = (pixels: number[][]) => new Uint8ClampedArray(pixels.flat());

describe("lifting an annotation", () => {
  it("keeps only what differs from the page without it", () => {
    const page = [200, 180, 160, 255];
    const before = image([[0, 0, 0, 255], page, [190, 175, 160, 255]]);
    const after = image([page, page, page]);
    const lifted = liftedPixels(before, after);
    expect([...lifted.slice(0, 4)]).toEqual([0, 0, 0, 255]);
    // Same as the page, or almost (compression noise): transparent.
    expect(lifted[7]).toBe(0);
    expect(lifted[11]).toBe(0);
  });
});
