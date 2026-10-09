import { describe, expect, it } from "vitest";
import { growsLeft } from "./annotate.svelte";
import { clampSize, cssFamily, defaultTextStyle, groupFonts, isRtl, startsRtl, stepSize } from "./textBox";

describe("text boxes", () => {
  it("finds the direction from the first letter", () => {
    expect(startsRtl("مرحبا PDF")).toBe(true);
    expect(startsRtl("PDF مرحبا")).toBe(false);
    expect(startsRtl("2026 - مرحبا")).toBe(true);
    expect(startsRtl("")).toBe(false);
    const style = defaultTextStyle();
    expect(isRtl("Hello\nمرحبا", style)).toBe(false);
    expect(isRtl("Hello", { ...style, direction: "rtl" })).toBe(true);
  });

  it("grows leftwards only for right-to-left text without a set width", () => {
    const style = defaultTextStyle();
    expect(growsLeft("مرحبا", style)).toBe(true);
    expect(growsLeft("Hello", style)).toBe(false);
    expect(growsLeft("مرحبا", { ...style, width: 100 })).toBe(false);
  });

  it("steps and limits font sizes", () => {
    expect(stepSize(14, 1)).toBe(16);
    expect(stepSize(14, -1)).toBe(12);
    expect(stepSize(72, 1)).toBe(84);
    expect(stepSize(8, -1)).toBe(7);
    expect(clampSize(1)).toBe(4);
    expect(clampSize(500)).toBe(200);
    expect(clampSize(12.3)).toBe(12.5);
  });

  it("groups fonts and names them for CSS with the same fallbacks as the engine", () => {
    const g = groupFonts([
      { family: "Rubik", bundled: true, arabic: true },
      { family: "Arial", bundled: false, arabic: true },
      { family: "Consolas", bundled: false, arabic: false },
    ]);
    expect(g.bundled.map((f) => f.family)).toEqual(["Rubik"]);
    expect(g.arabic.map((f) => f.family)).toEqual(["Arial"]);
    expect(g.other.map((f) => f.family)).toEqual(["Consolas"]);
    expect(cssFamily({ family: "Amiri", bundled: true })).toBe('"PDFRivet Amiri", "PDFRivet Rubik", "PDFRivet Amiri"');
    expect(cssFamily({ family: 'Odd"Name', bundled: false })).toBe('"OddName", "PDFRivet Rubik", "PDFRivet Amiri"');
  });
});
