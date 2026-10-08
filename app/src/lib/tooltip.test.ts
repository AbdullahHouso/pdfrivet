import { describe, expect, it } from "vitest";
import { formatShortcut, tooltipContent } from "./tooltip";

describe("tooltips", () => {
  it("shows shortcuts the way each system writes them", () => {
    expect(formatShortcut("Ctrl+O", false)).toBe("Ctrl+O");
    expect(formatShortcut("Ctrl+Shift+S", true)).toBe("⌘⇧S");
    expect(formatShortcut("V", true)).toBe("V");
  });

  it("doesn't repeat a label that is already visible", () => {
    expect(tooltipContent("Open PDF…", "Open PDF…", "Ctrl+O")).toEqual({ text: null, shortcut: "Ctrl+O" });
    expect(tooltipContent("Open PDF…", "Open PDF…", null)).toBeNull();
    expect(tooltipContent("Zoom in", "", "Ctrl+=")).toEqual({ text: "Zoom in", shortcut: "Ctrl+=" });
    expect(tooltipContent("Rotate view", "", null)).toEqual({ text: "Rotate view", shortcut: null });
  });
});
