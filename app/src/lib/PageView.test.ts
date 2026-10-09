// @vitest-environment happy-dom
// Component test: a page that was rendered before appears sharp straight away,
// without the blurry preview; a new page shows the preview first.

import { cleanup, render, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RenderOptions } from "./pdf";

const state = { inCache: false };
const pixels = { width: 10, height: 10, data: {} as ImageData };
const renderPage = vi.fn(async (_doc: number, _page: number, opts?: RenderOptions) =>
  opts?.cachedOnly && !state.inCache ? null : pixels,
);
vi.mock("./pdf", () => ({
  renderPage: (doc: number, page: number, opts: RenderOptions) => renderPage(doc, page, opts),
  getLinks: async () => [],
  // A page without text: just the 8-byte header.
  fetchPageText: async () => new ArrayBuffer(8),
  toRivetError: (e: unknown) => e,
}));

const { default: PageView } = await import("./PageView.svelte");
const { forgetPages } = await import("./pageCanvasCache");

const show = () => render(PageView, { docId: 1, index: 3, width: 600, height: 800, widthPt: 600, rotation: 0 });
const scales = () =>
  renderPage.mock.calls.map(([, , opts]) => ({ scale: opts?.scale, cachedOnly: !!opts?.cachedOnly }));

describe("PageView", () => {
  beforeEach(() => {
    renderPage.mockClear();
    forgetPages(1);
  });
  afterEach(() => cleanup());

  it("shows a page rendered before without the blurry preview", async () => {
    state.inCache = true;
    show();
    await waitFor(() => expect(renderPage).toHaveBeenCalledTimes(1));
    expect(scales()).toEqual([{ scale: 1, cachedOnly: true }]);
  });

  it("shows a page that was just on screen again without asking the engine", async () => {
    state.inCache = true;
    const first = show();
    await waitFor(() => expect(renderPage).toHaveBeenCalledTimes(1));
    first.unmount();
    renderPage.mockClear();
    show();
    await new Promise((r) => setTimeout(r, 50));
    expect(renderPage).not.toHaveBeenCalled();
  });

  it("shows a quick preview while a new page renders", async () => {
    state.inCache = false;
    show();
    await waitFor(() => expect(renderPage).toHaveBeenCalledTimes(3));
    expect(scales()).toEqual([
      { scale: 1, cachedOnly: true },
      { scale: 0.25, cachedOnly: false },
      { scale: 1, cachedOnly: false },
    ]);
  });
});
