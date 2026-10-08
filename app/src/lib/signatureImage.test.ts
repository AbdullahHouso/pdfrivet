import { describe, expect, it } from "vitest";
import { contentBounds, fitWithin, normalizeStrokes, removeBackground, suggestThreshold } from "./signatureImage";

/** A width × height picture filled with one colour. */
function picture(width: number, height: number, [r, g, b]: number[]) {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < data.length; i += 4) data.set([r, g, b, 255], i);
  return data;
}

describe("signature pictures", () => {
  it("makes the paper transparent and keeps the ink", () => {
    const data = new Uint8ClampedArray([
      245,
      243,
      238,
      255, // paper
      20,
      30,
      120,
      255, // blue ink
      180,
      180,
      180,
      255, // a soft edge
    ]);
    removeBackground(data, 200);
    expect(data[3]).toBe(0);
    expect(data[7]).toBe(255);
    expect(data.slice(4, 7)).toEqual(new Uint8ClampedArray([20, 30, 120]));
    expect(data[11]).toBeGreaterThan(0);
    expect(data[11]).toBeLessThan(255);
  });

  it("crops to the signature with a small margin", () => {
    const data = new Uint8ClampedArray(10 * 10 * 4);
    data[(3 * 10 + 4) * 4 + 3] = 255;
    data[(6 * 10 + 7) * 4 + 3] = 255;
    expect(contentBounds(data, 10, 10, 1)).toEqual({ x: 3, y: 2, width: 6, height: 6 });
    expect(contentBounds(new Uint8ClampedArray(16), 2, 2)).toBeNull();
  });

  it("guesses a threshold below the paper's tone", () => {
    const threshold = suggestThreshold(picture(40, 40, [230, 230, 230]));
    expect(threshold).toBeLessThan(230);
    expect(threshold).toBeGreaterThan(150);
  });

  it("shrinks big pictures, keeping proportions", () => {
    expect(fitWithin(4000, 1000, 1200)).toEqual({ width: 1200, height: 300 });
    expect(fitWithin(300, 100, 1200)).toEqual({ width: 300, height: 100 });
  });

  it("moves drawn strokes to the origin", () => {
    const n = normalizeStrokes([
      [
        { x: 10, y: 20 },
        { x: 30, y: 25 },
      ],
      [{ x: 15, y: 40 }],
    ]);
    expect(n.width).toBe(20);
    expect(n.height).toBe(20);
    expect(n.strokes[1][0]).toEqual({ x: 5, y: 20 });
  });
});
