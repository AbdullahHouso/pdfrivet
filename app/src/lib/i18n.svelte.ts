// Reactive i18n for the UI. Usage in components: `i18n.t("open-file")`.
// Changing `i18n.locale` re-renders every translated string and flips
// the page direction (LTR/RTL) automatically.

import type { FluentVariable } from "@fluent/bundle";
import languageList from "../../../locales/languages.json";
import { buildBundles, type Language, negotiate, translate } from "./i18n-core";

// Vite bundles every locales/<code>/*.ftl file at build time.
const files = import.meta.glob<string>("../../../locales/*/*.ftl", {
  query: "?raw",
  import: "default",
  eager: true,
});

const sources: Record<string, string[]> = {};
for (const [path, text] of Object.entries(files)) {
  const code = path.split("/").at(-2) as string;
  sources[code] = [...(sources[code] ?? []), text];
}

export const languages: Language[] = (languageList as Language[]).filter((l) => sources[l.code]);
const bundles = buildBundles(sources);
const STORAGE_KEY = "rivet.locale";

function initialLocale(): string {
  const codes = languages.map((l) => l.code);
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && codes.includes(saved)) return saved;
  } catch {
    // Storage can be unavailable; fall back to the OS language.
  }
  // OS languages come from Rust (see system_locales_script in src-tauri/src/lib.rs).
  const system = (window as { __RIVET_SYSTEM_LOCALES__?: string[] }).__RIVET_SYSTEM_LOCALES__ ?? [];
  return negotiate([...system, ...navigator.languages], codes);
}

let current = $state(initialLocale());

function applyToDocument(code: string) {
  const lang = languages.find((l) => l.code === code);
  document.documentElement.lang = code;
  document.documentElement.dir = lang?.dir ?? "ltr";
}
applyToDocument(current);

export const i18n = {
  get locale() {
    return current;
  },
  set locale(code: string) {
    current = code;
    applyToDocument(code);
    try {
      localStorage.setItem(STORAGE_KEY, code);
    } catch {
      // Not critical: the choice just won't be remembered.
    }
  },
  t(id: string, args?: Record<string, FluentVariable>): string {
    return translate(bundles, current, id, args);
  },
};
