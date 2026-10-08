// Turning a photo or scan of a signature into a clean picture: the paper
// becomes transparent, the ink keeps its colour, and the empty space around
// the signature is cropped away. Pure functions on RGBA pixels (tested).

/** Perceived brightness of a pixel, 0–255. */
function luminance(r: number, g: number, b: number): number {
  return 0.299 * r + 0.587 * g + 0.114 * b;
}

/**
 * Makes light pixels (the paper) transparent. Pixels brighter than
 * `threshold` disappear; darker ones stay, with a soft edge `softness` wide so
 * strokes stay smooth. Works in place on RGBA pixels.
 */
export function removeBackground(data: Uint8ClampedArray, threshold: number, softness = 40) {
  for (let i = 0; i < data.length; i += 4) {
    const lum = luminance(data[i], data[i + 1], data[i + 2]);
    const keep = Math.max(0, Math.min(1, (threshold - lum) / softness));
    data[i + 3] = Math.round(data[i + 3] * keep);
  }
}

/** The smallest rectangle around pixels that aren't transparent, with a margin; null if there are none. */
export function contentBounds(data: Uint8ClampedArray, width: number, height: number, margin = 4) {
  let left = width;
  let top = height;
  let right = -1;
  let bottom = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (data[(y * width + x) * 4 + 3] > 16) {
        if (x < left) left = x;
        if (x > right) right = x;
        if (y < top) top = y;
        if (y > bottom) bottom = y;
      }
    }
  }
  if (right < 0) return null;
  left = Math.max(0, left - margin);
  top = Math.max(0, top - margin);
  right = Math.min(width - 1, right + margin);
  bottom = Math.min(height - 1, bottom + margin);
  return { x: left, y: top, width: right - left + 1, height: bottom - top + 1 };
}

/** A size that fits within `max` on both sides, keeping the proportions (never enlarged). */
export function fitWithin(width: number, height: number, max: number) {
  const factor = Math.min(1, max / Math.max(width, height));
  return { width: Math.max(1, Math.round(width * factor)), height: Math.max(1, Math.round(height * factor)) };
}

/** A guess for the threshold: a bit darker than the paper's brightness (the brightest common tone). */
export function suggestThreshold(data: Uint8ClampedArray): number {
  const histogram = new Array(256).fill(0);
  for (let i = 0; i < data.length; i += 16) {
    histogram[Math.round(luminance(data[i], data[i + 1], data[i + 2]))]++;
  }
  // The paper is the most common tone in the brighter half.
  let paper = 255;
  let most = -1;
  for (let v = 128; v < 256; v++) {
    if (histogram[v] > most) {
      most = histogram[v];
      paper = v;
    }
  }
  return Math.max(60, paper - 45);
}

/** Moves strokes so their bounding box starts at (0, 0); returns them with the box's size. */
export function normalizeStrokes(strokes: { x: number; y: number }[][]) {
  const all = strokes.flat();
  if (all.length === 0) return { strokes: [], width: 0, height: 0 };
  const minX = Math.min(...all.map((p) => p.x));
  const minY = Math.min(...all.map((p) => p.y));
  const maxX = Math.max(...all.map((p) => p.x));
  const maxY = Math.max(...all.map((p) => p.y));
  return {
    strokes: strokes.map((s) => s.map((p) => ({ x: p.x - minX, y: p.y - minY }))),
    width: Math.max(1, maxX - minX),
    height: Math.max(1, maxY - minY),
  };
}
