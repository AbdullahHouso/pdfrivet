import { describe, expect, it } from "vitest";
import { shortcutKey } from "./keys";

describe("shortcutKey", () => {
  it("uses the typed character on Latin layouts", () => {
    expect(shortcutKey({ key: "s", code: "KeyS" })).toBe("s");
    expect(shortcutKey({ key: "S", code: "KeyS" })).toBe("s");
    // French AZERTY: the key in the US "Q" position types "a".
    expect(shortcutKey({ key: "a", code: "KeyQ" })).toBe("a");
    expect(shortcutKey({ key: "=", code: "Equal" })).toBe("=");
  });

  it("uses the key position on non-Latin layouts", () => {
    expect(shortcutKey({ key: "س", code: "KeyS" })).toBe("s");
    expect(shortcutKey({ key: "ص", code: "KeyW" })).toBe("w");
    expect(shortcutKey({ key: "٠", code: "Digit0" })).toBe("0");
    // Settings (Ctrl+,): the comma key types "و" on the Arabic layout.
    expect(shortcutKey({ key: "و", code: "Comma" })).toBe(",");
  });

  it("leaves named keys alone", () => {
    expect(shortcutKey({ key: "F11", code: "F11" })).toBe("f11");
    expect(shortcutKey({ key: "Tab", code: "Tab" })).toBe("tab");
  });
});
