// Annotating: which tool is active, how each tool draws, and the changes
// themselves (each one recorded for undo, the page re-rendered, the tab
// marked as changed).

import type { Annotation } from "./bindings/Annotation";
import type { Color } from "./bindings/Color";
import { loadPageText } from "./pageText";
import { addAnnotation, deleteAnnotation, updateAnnotation, userName } from "./pdf";
import { settings, type ToolStyle } from "./settings.svelte";
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
  | "eraser";

/** Tools that mark selected text. */
export const MARKUP_TOOLS = ["highlight", "underline", "strikeout"] as const;
export type MarkupTool = (typeof MARKUP_TOOLS)[number];

export function isMarkupTool(tool: AnnotTool | null): tool is MarkupTool {
  return MARKUP_TOOLS.includes(tool as MarkupTool);
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
};

/** A fill that keeps the outline visible: the colour mixed with white. */
export function lighter(c: Color, amount = 0.6): Color {
  const mix = (v: number) => Math.round(v + (255 - v) * amount);
  return { r: mix(c.r), g: mix(c.g), b: mix(c.b) };
}

let open = $state(false);
let tool = $state<AnnotTool | null>(null);
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
    if (!value) tool = null;
  },
  /** The active tool; `null` selects and moves annotations (and selects text). */
  get tool() {
    return tool;
  },
  set tool(value: AnnotTool | null) {
    tool = value;
    if (value) open = true;
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

export async function remove(tab: Tab, page: number, a: Annotation) {
  await deleteAnnotation(tab.docId, page, a.id);
  // Other apps' kinds can't be added back, so deleting them can't be undone.
  if (a.editable) tab.history.record({ page, before: a, after: null });
  if (tab.selectedAnnotation?.id === a.id) tab.selectedAnnotation = null;
  changed(tab, page);
}

/** Applies a step of the history: forwards (redo) or backwards (undo). */
async function apply(tab: Tab, page: number, from: Annotation | null, to: Annotation | null) {
  if (from && to) {
    await updateAnnotation(tab.docId, page, to);
  } else if (to) {
    const id = await addAnnotation(tab.docId, page, to);
    tab.history.rename(to.id, id);
  } else if (from) {
    await deleteAnnotation(tab.docId, page, from.id);
  }
  tab.selectedAnnotation = null;
  changed(tab, page);
}

export async function undo(tab: Tab): Promise<number | null> {
  const step = tab.history.undo();
  if (!step) return null;
  await apply(tab, step.page, step.after, step.before);
  return step.page;
}

export async function redo(tab: Tab): Promise<number | null> {
  const step = tab.history.redo();
  if (!step) return null;
  await apply(tab, step.page, step.before, step.after);
  return step.page;
}

const MARKUP_STYLE = { highlight: "highlight", underline: "underline", strikeout: "strikeout" } as const;

/** Highlights, underlines or strikes out the selected text (one annotation per page). */
export async function markSelection(tab: Tab, tool: MarkupTool) {
  const sel = tab.selection;
  if (!sel || isEmpty(sel)) return;
  const [start, end] = ordered(sel);
  const style = annotate.style(tool);
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
    });
  }
  tab.selection = null;
}
