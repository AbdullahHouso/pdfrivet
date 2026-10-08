// User settings and the recent-files list, saved with tauri-plugin-store
// (settings.json in the app's config folder).

import { load, type Store } from "@tauri-apps/plugin-store";
import { findRecent, type RecentFile, removeRecent, touchRecent, updatePosition } from "./recent";
import type { PageLayout } from "./tabs.svelte";

export type Theme = "system" | "light" | "dark" | "black";
/**
 * A colour laid over the pages while reading (the file isn't changed):
 * warm paper, green, dimmed, or dark pages with light text.
 */
export type PageTone = "original" | "warm" | "green" | "dimmed" | "dark";
/** Open documents as tabs in one window (default), or each in its own window. */
export type DocumentWindows = "tabs" | "windows";
/** Mouse mode: click links and fields, or drag the page around. */
export type Tool = "select" | "hand";

/** Print choices remembered between prints. */
export interface PrintPrefs {
  printer?: string;
  scaling?: "fit" | "actual" | "shrink" | "custom";
  orientation?: "auto" | "portrait" | "landscape";
  duplex?: "oneSided" | "longEdge" | "shortEdge";
  grayscale?: boolean;
}

let store: Store | null = null;
let theme = $state<Theme>("system");
let recent = $state<RecentFile[]>([]);
// The view new tabs start with (the last one you picked).
let pageLayout = $state<PageLayout>("single");
let continuous = $state(true);
let tool = $state<Tool>("select");
let printPrefs = $state<PrintPrefs>({});
let pageTone = $state<PageTone>("original");
let documentWindows = $state<DocumentWindows>("tabs");
let taskbarTabs = $state(true);
let autoUpdate = $state(true);
let lastUpdateCheck = 0;
let skippedVersion = $state<string | null>(null);

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

/** Takes over a setting changed in another window. */
function apply(key: string, value: unknown) {
  if (value === undefined || value === null) return;
  switch (key) {
    case "theme":
      theme = value as Theme;
      applyTheme(theme);
      break;
    case "recent":
      recent = value as RecentFile[];
      break;
    case "pageTone":
      pageTone = value as PageTone;
      break;
    case "documentWindows":
      documentWindows = value as DocumentWindows;
      break;
    case "taskbarTabs":
      taskbarTabs = value as boolean;
      break;
    case "tool":
      tool = value as Tool;
      break;
    case "print":
      printPrefs = value as PrintPrefs;
      break;
    case "autoUpdate":
      autoUpdate = value as boolean;
      break;
    case "skippedVersion":
      skippedVersion = value as string;
      break;
    case "lastUpdateCheck":
      lastUpdateCheck = value as number;
      break;
  }
}

export const settings = {
  /** Loads saved settings. Call once at startup; the app works without it. */
  async init() {
    try {
      store = await load("settings.json", { autoSave: 500, defaults: {} });
      theme = (await store.get<Theme>("theme")) ?? "system";
      recent = (await store.get<RecentFile[]>("recent")) ?? [];
      pageLayout = (await store.get<PageLayout>("pageLayout")) ?? "single";
      continuous = (await store.get<boolean>("continuous")) ?? true;
      tool = (await store.get<Tool>("tool")) ?? "select";
      printPrefs = (await store.get<PrintPrefs>("print")) ?? {};
      pageTone = (await store.get<PageTone>("pageTone")) ?? "original";
      documentWindows = (await store.get<DocumentWindows>("documentWindows")) ?? "tabs";
      taskbarTabs = (await store.get<boolean>("taskbarTabs")) ?? true;
      // Several windows share these settings: follow changes made in the others.
      await store.onChange((key, value) => apply(key, value));
      autoUpdate = (await store.get<boolean>("autoUpdate")) ?? true;
      lastUpdateCheck = (await store.get<number>("lastUpdateCheck")) ?? 0;
      skippedVersion = (await store.get<string>("skippedVersion")) ?? null;
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

  /** Tabs in one window, or a window per document. */
  get documentWindows() {
    return documentWindows;
  },
  set documentWindows(value: DocumentWindows) {
    documentWindows = value;
    save("documentWindows", value);
  },
  /** Windows only: each tab gets its own preview in the taskbar. */
  get taskbarTabs() {
    return taskbarTabs;
  },
  set taskbarTabs(value: boolean) {
    taskbarTabs = value;
    save("taskbarTabs", value);
  },

  get pageTone() {
    return pageTone;
  },
  set pageTone(value: PageTone) {
    pageTone = value;
    save("pageTone", value);
  },

  get pageLayout() {
    return pageLayout;
  },
  set pageLayout(value: PageLayout) {
    pageLayout = value;
    save("pageLayout", value);
  },
  get continuous() {
    return continuous;
  },
  set continuous(value: boolean) {
    continuous = value;
    save("continuous", value);
  },

  get tool() {
    return tool;
  },
  set tool(value: Tool) {
    tool = value;
    save("tool", value);
  },

  get printPrefs() {
    return printPrefs;
  },
  set printPrefs(value: PrintPrefs) {
    printPrefs = value;
    save("print", value);
  },

  /** Look for a new version once a day. */
  get autoUpdate() {
    return autoUpdate;
  },
  set autoUpdate(value: boolean) {
    autoUpdate = value;
    save("autoUpdate", value);
  },
  /** When updates were last looked for (ms since 1970). */
  get lastUpdateCheck() {
    return lastUpdateCheck;
  },
  set lastUpdateCheck(value: number) {
    lastUpdateCheck = value;
    save("lastUpdateCheck", value);
  },
  /** A version the user chose to skip; automatic checks don't offer it again. */
  get skippedVersion() {
    return skippedVersion;
  },
  set skippedVersion(value: string | null) {
    skippedVersion = value;
    save("skippedVersion", value);
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
  updatePosition(path: string, position: Pick<RecentFile, "page" | "zoom" | "zoomMode" | "pagesRtl">) {
    recent = updatePosition(recent, path, position);
    save("recent", recent);
  },
  clearRecent() {
    recent = [];
    save("recent", recent);
  },
  removeRecent(path: string) {
    recent = removeRecent(recent, path);
    save("recent", recent);
  },
};
