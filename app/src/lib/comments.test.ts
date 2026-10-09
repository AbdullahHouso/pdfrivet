import { describe, expect, it } from "vitest";
import type { Annotation } from "./bindings/Annotation";
import { authorsOf, buildThreads, commentKind, emptyFilter, filterThreads, markedText } from "./comments.svelte";
import { makePageText } from "./textSelect";

function annot(id: string, change: Partial<Annotation> = {}): Annotation {
  return {
    id,
    kind: { kind: "note" },
    rect: { left: 0.1, top: 0.1, right: 0.2, bottom: 0.2 },
    color: { r: 0, g: 0, b: 0 },
    opacity: 1,
    width: 1,
    contents: "",
    author: "Sara",
    modified: null,
    editable: true,
    replyTo: null,
    ...change,
  };
}

const at = (top: number) => ({ left: 0.1, top, right: 0.2, bottom: top + 0.05 });

describe("comments", () => {
  it("groups replies under their annotation, in page and reading order", () => {
    const threads = buildThreads([
      [
        3,
        [
          annot("low", { rect: at(0.8) }),
          annot("high", { rect: at(0.1), kind: { kind: "ink", strokes: [] } }),
          annot("r2", { replyTo: "high", modified: "2026-01-02T00:00:00Z" }),
          annot("r1", { replyTo: "high", modified: "2026-01-01T00:00:00Z" }),
          annot("lost", { replyTo: "gone" }),
        ],
      ],
      [0, [annot("first")]],
    ]);
    expect(threads.map((t) => t.annotation.id)).toEqual(["first", "high", "lost", "low"]);
    expect(threads[1].replies.map((r) => r.id)).toEqual(["r1", "r2"]);
    expect(commentKind(threads[1].annotation)).toBe("ink");
  });

  it("filters by kind, author and text", () => {
    const threads = buildThreads([
      [
        0,
        [
          annot("a", { contents: "Check the date" }),
          annot("b", { kind: { kind: "markup", style: "highlight", quads: [] }, author: "Omar" }),
          annot("c", { replyTo: "b", contents: "تم", author: "Lina" }),
        ],
      ],
    ]);
    const ids = (f: Partial<ReturnType<typeof emptyFilter>>) =>
      filterThreads(threads, { ...emptyFilter(), ...f }).map((t) => t.annotation.id);
    expect(ids({ kinds: new Set(["highlight"]) })).toEqual(["b"]);
    expect(ids({ authors: new Set(["Lina"]) })).toEqual(["b"]);
    expect(ids({ text: "DATE" })).toEqual(["a"]);
    expect(ids({ text: "تم" })).toEqual(["b"]);
    expect(authorsOf(threads)).toEqual(["Lina", "Omar", "Sara"]);
  });

  it("finds the text a highlight marks", () => {
    const box = (left: number) => ({ left, top: 0.1, right: left + 0.01, bottom: 0.12 });
    const text = makePageText([
      { char: "a", box: box(0.1) },
      { char: "b", box: box(0.11) },
      { char: " ", flags: 1 },
      { char: "c", box: box(0.13) },
      { char: "d", box: box(0.5) },
    ]);
    expect(markedText(text, [{ left: 0.09, top: 0.09, right: 0.15, bottom: 0.13 }])).toBe("ab c");
  });
});
