// Page ranges typed by people, e.g. "1-5, 8, 11-13". Pure and unit-tested.

/** Converts Arabic-Indic and Persian digits to 0-9. */
export function westernDigits(text: string): string {
  return text.replace(/[٠-٩۰-۹]/g, (d) => String((d.charCodeAt(0) & 0xf) % 10));
}

/**
 * Parses a page range (1-based, as typed) into sorted, unique 0-based page indexes.
 * Accepts commas (also the Arabic comma "،"), spaces, and ranges with "-" or "–".
 * Returns null if the text is invalid or mentions pages outside 1..pageCount.
 */
export function parsePageRange(text: string, pageCount: number): number[] | null {
  const parts = westernDigits(text)
    .split(/[,،;\s]+/)
    .filter(Boolean);
  if (parts.length === 0) return null;
  const pages = new Set<number>();
  for (const part of parts) {
    const match = /^(\d+)(?:\s*[-–]\s*(\d+))?$/.exec(part);
    if (!match) return null;
    const from = Number(match[1]);
    const to = match[2] === undefined ? from : Number(match[2]);
    if (from < 1 || to < 1 || from > pageCount || to > pageCount) return null;
    for (let p = Math.min(from, to); p <= Math.max(from, to); p++) pages.add(p - 1);
  }
  return [...pages].sort((a, b) => a - b);
}

/** Writes 0-based pages as a short 1-based range for people: [0, 1, 2, 6] is "1-3, 7". */
export function pageList(pages: number[]): string {
  const sorted = [...new Set(pages)].sort((a, b) => a - b);
  const parts: string[] = [];
  for (let i = 0; i < sorted.length; i++) {
    let j = i;
    while (j + 1 < sorted.length && sorted[j + 1] === sorted[j] + 1) j++;
    parts.push(j > i ? `${sorted[i] + 1}-${sorted[j] + 1}` : `${sorted[i] + 1}`);
    i = j;
  }
  return parts.join(", ");
}
