// Printing: the print dialog prints whatever the webview shows, so we build a
// hidden "print root" with one image per PDF page (rendered by PDFium at print
// resolution), hide the app with a print stylesheet, and open the dialog.

import type { PageSize } from "./bindings/PageSize";

const PROTOCOL_BASE = /Windows|Android/.test(navigator.userAgent) ? "http://rivet.localhost" : "rivet://localhost";

/** Print resolution: sharp text, while long documents stay within memory. */
function dpiFor(pageCount: number): number {
  return pageCount > 200 ? 150 : 200;
}

export interface PrintProgress {
  done: number;
  total: number;
}

/**
 * Prepares the given pages (0-based) and opens the system print dialog.
 * Resolves when the dialog closes; rejects with an AbortError if cancelled.
 */
export async function printDocument(
  docId: number,
  pageSizes: PageSize[],
  pages: number[],
  onprogress: (p: PrintProgress) => void,
  signal: AbortSignal,
): Promise<void> {
  const dpi = dpiFor(pages.length);
  const root = document.createElement("div");
  root.id = "print-root";
  const style = document.createElement("style");

  // Each paper size gets a named @page rule, so mixed page sizes print correctly.
  const pageNames = new Map<string, string>();
  const rules: string[] = [];
  const images = pages.map((index) => {
    const size = pageSizes[index];
    const key = `${size.width.toFixed(1)}x${size.height.toFixed(1)}`;
    let name = pageNames.get(key);
    if (!name) {
      name = `rivet-page-${pageNames.size}`;
      pageNames.set(key, name);
      rules.push(`@page ${name} { size: ${size.width}pt ${size.height}pt; margin: 0; }`);
    }
    const img = new Image();
    img.className = "print-page";
    img.alt = "";
    // A hair smaller than the sheet: an image exactly as tall as the page can
    // round up and push an empty page after it.
    img.style.width = `${size.width - 1}pt`;
    img.style.height = `${size.height - 1}pt`;
    img.style.setProperty("page", name);
    img.dataset.src = `${PROTOCOL_BASE}/print/${docId}/${index}?dpi=${dpi}`;
    root.append(img);
    return img;
  });
  style.textContent = rules.join("\n");

  const cleanup = () => {
    root.remove();
    style.remove();
  };

  try {
    // Load a few pages at a time; the engine renders them one after another.
    let done = 0;
    let next = 0;
    onprogress({ done, total: images.length });
    const worker = async () => {
      while (next < images.length) {
        if (signal.aborted) throw new DOMException("Printing cancelled", "AbortError");
        const img = images[next++];
        img.src = img.dataset.src ?? "";
        await img.decode();
        onprogress({ done: ++done, total: images.length });
      }
    };
    await Promise.all([worker(), worker(), worker()]);
    if (signal.aborted) throw new DOMException("Printing cancelled", "AbortError");

    document.head.append(style);
    document.body.append(root);
    await new Promise<void>((resolve) => {
      window.addEventListener("afterprint", () => resolve(), { once: true });
      window.print();
    });
  } finally {
    cleanup();
  }
}
