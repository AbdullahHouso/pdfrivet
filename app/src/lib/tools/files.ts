// Naming the files tools write: next to the document by default, never
// over an existing file.

import { filesExist } from "../pdf";

export const fileNameOf = (path: string) => path.split(/[\\/]/).at(-1) ?? path;

/** The folder of a path, without the trailing separator. */
export function folderOf(path: string): string {
  return path.slice(0, path.length - fileNameOf(path).length).replace(/[\\/]$/, "");
}

/** The separator a path uses (Windows or not). */
export const separatorOf = (path: string) => (path.includes("\\") ? "\\" : "/");

/**
 * Makes names unique among themselves and against `taken` (case-insensitive,
 * as on Windows and macOS): "page.png", then "page (2).png"…
 */
export function uniqueNames(names: string[], taken: Set<string>): string[] {
  const used = new Set([...taken].map((t) => t.toLowerCase()));
  return names.map((name) => {
    const dot = name.lastIndexOf(".");
    const [stem, ext] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
    let candidate = name;
    for (let k = 2; used.has(candidate.toLowerCase()); k++) candidate = `${stem} (${k})${ext}`;
    used.add(candidate.toLowerCase());
    return candidate;
  });
}

/** Full paths in `folder` for `names`, none of them an existing file. */
export async function freePaths(folder: string, names: string[], separator: string): Promise<string[]> {
  const taken = new Set<string>();
  for (let tries = 0; tries < 8; tries++) {
    const candidates = uniqueNames(names, taken);
    const exists = await filesExist(candidates.map((n) => folder + separator + n));
    const clash = candidates.filter((_, i) => exists[i]);
    if (clash.length === 0) return candidates.map((n) => folder + separator + n);
    for (const n of clash) taken.add(n);
  }
  throw new Error("couldn't find free file names");
}

/** File names for page pictures: "report-p001.png" (numbers padded so they sort). */
export function pageImageNames(base: string, pages: number[], pageCount: number, ext: string): string[] {
  const width = String(pageCount).length;
  return pages.map((p) => `${base}-p${String(p + 1).padStart(width, "0")}.${ext}`);
}
