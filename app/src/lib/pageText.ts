// The text of recently used pages (characters and their boxes), kept so the
// pointer can be matched against it synchronously while selecting. Text
// doesn't change when forms are filled or annotations added, so entries stay
// valid until the document closes.

import { fetchPageText } from "./pdf";
import { type PageText, parsePageText } from "./textSelect";

/** Pages kept; a dense page is about 70 KB. */
const MAX_PAGES = 24;

const pending = new Map<string, Promise<PageText>>();
const loaded = new Map<string, PageText>();

const key = (docId: number, page: number) => `${docId}/${page}`;

/** The page's text if it's already loaded (marks it as recently used). */
export function peekPageText(docId: number, page: number): PageText | undefined {
  const k = key(docId, page);
  const text = loaded.get(k);
  if (text) {
    loaded.delete(k);
    loaded.set(k, text);
  }
  return text;
}

/** Loads the page's text (once; later calls share the same request). */
export function loadPageText(docId: number, page: number): Promise<PageText> {
  const k = key(docId, page);
  const ready = peekPageText(docId, page);
  if (ready) return Promise.resolve(ready);
  let request = pending.get(k);
  if (!request) {
    request = fetchPageText(docId, page)
      .then((buffer) => {
        const text = parsePageText(buffer);
        loaded.set(k, text);
        while (loaded.size > MAX_PAGES) loaded.delete(loaded.keys().next().value as string);
        return text;
      })
      .finally(() => pending.delete(k));
    pending.set(k, request);
  }
  return request;
}

/** Drops a closed document's pages. */
export function forgetPageText(docId: number) {
  for (const k of [...loaded.keys()]) if (k.startsWith(`${docId}/`)) loaded.delete(k);
}
