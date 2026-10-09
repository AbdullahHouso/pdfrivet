// Geometry for drawing and editing annotations on a page. Annotations come
// from the engine in page fractions of the upright page (like links and
// selections); on screen a page may be rotated, so points go through
// `rotatePoint` / `unrotatePoint` on the way in and out. Everything here is
// pure and unit-tested.

import type { Annotation } from "./bindings/Annotation";
import type { PagePoint } from "./bindings/PagePoint";
import type { PageRect } from "./bindings/PageRect";
import { type Degrees, type FractionPoint, rotateRect, unrotatePoint } from "./layout";

/** Where a point of the upright page appears after rotating the view clockwise. */
export function rotatePoint(p: FractionPoint, rotation: Degrees): FractionPoint {
  switch (rotation) {
    case 90:
      return { x: 1 - p.y, y: p.x };
    case 180:
      return { x: 1 - p.x, y: 1 - p.y };
    case 270:
      return { x: p.y, y: 1 - p.x };
    default:
      return p;
  }
}

/** The page as shown: its size on screen (px) and the view rotation. */
export interface PageBox {
  width: number;
  height: number;
  rotation: Degrees;
}

/** An upright-page fraction → px on the page as shown. */
export function toPx(p: PagePoint, box: PageBox): { x: number; y: number } {
  const r = rotatePoint(p, box.rotation);
  return { x: r.x * box.width, y: r.y * box.height };
}

/** px on the page as shown → upright-page fraction. */
export function fromPx(x: number, y: number, box: PageBox): PagePoint {
  return unrotatePoint({ x: x / box.width, y: y / box.height }, box.rotation);
}

/** An upright-page rectangle → px rectangle on the page as shown. */
export function rectToPx(r: PageRect, box: PageBox) {
  const s = rotateRect(r, box.rotation);
  return {
    left: s.left * box.width,
    top: s.top * box.height,
    width: (s.right - s.left) * box.width,
    height: (s.bottom - s.top) * box.height,
  };
}

/** The rectangle spanned by two points (any order). */
export function rectFrom(a: PagePoint, b: PagePoint): PageRect {
  return {
    left: Math.min(a.x, b.x),
    top: Math.min(a.y, b.y),
    right: Math.max(a.x, b.x),
    bottom: Math.max(a.y, b.y),
  };
}

function distanceToSegment(p: { x: number; y: number }, a: { x: number; y: number }, b: { x: number; y: number }) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const length2 = dx * dx + dy * dy;
  const t = length2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / length2));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

function distanceToPolyline(p: { x: number; y: number }, points: { x: number; y: number }[]) {
  if (points.length === 1) return Math.hypot(p.x - points[0].x, p.y - points[0].y);
  let best = Number.POSITIVE_INFINITY;
  for (let i = 1; i < points.length; i++) best = Math.min(best, distanceToSegment(p, points[i - 1], points[i]));
  return best;
}

function inside(p: { x: number; y: number }, r: { left: number; top: number; width: number; height: number }, pad = 0) {
  return p.x >= r.left - pad && p.x <= r.left + r.width + pad && p.y >= r.top - pad && p.y <= r.top + r.height + pad;
}

/**
 * Whether a point (px on the page as shown) touches an annotation. Lines and
 * drawings must be hit near their strokes (within `tolerance` px); shapes
 * without a fill near their outline; everything else anywhere inside.
 */
export function hits(a: Annotation, x: number, y: number, box: PageBox, tolerance = 6): boolean {
  const p = { x, y };
  const kind = a.kind;
  switch (kind.kind) {
    case "markup":
      return kind.quads.some((q) => inside(p, rectToPx(q, box), 2));
    case "ink":
      return kind.strokes.some(
        (s) =>
          distanceToPolyline(
            p,
            s.map((pt) => toPx(pt, box)),
          ) <= tolerance,
      );
    case "line":
      return distanceToPolyline(p, [toPx(kind.from, box), toPx(kind.to, box)]) <= tolerance;
    case "square":
    case "circle": {
      const r = rectToPx(a.rect, box);
      if (kind.fill) return inside(p, r);
      if (!inside(p, r, tolerance)) return false;
      if (kind.kind === "square") {
        // Near one of the four edges.
        return (
          Math.min(
            Math.abs(p.x - r.left),
            Math.abs(p.x - (r.left + r.width)),
            Math.abs(p.y - r.top),
            Math.abs(p.y - (r.top + r.height)),
          ) <= tolerance
        );
      }
      // Near the ellipse's outline.
      const rx = r.width / 2;
      const ry = r.height / 2;
      if (rx < 1 || ry < 1) return true;
      const nx = (p.x - (r.left + rx)) / rx;
      const ny = (p.y - (r.top + ry)) / ry;
      const d = Math.hypot(nx, ny);
      return Math.abs(d - 1) * Math.min(rx, ry) <= tolerance;
    }
    default:
      return inside(p, rectToPx(a.rect, box));
  }
}

