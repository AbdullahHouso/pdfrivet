import { describe, expect, it } from "vitest";
import {
  canMove,
  dragHandle,
  fromPx,
  hits,
  moved,
  type PageBox,
  pxToRect,
  rectToPx,
  resized,
  rotatePoint,
  simplify,
  toPx,
  topmostAt,
} from "./annotGeometry";
import type { Annotation } from "./bindings/Annotation";
import { unrotatePoint } from "./layout";

const box: PageBox = { width: 1000, height: 2000, rotation: 0 };

function annotation(kind: Annotation["kind"], rect = { left: 0.1, top: 0.1, right: 0.3, bottom: 0.2 }): Annotation {
  return {
    id: "a",
    kind,
    rect,
    color: { r: 255, g: 0, b: 0 },
    opacity: 1,
    width: 2,
    contents: "",
    author: "",
    modified: null,
    editable: true,
    replyTo: null,
  };
}

describe("rotation", () => {
  it("rotatePoint and unrotatePoint undo each other", () => {
    for (const rotation of [0, 90, 180, 270] as const) {
      const p = { x: 0.2, y: 0.7 };
      const back = unrotatePoint(rotatePoint(p, rotation), rotation);
      expect(back.x).toBeCloseTo(p.x);
      expect(back.y).toBeCloseTo(p.y);
    }
  });

  it("converts between px and fractions on a rotated page", () => {
    const turned: PageBox = { width: 2000, height: 1000, rotation: 90 };
    const p = { x: 0.25, y: 0.1 };
    const px = toPx(p, turned);
    // A quarter turn clockwise: the top-left area moves to the top-right.
    expect(px.x).toBeCloseTo(1800);
    expect(px.y).toBeCloseTo(250);
    const back = fromPx(px.x, px.y, turned);
    expect(back.x).toBeCloseTo(p.x);
    expect(back.y).toBeCloseTo(p.y);
    const r = { left: 0.1, top: 0.2, right: 0.3, bottom: 0.4 };
    const again = pxToRect(rectToPx(r, turned), turned);
    expect(again.left).toBeCloseTo(r.left);
    expect(again.bottom).toBeCloseTo(r.bottom);
  });
});

describe("hit testing", () => {
  it("finds strokes only near the line", () => {
    const ink = annotation({
      kind: "ink",
      strokes: [
        [
          { x: 0.1, y: 0.1 },
          { x: 0.3, y: 0.1 },
        ],
      ],
    });
    expect(hits(ink, 200, 203, box)).toBe(true); // 3 px from the stroke
    expect(hits(ink, 200, 230, box)).toBe(false);
  });

  it("finds unfilled shapes by their outline and filled ones anywhere", () => {
    const empty = annotation({ kind: "square", fill: null });
    expect(hits(empty, 101, 300, box)).toBe(true); // on the left edge
    expect(hits(empty, 200, 300, box)).toBe(false); // in the middle
    const filled = annotation({ kind: "square", fill: { r: 1, g: 1, b: 1 } });
    expect(hits(filled, 200, 300, box)).toBe(true);
    const ellipse = annotation({ kind: "circle", fill: null });
    expect(hits(ellipse, 200, 201, box)).toBe(true); // top of the ellipse
    expect(hits(ellipse, 200, 300, box)).toBe(false); // its centre
  });

  it("finds text markup on any of its lines", () => {
    const quads = [
      { left: 0.1, top: 0.1, right: 0.5, bottom: 0.11 },
      { left: 0.1, top: 0.12, right: 0.3, bottom: 0.13 },
    ];
    const mark = annotation({ kind: "markup", style: "highlight", quads });
    expect(hits(mark, 250, 250, box)).toBe(true);
    expect(hits(mark, 450, 250, box)).toBe(false); // past the end of line 2
  });

  it("picks the topmost annotation", () => {
    const below = { ...annotation({ kind: "note" }), id: "below" };
    const above = { ...annotation({ kind: "note" }), id: "above" };
    expect(topmostAt([below, above], 150, 300, box)?.id).toBe("above");
    expect(topmostAt([below, above], 900, 1900, box)).toBeNull();
  });
});

describe("moving and resizing", () => {
  it("moves every point of a drawing", () => {
    const line = annotation({ kind: "line", from: { x: 0.1, y: 0.1 }, to: { x: 0.3, y: 0.2 }, arrow: true });
    const m = moved(line, 0.1, 0.05);
    if (m.kind.kind !== "line") throw new Error("still a line");
    expect(m.kind.from.x).toBeCloseTo(0.2);
    expect(m.kind.from.y).toBeCloseTo(0.15);
    expect(m.kind.to.x).toBeCloseTo(0.4);
    expect(m.kind.to.y).toBeCloseTo(0.25);
    expect(m.kind.arrow).toBe(true);
    expect(m.rect.left).toBeCloseTo(0.2);
  });

  it("stretches drawings with their frame", () => {
    const ink = annotation({
      kind: "ink",
      strokes: [
        [
          { x: 0.1, y: 0.1 },
          { x: 0.3, y: 0.2 },
        ],
      ],
    });
    const r = resized(ink, { left: 0.1, top: 0.1, right: 0.5, bottom: 0.3 });
    expect(r.kind.kind === "ink" && r.kind.strokes[0][1]).toEqual({ x: 0.5, y: 0.3 });
  });

  it("drags handles without turning the frame inside out", () => {
    const r = { left: 100, top: 100, width: 50, height: 40 };
    expect(dragHandle(r, "se", 10, 20)).toEqual({ left: 100, top: 100, width: 60, height: 60 });
    expect(dragHandle(r, "w", 200, 0).width).toBe(8);
    expect(dragHandle(r, "n", 0, -10)).toEqual({ left: 100, top: 90, width: 50, height: 50 });
  });

  it("only moves kinds that aren't tied to text or another app", () => {
    expect(canMove(annotation({ kind: "note" }))).toBe(true);
    expect(canMove(annotation({ kind: "markup", style: "underline", quads: [] }))).toBe(false);
    expect(canMove({ ...annotation({ kind: "note" }), editable: false })).toBe(false);
  });
});

describe("simplify", () => {
  it("drops points that don't change the shape", () => {
    const straight = Array.from({ length: 50 }, (_, i) => ({ x: i, y: i * 0.5 }));
    expect(simplify(straight, 0.5)).toHaveLength(2);
    const corner = [
      { x: 0, y: 0 },
      { x: 5, y: 0.1 },
      { x: 10, y: 0 },
      { x: 10, y: 10 },
    ];
    expect(simplify(corner, 0.5)).toEqual([corner[0], corner[2], corner[3]]);
  });
});
