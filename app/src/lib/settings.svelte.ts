// User settings and the recent-files list, saved with tauri-plugin-store
// (settings.json in the app's config folder).

import { load, type Store } from "@tauri-apps/plugin-store";
import { findRecent, type RecentFile, removeRecent, touchRecent, updatePosition } from "./recent";

export type Theme = "system" | "light" | "dark";

let store: Store | null = null;
let theme = $state<Theme>("system");
let recent = $state<RecentFile[]>([]);

function applyTheme(value: Theme) {
  if (value === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = value;
}

async function save(key: string, value: unknown) {
  try {
    await store?.set(key, value);
  } catch (e) {
    console.warn("[settings] could not save", key, e);
  }
}

export const settings = {
  /** Loads saved settings. Call once at startup; the app works without it. */
  async init() {
    try {
      store = await load("settings.json", { autoSave: 500, defaults: {} });
      theme = (await store.get<Theme>("theme")) ?? "system";
      recent = (await store.get<RecentFile[]>("recent")) ?? [];
    } catch (e) {
      console.warn("[settings] using defaults", e);
    }
    applyTheme(theme);
  },

  get theme() {
    return theme;
  },
  set theme(value: Theme) {
    theme = value;
    applyTheme(value);
    save("theme", value);
  },

  get recent() {
    return recent;
  },
  findRecent(path: string) {
    return findRecent(recent, path);
  },
  touchRecent(entry: RecentFile) {
    recent = touchRecent(recent, entry);
    save("recent", recent);
  },
  updatePosition(path: string, position: Pick<RecentFile, "page" | "zoom" | "zoomMode">) {
    recent = updatePosition(recent, path, position);
    save("recent", recent);
  },
  removeRecent(path: string) {
    recent = removeRecent(recent, path);
    save("recent", recent);
  },
};
