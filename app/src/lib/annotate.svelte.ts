// Annotating: which tool is active, how each tool draws, and the changes
// themselves (each one recorded for undo, the page re-rendered, the tab
// marked as changed).

import type { Annotation } from "./bindings/Annotation";
import type { Color } from "./bindings/Color";
import type { PageRect } from "./bindings/PageRect";
import { type Bookmark, copyTree } from "./outlineTree";
import { forgetPageText, loadPageText } from "./pageText";
import {
  addAnnotation,
  addImageStamp,
  deleteAnnotation,
  getAnnotations,
  type Picture,
  redact,
  restoreAnnotation,
  setOutline,
  updateAnnotation,
  userName,
} from "./pdf";
import { settings, type ToolStyle } from "./settings.svelte";
import { type Signature, signaturePixels } from "./signatures.svelte";
import type { Tab } from "./tabs.svelte";
import { isEmpty, ordered, rangeOnPage, selectionRects } from "./textSelect";

export type AnnotTool =
  | "highlight"
  | "underline"
  | "strikeout"
  | "pen"
  | "rectangle"
  | "ellipse"
  | "line"
  | "arrow"
  | "note"
  | "eraser"
  | "signature"
  | "redact";

/** Tools that mark selected text. */
export const MARKUP_TOOLS = ["highlight", "underline", "strikeout"] as const;
export type MarkupTool = (typeof MARKUP_TOOLS)[number];

export function isMarkupTool(tool: AnnotTool | null): tool is MarkupTool {
  return MARKUP_TOOLS.includes(tool as MarkupTool);
}

/** Tools used on text you select, or on an area you drag out where there's no text (scans). */
export function worksOnText(tool: AnnotTool | null): tool is MarkupTool | "redact" {
  return isMarkupTool(tool) || tool === "redact";
}

const hex = (h: string): Color => ({
  r: Number.parseInt(h.slice(1, 3), 16),
  g: Number.parseInt(h.slice(3, 5), 16),
  b: Number.parseInt(h.slice(5, 7), 16),
});

