// Framework-free translation logic (easy to unit test).
// Languages are discovered from /locales: adding one needs no code changes.

import { FluentBundle, FluentResource, type FluentVariable } from "@fluent/bundle";

export interface Language {
  code: string;
  name: string;
  dir: "ltr" | "rtl";
}

export const SOURCE_LANGUAGE = "en";

/** Builds one Fluent bundle per language from `{ code: [ftl source, ...] }`. */
export function buildBundles(sources: Record<string, string[]>): Map<string, FluentBundle> {
  const bundles = new Map<string, FluentBundle>();
  for (const [code, files] of Object.entries(sources)) {
    const bundle = new FluentBundle(code);
    for (const text of files) {
      const errors = bundle.addResource(new FluentResource(text));
      for (const e of errors) console.warn(`[i18n] ${code}: ${e.message}`);
    }
    bundles.set(code, bundle);
  }
  return bundles;
}

/** Picks the best available language for the user's preferences, e.g. "ar-SA" → "ar". */
export function negotiate(preferred: readonly string[], available: readonly string[]): string {
  for (const wanted of preferred) {
    const lower = wanted.toLowerCase();
    const exact = available.find((code) => code.toLowerCase() === lower);
    if (exact) return exact;
    const base = lower.split("-")[0];
    const partial = available.find((code) => code.toLowerCase().split("-")[0] === base);
    if (partial) return partial;
  }
  return SOURCE_LANGUAGE;
}

/** Translates `id` in `locale`, falling back to English, then to the id itself. */
export function translate(
  bundles: Map<string, FluentBundle>,
  locale: string,
  id: string,
  args?: Record<string, FluentVariable>,
): string {
  for (const code of locale === SOURCE_LANGUAGE ? [locale] : [locale, SOURCE_LANGUAGE]) {
    const bundle = bundles.get(code);
    const message = bundle?.getMessage(id);
    if (bundle && message?.value) {
      return bundle.formatPattern(message.value, args);
    }
  }
  return id;
}
