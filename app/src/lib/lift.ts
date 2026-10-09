// The pixels of an annotation as drawn on the page: where the page with it
// (`before`) differs from the page without it (`after`). Everything else
// becomes transparent, so the annotation can be moved over the page and look
// exactly as it did (its own font, colours and shapes, whoever drew it).

/** How different two pixels must be (sum of the RGB differences) to belong to the annotation. */
const THRESHOLD = 24;

export function liftedPixels(a: Uint8ClampedArray, b: Uint8ClampedArray): Uint8ClampedArray<ArrayBuffer> {
  const o = new Uint8ClampedArray(a.length);
  for (let i = 0; i < a.length; i += 4) {
    const diff = Math.abs(a[i] - b[i]) + Math.abs(a[i + 1] - b[i + 1]) + Math.abs(a[i + 2] - b[i + 2]);
    if (diff > THRESHOLD) {
      o[i] = a[i];
      o[i + 1] = a[i + 1];
      o[i + 2] = a[i + 2];
      o[i + 3] = 255;
    }
  }
  return o;
}
