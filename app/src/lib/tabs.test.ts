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
  canAssemble: true,
  canPrint: true,
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

describe("side by side", () => {
  async function closeAll() {
    for (const t of [...tabs.list]) await tabs.close(t.id);
  }

  it("shows a tab beside the active one, and focusing a pane makes it active", async () => {
    await closeAll();
    const a = tabs.add(1, "/a.pdf", info);
    const b = tabs.add(2, "/b.pdf", info);
    tabs.activate(a.id);
    expect(tabs.besideCandidate?.id).toBe(b.id);
    tabs.showBeside(b.id);
    expect(tabs.panes.map((t) => t.id)).toEqual([a.id, b.id]);
    expect(tabs.split?.sync).toBe(false);
    tabs.focusPane(b.id);
    expect(tabs.active?.id).toBe(b.id);
    expect(tabs.panes.map((t) => t.id)).toEqual([a.id, b.id]);
  });

  it("shows another tab in the focused pane, keeping the other", async () => {
    await closeAll();
    const a = tabs.add(1, "/a.pdf", info);
    const b = tabs.add(2, "/b.pdf", info);
    const c = tabs.add(3, "/c.pdf", info);
    tabs.activate(a.id);
    tabs.showBeside(b.id);
    tabs.focusPane(b.id);
    tabs.activate(c.id);
    expect(tabs.panes.map((t) => t.id)).toEqual([a.id, c.id]);
    expect(tabs.active?.id).toBe(c.id);
    // A file opened meanwhile goes to the focused pane too.
    const d = tabs.add(4, "/d.pdf", info);
    expect(tabs.panes.map((t) => t.id)).toEqual([a.id, d.id]);
  });

  it("ends when either tab closes, showing the other", async () => {
    await closeAll();
    const a = tabs.add(1, "/a.pdf", info);
    const b = tabs.add(2, "/b.pdf", info);
    tabs.add(3, "/c.pdf", info);
    tabs.activate(a.id);
    tabs.showBeside(b.id);
    await tabs.close(a.id);
    expect(tabs.split).toBeNull();
    expect(tabs.active?.id).toBe(b.id);

    tabs.showBeside(tabs.list[1].id);
    await tabs.close(tabs.list[1].id);
    expect(tabs.split).toBeNull();
    expect(tabs.active?.id).toBe(b.id);
  });

  it("ends on the start page", async () => {
    await closeAll();
    const a = tabs.add(1, "/a.pdf", info);
    const b = tabs.add(2, "/b.pdf", info);
    tabs.showBeside(a.id);
    expect(tabs.panes).toHaveLength(2);
    tabs.showHome();
    expect(tabs.split).toBeNull();
    expect(tabs.panes).toHaveLength(0);
    tabs.closeHome();
    expect(tabs.panes.map((t) => t.id)).toEqual([b.id]);
  });
});
