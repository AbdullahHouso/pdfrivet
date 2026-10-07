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