/** The topmost annotation under a point (annotations are drawn in order, so the last one wins). */
export function topmostAt(list: Annotation[], x: number, y: number, box: PageBox): Annotation | null {
  for (let i = list.length - 1; i >= 0; i--) if (hits(list[i], x, y, box)) return list[i];
  return null;
}

/** Whether an annotation can be moved and resized (text markup stays on its text). */
export function canMove(a: Annotation): boolean {
  return a.editable && ["ink", "line", "square", "circle", "note", "stamp", "freeText"].includes(a.kind.kind);
}

/** Whether only its position changes when resized (a note's icon keeps its size). */
export function keepsSize(a: Annotation): boolean {
  return a.kind.kind === "note";
}

function moveRect(r: PageRect, dx: number, dy: number): PageRect {
  return { left: r.left + dx, top: r.top + dy, right: r.right + dx, bottom: r.bottom + dy };
}

const movePoint = (p: PagePoint, dx: number, dy: number): PagePoint => ({ x: p.x + dx, y: p.y + dy });

/** The annotation moved by (dx, dy) page fractions. */
export function moved(a: Annotation, dx: number, dy: number): Annotation {
  const kind = a.kind;
  const rect = moveRect(a.rect, dx, dy);
  switch (kind.kind) {
    case "ink":
      return { ...a, rect, kind: { ...kind, strokes: kind.strokes.map((s) => s.map((p) => movePoint(p, dx, dy))) } };
    case "line":
      return { ...a, rect, kind: { ...kind, from: movePoint(kind.from, dx, dy), to: movePoint(kind.to, dx, dy) } };
    default:
      return { ...a, rect };
  }
}

/** The annotation stretched from its rectangle into `to`. */
export function resized(a: Annotation, to: PageRect): Annotation {
  const from = a.rect;
  const w = from.right - from.left || 1e-6;
  const h = from.bottom - from.top || 1e-6;
  const map = (p: PagePoint): PagePoint => ({
    x: to.left + ((p.x - from.left) / w) * (to.right - to.left),
    y: to.top + ((p.y - from.top) / h) * (to.bottom - to.top),
  });
  const kind = a.kind;
  switch (kind.kind) {
    case "ink":
      return { ...a, rect: to, kind: { ...kind, strokes: kind.strokes.map((s) => s.map(map)) } };
    case "line":
      return { ...a, rect: to, kind: { ...kind, from: map(kind.from), to: map(kind.to) } };
    default:
      return { ...a, rect: to };
  }
}

/** Which corner or edge of a selection frame is dragged ("nw", "n", "ne", "e", "se", "s", "sw", "w"). */
export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

/**
 * A px rectangle on the page as shown, with one handle dragged by (dx, dy) px.
 * The result never turns inside out and keeps a minimum size.
 */
export function dragHandle(
  r: { left: number; top: number; width: number; height: number },
  handle: Handle,
  dx: number,
  dy: number,
  min = 8,
) {
  let { left, top } = r;
  let right = r.left + r.width;
  let bottom = r.top + r.height;
  if (handle.includes("w")) left = Math.min(left + dx, right - min);
  if (handle.includes("e")) right = Math.max(right + dx, left + min);
  if (handle.includes("n")) top = Math.min(top + dy, bottom - min);
  if (handle.includes("s")) bottom = Math.max(bottom + dy, top + min);
  return { left, top, width: right - left, height: bottom - top };
}

/** A px rectangle on the page as shown → upright-page fractions. */
export function pxToRect(r: { left: number; top: number; width: number; height: number }, box: PageBox): PageRect {
  return rectFrom(fromPx(r.left, r.top, box), fromPx(r.left + r.width, r.top + r.height, box));
}

/**
 * Fewer points along a freehand stroke (Ramer–Douglas–Peucker): points that
 * add less than `tolerance` (same units as the points) are dropped. Keeps
 * files small and lines smooth.
 */
export function simplify<P extends { x: number; y: number }>(points: P[], tolerance: number): P[] {
  if (points.length <= 2) return points;
  const keep = new Array(points.length).fill(false);
  keep[0] = keep[points.length - 1] = true;
  const stack: [number, number][] = [[0, points.length - 1]];
  while (stack.length > 0) {
    const [first, last] = stack.pop() as [number, number];
    let worst = -1;
    let worstDistance = tolerance;
    for (let i = first + 1; i < last; i++) {
      const d = distanceToSegment(points[i], points[first], points[last]);
      if (d > worstDistance) {
        worst = i;
        worstDistance = d;
      }
    }
    if (worst >= 0) {
      keep[worst] = true;
      stack.push([first, worst], [worst, last]);
    }
  }
  return points.filter((_, i) => keep[i]);
}
