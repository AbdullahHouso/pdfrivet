import { describe, expect, it, vi } from "vitest";
import type { DocInfo } from "./bindings/DocInfo";

const closeDocument = vi.fn(async () => {});
vi.mock("./pdf", () => ({ closeDocument: (...a: unknown[]) => closeDocument(...(a as [])) }));

const { tabs } = await import("./tabs.svelte");

const info: DocInfo = {
  pageCount: 10,
  title: "Report",
  author: null,
  pageSizes: Array(10).fill({ width: 600, height: 800 }),
  rtl: false,
  canCopy: true,
  canAnnotate: true,
  canEditOutline: true,
  canReply: true,
};

describe("moving tabs between windows", () => {
  it("hands over a tab's view without closing its document", () => {
    const tab = tabs.add(7, "/docs/report.pdf", info);
    tab.page = 4;
    tab.zoom = 1.5;
    tab.zoomMode = "custom";
    tab.rotation = 90;
    tab.pageTone = "green";
    tab.dirty = true;
    const state = tab.state();

    tabs.detach(tab.id);
    expect(tabs.list).toHaveLength(0);
    expect(closeDocument).not.toHaveBeenCalled();

    // Another window takes it over.
    const moved = tabs.adopt(state, info);
    expect(moved.docId).toBe(7);
    expect([moved.page, moved.zoom, moved.zoomMode, moved.rotation, moved.pageTone, moved.dirty]).toEqual([
      4,
      1.5,
      "custom",
      90,
      "green",
      true,
    ]);
    expect(tabs.active?.id).toBe(moved.id);
  });

  it("keeps the page within the document", () => {
    const moved = tabs.adopt({ ...tabs.list[0].state(), page: 99 }, info);
    expect(moved.page).toBe(9);
  });
});
