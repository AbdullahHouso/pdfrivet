import { describe, expect, it } from "vitest";
import type { Annotation } from "./bindings/Annotation";
import { type AnnotationStep, History } from "./history.svelte";

const note = (id: string, contents = ""): Annotation => ({
  id,
  kind: { kind: "note" },
  rect: { left: 0, top: 0, right: 0.1, bottom: 0.1 },
  color: { r: 0, g: 0, b: 0 },
  opacity: 1,
  width: 1,
  contents,
  author: "",
  modified: null,
  editable: true,
  replyTo: null,
});

describe("History", () => {
  it("undoes and redoes in order", () => {
    const h = new History();
    expect(h.canUndo).toBe(false);
    h.record({ page: 0, before: null, after: note("a") });
    h.record({ page: 0, before: note("a"), after: note("a", "hi") });
    expect((h.undo()?.[0] as AnnotationStep | undefined)?.after?.contents).toBe("hi");
    expect(h.canRedo).toBe(true);
    expect((h.undo()?.[0] as AnnotationStep | undefined)?.before).toBeNull();
    expect(h.undo()).toBeUndefined();
    expect((h.redo()?.[0] as AnnotationStep | undefined)?.after?.id).toBe("a");
  });

  it("forgets what was undone after a new change", () => {
    const h = new History();
    h.record({ page: 0, before: null, after: note("a") });
    h.undo();
    h.record({ page: 1, before: null, after: note("b") });
    expect(h.canRedo).toBe(false);
  });

  it("follows an annotation that got a new id", () => {
    const h = new History();
    h.record({ page: 0, before: null, after: note("#3") });
    h.record({ page: 0, before: note("#3"), after: note("#3", "x") });
    h.rename("#3", "pdfrivet-1");
    const [step] = (h.undo() ?? []) as AnnotationStep[];
    expect(step?.before?.id).toBe("pdfrivet-1");
    expect(step?.after?.id).toBe("pdfrivet-1");
  });

  it("undoes a group of changes together", () => {
    const h = new History();
    h.record({ page: 0, before: null, after: note("a") });
    h.beginGroup();
    h.record({ page: 0, before: note("b"), after: null });
    h.record({ page: 1, before: note("c"), after: null });
    h.endGroup();
    expect((h.undo() as AnnotationStep[] | undefined)?.map((s) => s.before?.id)).toEqual(["b", "c"]);
    expect((h.undo() as AnnotationStep[] | undefined)?.map((s) => s.after?.id)).toEqual(["a"]);
  });
});

describe("History with bookmarks", () => {
  it("keeps bookmark steps out of annotation renames", () => {
    const h = new History();
    h.record({ kind: "outline", before: [], after: [] });
    h.rename("a", "b");
    expect(h.undo()?.[0].kind).toBe("outline");
  });
});
