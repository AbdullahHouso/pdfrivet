// User settings and the recent-files list, saved with tauri-plugin-store
// (settings.json in the app's config folder).

import { load, type Store } from "@tauri-apps/plugin-store";
import type { Color } from "./bindings/Color";
import { type FileView, findRecent, type RecentFile, removeRecent, touchRecent, updateView } from "./recent";
import type { PageLayout } from "./tabs.svelte";

export type Theme = "system" | "light" | "dark" | "black";
/**
 * A colour laid over the pages while reading (the file isn't changed):
 * warm paper, green, dimmed, or dark pages with light text.
 */
export type PageTone = "original" | "warm" | "green" | "dimmed" | "dark";
/** The page colours, with a swatch showing what a white page looks like in each. */
export const PAGE_TONES: { value: PageTone; swatch: string }[] = [
  { value: "original", swatch: "#ffffff" },
  { value: "warm", swatch: "#f3e3bd" },
  { value: "green", swatch: "#cce8cf" },
  { value: "dimmed", swatch: "#b9b9b9" },
  { value: "dark", swatch: "#232428" },
];
/** Open documents as tabs in one window (default), or each in its own window. */
export type DocumentWindows = "tabs" | "windows";
/** The zoom new documents open at: fit the width, fit the page, or a percentage. */
export type DefaultZoom = "fit-width" | "fit-page" | `${number}`;
/** Mouse mode: click links and fields, or drag the page around. */
export type Tool = "select" | "hand";

/** How an annotation tool draws (Annotate toolbar); remembered per tool. */
export interface ToolStyle {
  color: Color;
  /** Line width in points. */
  width: number;
  /** 0–1. */
  opacity: number;
  /** Rectangles and ellipses: filled with a lighter shade of the colour. */
  fill: boolean;
}

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
// How documents open the first time (Settings → Appearance and Reading).
// Files opened before reopen the way you last viewed them.
let defaultZoom = $state<DefaultZoom>("fit-width");
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
let author = $state("");
// Animations start off when the system asks for reduced motion.
const lessMotion = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
let animations = $state(!lessMotion);
let toolStyles = $state<Record<string, ToolStyle>>({});

/** Turns animations on or off for the whole window (see app.css and motion.ts). */
function applyMotion(on: boolean) {
  if (typeof document !== "undefined") document.documentElement.dataset.motion = on ? "on" : "off";
}
applyMotion(!lessMotion);

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
    case "defaultZoom":
      defaultZoom = value as DefaultZoom;
      break;
    case "pageLayout":
      pageLayout = value as PageLayout;
      break;
    case "continuous":
      continuous = value as boolean;
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
    case "author":
      author = value as string;
      break;
    case "animations":
      animations = value as boolean;
      applyMotion(animations);
      break;
    case "toolStyles":
      toolStyles = value as Record<string, ToolStyle>;
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
      defaultZoom = (await store.get<DefaultZoom>("defaultZoom")) ?? "fit-width";
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
      author = (await store.get<string>("author")) ?? "";
      animations = (await store.get<boolean>("animations")) ?? animations;
      toolStyles = (await store.get<Record<string, ToolStyle>>("toolStyles")) ?? {};
    } catch (e) {
      console.warn("[settings] using defaults", e);
    }
    applyTheme(theme);
    applyMotion(animations);
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

  get defaultZoom() {
    return defaultZoom;
  },
  set defaultZoom(value: DefaultZoom) {
    defaultZoom = value;
    save("defaultZoom", value);
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

  /** Menus, dialogs and tooltips animate in and out. */
  get animations() {
    return animations;
  },
  set animations(value: boolean) {
    animations = value;
    applyMotion(value);
    save("animations", value);
  },

  /** The name written as the author of new annotations (empty: the OS user name). */
  get author() {
    return author;
  },
  set author(value: string) {
    author = value;
    save("author", value);
  },
  /** Styles chosen per annotation tool (missing tools use their defaults). */
  get toolStyles() {
    return toolStyles;
  },
  setToolStyle(tool: string, style: ToolStyle) {
    toolStyles = { ...toolStyles, [tool]: style };
    save("toolStyles", toolStyles);
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
  updateView(path: string, view: FileView) {
    recent = updateView(recent, path, view);
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
