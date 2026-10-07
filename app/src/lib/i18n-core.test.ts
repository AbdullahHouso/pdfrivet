import { describe, expect, it } from "vitest";
import { buildBundles, negotiate, translate } from "./i18n-core";

const bundles = buildBundles({
  en: ["hello = Hello\npages = { $count } pages\nonly-en = English only"],
  ar: ["hello = مرحبا\npages = { $count } صفحات"],
});

describe("negotiate", () => {
  it("matches exact and regional codes", () => {
    expect(negotiate(["ar"], ["en", "ar"])).toBe("ar");
    expect(negotiate(["ar-SA", "en-US"], ["en", "ar"])).toBe("ar");
  });
  it("falls back to English", () => {
    expect(negotiate(["fr-FR"], ["en", "ar"])).toBe("en");
  });
});

describe("translate", () => {
  it("translates and formats arguments", () => {
    expect(translate(bundles, "ar", "hello")).toBe("مرحبا");
    // Fluent wraps arguments in Unicode isolation marks for correct bidi display.
    expect(translate(bundles, "en", "pages", { count: 3 })).toBe("⁨3⁩ pages");
  });
  it("falls back to English, then to the message id", () => {
    expect(translate(bundles, "ar", "only-en")).toBe("English only");
    expect(translate(bundles, "ar", "missing-key")).toBe("missing-key");
  });
});