export function toHex(c: Color): string {
  return `#${[c.r, c.g, c.b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

/** The colours offered in the Annotate toolbar (document content, so fixed, not themed). */
export const PALETTE: Color[] = [
  "#ffd400", // yellow
  "#7ed957", // green
  "#4aa8ff", // blue
  "#ff7eb6", // pink
  "#e53935", // red
  "#ff9800", // orange
  "#8e5cf7", // purple
  "#1b1c20", // black
].map(hex);

const DEFAULTS: Record<AnnotTool, ToolStyle> = {
  highlight: { color: hex("#ffd400"), width: 1, opacity: 1, fill: false },
  underline: { color: hex("#4aa8ff"), width: 1, opacity: 1, fill: false },
  strikeout: { color: hex("#e53935"), width: 1, opacity: 1, fill: false },
  pen: { color: hex("#e53935"), width: 2, opacity: 1, fill: false },
  rectangle: { color: hex("#e53935"), width: 2, opacity: 1, fill: false },
  ellipse: { color: hex("#e53935"), width: 2, opacity: 1, fill: false },
  line: { color: hex("#e53935"), width: 2, opacity: 1, fill: false },
  arrow: { color: hex("#e53935"), width: 2, opacity: 1, fill: false },
  note: { color: hex("#ffd400"), width: 1, opacity: 1, fill: false },
  eraser: { color: hex("#1b1c20"), width: 1, opacity: 1, fill: false },
  signature: { color: hex("#1b1c20"), width: 1, opacity: 1, fill: false },
  redact: { color: hex("#1b1c20"), width: 1, opacity: 1, fill: false },
};

/** A fill that keeps the outline visible: the colour mixed with white. */
export function lighter(c: Color, amount = 0.6): Color {
  const mix = (v: number) => Math.round(v + (255 - v) * amount);
  return { r: mix(c.r), g: mix(c.g), b: mix(c.b) };
}

let open = $state(false);
let tool = $state<AnnotTool | null>(null);
let signature = $state<Signature | null>(null);
let osUser: Promise<string> | null = null;

/** The OS user name, asked for once. */
function osUserName(): Promise<string> {
  osUser ??= userName().catch(() => "");
  return osUser;
}

export const annotate = {
  /** The Annotate toolbar is shown. */
  get open() {
    return open;
  },
  set open(value: boolean) {
    open = value;
    if (!value) {
      tool = null;
      signature = null;
    }
  },
  /** The active tool; `null` selects and moves annotations (and selects text). */
  get tool() {
    return tool;
  },
  set tool(value: AnnotTool | null) {
    tool = value;
    if (value) open = true;
    if (value !== "signature") signature = null;
  },
  /** The signature being placed (with the Signature tool). */
  get signature() {
    return signature;
  },
  /** Starts placing a signature: it follows the pointer until a click puts it down. */
  place(sig: Signature) {
    tool = "signature";
    open = true;
    signature = sig;
  },
  style(t: AnnotTool): ToolStyle {
    return settings.toolStyles[t] ?? DEFAULTS[t];
  },
  setStyle(t: AnnotTool, change: Partial<ToolStyle>) {
    settings.setToolStyle(t, { ...this.style(t), ...change });
  },
  /** Who new annotations are by: the name in Settings, or the OS user name. */
  async author(): Promise<string> {
    return settings.author || (await osUserName());
  },
};

// The changes. Each is recorded for undo, re-renders the page and marks the tab as changed.

function changed(tab: Tab, page: number) {
  tab.dirty = true;
  tab.bumpPage(page);
}

/** Adds an annotation; returns it with its id. */
export async function add(tab: Tab, page: number, a: Annotation): Promise<Annotation> {
  const draft = { ...a, author: a.author || (await annotate.author()) };
  const id = await addAnnotation(tab.docId, page, draft);
  const added = { ...draft, id };
  tab.history.record({ page, before: null, after: added });
  changed(tab, page);
  return added;
}

/** Replaces `before` with `after` (same id); returns the result. */
export async function update(tab: Tab, page: number, before: Annotation, after: Annotation): Promise<Annotation> {
  const id = await updateAnnotation(tab.docId, page, after);
  // An annotation without a name gets one the first time it's changed.
  tab.history.rename(before.id, id);
  const result = { ...after, id };
  tab.history.record({ page, before: { ...before, id }, after: result });
  if (tab.selectedAnnotation?.id === before.id) tab.selectedAnnotation = { page, id };
  changed(tab, page);
  return result;
}

/** Places a picture (a signature) as a stamp; returns it with its id. */
export async function addStamp(tab: Tab, page: number, a: Annotation, picture: Picture): Promise<Annotation> {
  const draft = { ...a, author: a.author || (await annotate.author()) };
  const id = await addImageStamp(tab.docId, page, draft, picture);
  const added = { ...draft, id };
  tab.history.record({ page, before: null, after: added });
  changed(tab, page);
  return added;
}

/**
 * Puts a signature on a page, in `rect` (page fractions). `pxRect` is the
 * same rectangle in screen px and `toFraction` converts screen px on the
 * page, so drawn strokes land exactly where the preview showed them.
 */
export async function placeSignature(
  tab: Tab,
  page: number,
  sig: Signature,
  rect: Annotation["rect"],
  pxRect: { left: number; top: number; width: number; height: number },
  toFraction: (x: number, y: number) => { x: number; y: number },
  pxPerPoint: number,
): Promise<Annotation> {
  const base: Annotation = {
    id: "",
    kind: { kind: "stamp" },
    rect,
    color: { r: 0, g: 0, b: 0 },
    opacity: 1,
    width: 1,
    contents: "",
    author: "",
    modified: null,
    editable: true,
    replyTo: null,
  };
  if (sig.kind === "image") {
    const pixels = await signaturePixels(sig);
    return addStamp(tab, page, base, { width: pixels.width, height: pixels.height, data: pixels.data });
  }
  const factor = pxRect.width / sig.width;
  const strokes = sig.strokes.map((s) =>
    s.map((p) => toFraction(pxRect.left + p.x * factor, pxRect.top + p.y * (pxRect.height / sig.height))),
  );
  return add(tab, page, {
    ...base,
    kind: { kind: "ink", strokes },
    color: sig.color,
    width: (sig.lineWidth * factor) / pxPerPoint,
  });
}

export async function remove(tab: Tab, page: number, a: Annotation) {
  // Its replies go with it (one undo step brings them all back).
  const replies = a.replyTo ? [] : (await getAnnotations(tab.docId, page)).filter((r) => r.replyTo === a.id);
  tab.history.beginGroup();
  try {
    for (const reply of replies) await removeOne(tab, page, reply);
    await removeOne(tab, page, a);
  } finally {
    tab.history.endGroup();
  }
}

async function removeOne(tab: Tab, page: number, a: Annotation) {
  // Deleted annotations stay in the document, hidden, so undo restores them exactly.
  await deleteAnnotation(tab.docId, page, a.id);
  tab.history.record({ page, before: a, after: null });
  if (tab.selectedAnnotation?.id === a.id) tab.selectedAnnotation = null;
  changed(tab, page);
}

/** Changes an annotation's comment. */
export function setComment(tab: Tab, page: number, a: Annotation, text: string): Promise<Annotation> {
  return update(tab, page, a, { ...a, contents: text });
}

/** Answers an annotation's comment; returns the reply. */
export async function reply(tab: Tab, page: number, parent: Annotation, text: string): Promise<Annotation> {
  // Replies point to their annotation by name; one from another app may not have one yet.
  const named = parent.id.startsWith("#") ? await update(tab, page, parent, parent) : parent;
  return add(tab, page, {
    id: "",
    kind: { kind: "note" },
    rect: named.rect,
    color: named.color,
    opacity: 1,
    width: 1,
    contents: text,
    author: "",
    modified: null,
    editable: true,
    replyTo: named.id,
  });
}

async function applyOutline(tab: Tab, items: Bookmark[]) {
  const copy = copyTree(items);
  await setOutline(tab.docId, copy);
  tab.outline = copy;
  tab.dirty = true;
}

/** Applies a step of the history: forwards (redo) or backwards (undo). */
async function apply(tab: Tab, page: number, from: Annotation | null, to: Annotation | null) {
  if (from && to) {
    await updateAnnotation(tab.docId, page, to);
  } else if (to) {
    // Undoing a delete or redoing an add: the annotation is still there, hidden.
    await restoreAnnotation(tab.docId, page, to.id);
  } else if (from) {
    await deleteAnnotation(tab.docId, page, from.id);
  }
  tab.selectedAnnotation = null;
  changed(tab, page);
}

/** Undoes the last change; returns the page it was on (null for bookmark changes). */
export async function undo(tab: Tab): Promise<number | null> {
  const steps = tab.history.undo();
  if (!steps) return null;
  for (const step of [...steps].reverse()) {
    if (step.kind === "outline") await applyOutline(tab, step.before);
    else await apply(tab, step.page, step.after, step.before);
  }
  return steps[0].kind === "outline" ? null : steps[0].page;
}

export async function redo(tab: Tab): Promise<number | null> {
  const steps = tab.history.redo();
  if (!steps) return null;
  for (const step of steps) {
    if (step.kind === "outline") await applyOutline(tab, step.after);
    else await apply(tab, step.page, step.before, step.after);
  }
  return steps[0].kind === "outline" ? null : steps[0].page;
}

const MARKUP_STYLE = { highlight: "highlight", underline: "underline", strikeout: "strikeout" } as const;

/** Highlights, underlines or strikes out the selected text (one annotation per page). */
export async function markSelection(tab: Tab, tool: MarkupTool) {
  const sel = tab.selection;
  if (!sel || isEmpty(sel)) return;
  const [start, end] = ordered(sel);
  const style = annotate.style(tool);
  const marked: number[] = [];
  for (let page = start.page; page <= end.page; page++) {
    const range = rangeOnPage(sel, page);
    if (!range) continue;
    const text = await loadPageText(tab.docId, page);
    const quads = selectionRects(text, range[0], range[1]);
    if (quads.length === 0) continue;
    const rect = {
      left: Math.min(...quads.map((q) => q.left)),
      top: Math.min(...quads.map((q) => q.top)),
      right: Math.max(...quads.map((q) => q.right)),
      bottom: Math.max(...quads.map((q) => q.bottom)),
    };
    await add(tab, page, {
      id: "",
      kind: { kind: "markup", style: MARKUP_STYLE[tool], quads },
      rect,
      color: style.color,
      opacity: style.opacity,
      width: 1,
      contents: "",
      author: "",
      modified: null,
      editable: true,
      replyTo: null,
    });
    marked.push(page);
  }
  // The selection stays until the pages show the marks, so nothing blinks.
  await Promise.all(marked.map((page) => tab.whenPainted(page)));
  if (tab.selection === sel) tab.selection = null;
}

/** Highlights, underlines or strikes out an area of a page (pictures and scans have no text to select). */
export async function markArea(tab: Tab, page: number, tool: MarkupTool, rect: PageRect) {
  const style = annotate.style(tool);
  const added = add(tab, page, {
    id: "",
    kind: { kind: "markup", style: MARKUP_STYLE[tool], quads: [rect] },
    rect,
    color: style.color,
    opacity: style.opacity,
    width: 1,
    contents: "",
    author: "",
    modified: null,
    editable: true,
    replyTo: null,
  });
  await added;
  await tab.whenPainted(page);
}

// --- Redaction ----------------------------------------------------------------
// Areas are only marked at first (shown outlined, and can be removed again);
// they are applied, permanently, when the document is saved or with "Apply".

let nextMark = 1;

/** Marks areas of a page for redaction. */
export function markForRedaction(tab: Tab, page: number, rects: PageRect[]) {
  if (rects.length === 0) return;
  tab.redactions = [...tab.redactions, ...rects.map((rect) => ({ id: nextMark++, page, rect }))];
  tab.dirty = true;
}

/** Marks the selected text for redaction. */
export async function markSelectionForRedaction(tab: Tab) {
  const sel = tab.selection;
  if (!sel || isEmpty(sel)) return;
  const [start, end] = ordered(sel);
  for (let page = start.page; page <= end.page; page++) {
    const range = rangeOnPage(sel, page);
    if (!range) continue;
    const text = await loadPageText(tab.docId, page);
    markForRedaction(tab, page, selectionRects(text, range[0], range[1]));
  }
  tab.selection = null;
}

export function unmarkRedaction(tab: Tab, id: number) {
  tab.redactions = tab.redactions.filter((r) => r.id !== id);
}

/** Applies every marked redaction, permanently. Undo can't bring the content back. */
export async function applyRedactions(tab: Tab) {
  const byPage = new Map<number, PageRect[]>();
  for (const r of tab.redactions) byPage.set(r.page, [...(byPage.get(r.page) ?? []), r.rect]);
  for (const [page, rects] of byPage) {
    await redact(tab.docId, page, rects);
    tab.redactions = tab.redactions.filter((r) => r.page !== page);
    changed(tab, page);
  }
  // The pages' text changed: search and selection read it again.
  forgetPageText(tab.docId);
  tab.selection = null;
  tab.search.clear();
}
