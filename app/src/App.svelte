<script lang="ts">
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { onMount, tick, untrack } from "svelte";
import AboutDialog from "./lib/AboutDialog.svelte";
import AnnotateBar from "./lib/AnnotateBar.svelte";
import {
  annotate,
  applyRedactions,
  markSelection,
  redo,
  remove as removeAnnotation,
  undo,
} from "./lib/annotate.svelte";
import type { Metadata } from "./lib/bindings/Metadata";
import type { PrintSettings } from "./lib/bindings/PrintSettings";
import CommentsPanel from "./lib/CommentsPanel.svelte";
import ConfirmDialog, { type Choice } from "./lib/ConfirmDialog.svelte";
import ContextMenu, { type MenuItem } from "./lib/ContextMenu.svelte";
import { writeClipboard } from "./lib/clipboard";
import DefaultAppCard from "./lib/DefaultAppCard.svelte";
import { defaultApp } from "./lib/defaultApp.svelte";
import { hasOtherWindows, moveToNewWindow, openInNewWindow, tabsWindowLabel, windowRequest } from "./lib/docWindows";
import FindBar from "./lib/FindBar.svelte";
import Icon from "./lib/Icon.svelte";
import { i18n } from "./lib/i18n.svelte";
import { isTyping, modalOpen, shortcutKey } from "./lib/keys";
import {
  COMMENTS_DEFAULT,
  COMMENTS_MIN,
  commentsMax,
  type Degrees,
  rotatedSize,
  SIDEBAR_DEFAULT,
  SIDEBAR_MIN,
  sidebarMax,
  stepZoom,
} from "./lib/layout";
import { fade, out, rise } from "./lib/motion";
import OrganizeView from "./lib/organize/OrganizeView.svelte";
import PasswordDialog from "./lib/PasswordDialog.svelte";
import PrintDialog from "./lib/PrintDialog.svelte";
import PropertiesDialog from "./lib/PropertiesDialog.svelte";
import { arrange } from "./lib/pageEdits";
import { pageList } from "./lib/pageRange";
import {
  captureTaskbarTab,
  closeDocument,
  documentInfo,
  extractPages,
  filesExist,
  getText,
  openDocument,
  type RivetError,
  saveDocument,
  setMetadata,
  setTaskbarTabs,
  takePendingFiles,
  toRivetError,
} from "./lib/pdf";
import { type PrintProgress, printDocument as printWithSystemDialog } from "./lib/print";
import { printDocument } from "./lib/printing";
import type { ZoomMode } from "./lib/recent";
import { revealWindow } from "./lib/reveal";
import SettingsDialog from "./lib/SettingsDialog.svelte";
import Sidebar, { type SidebarPane } from "./lib/Sidebar.svelte";
import Splitter from "./lib/Splitter.svelte";
import StartScreen from "./lib/StartScreen.svelte";
import { type DocumentWindows, settings } from "./lib/settings.svelte";
import TabBar from "./lib/TabBar.svelte";
import Toolbar from "./lib/Toolbar.svelte";
import { type Tab, type TabState, tabs } from "./lib/tabs.svelte";
import { isEmpty, toTextRange } from "./lib/textSelect";
import ExtractDialog from "./lib/tools/ExtractDialog.svelte";
import MergeDialog from "./lib/tools/MergeDialog.svelte";
import ProtectDialog from "./lib/tools/ProtectDialog.svelte";
import SplitDialog from "./lib/tools/SplitDialog.svelte";
import type { ToolId } from "./lib/tools/tools";
import { TOOLS } from "./lib/tools/tools";
import UpdateDialog from "./lib/UpdateDialog.svelte";
import { updater } from "./lib/updater.svelte";
import Viewer from "./lib/Viewer.svelte";

let error = $state<RivetError | null>(null);
let sidebarOpen = $state(true);
let sidebarPane = $state<SidebarPane>("thumbnails");
// The sidebar's width: up to a quarter of the window, and wide enough for
// search results' text while they are shown.
let windowWidth = $state(window.innerWidth);
let sidebarMin = $derived(sidebarPane === "search" ? Math.min(280, sidebarMax(windowWidth)) : SIDEBAR_MIN);
let sidebarDrag = $state<number | null>(null);
// The comments panel on the end side, resizable the same way.
let commentsOpen = $state(false);
let commentsDrag = $state<number | null>(null);
let commentsWidth = $derived(
  Math.round(Math.min(Math.max(commentsDrag ?? settings.commentsWidth, COMMENTS_MIN), commentsMax(windowWidth))),
);
let sidebarWidth = $derived(
  Math.round(Math.min(Math.max(sidebarDrag ?? settings.sidebarWidth, sidebarMin), sidebarMax(windowWidth))),
);
let findBar: FindBar | undefined = $state();
let showAbout = $state(false);
let showSettings = $state(false);
let dragging = $state(false);
/** A protected PDF waiting for its password. */
let passwordFor = $state<{ path: string; wrong: boolean } | null>(null);

/** An open question dialog, answered through `ask`. */
let question = $state<{ title: string; message: string; choices: Choice[]; answer: (id: string) => void } | null>(null);

/** Pages being prepared for printing, with a way to cancel. */
let printing = $state<(PrintProgress & { controller: AbortController }) | null>(null);

let viewer: Viewer | undefined = $state();
let toolbar: Toolbar | undefined = $state();
let active = $derived(tabs.active);

const fileName = (path: string) => path.split(/[\\/]/).at(-1) ?? path;

async function pickFiles() {
  const picked = await open({
    multiple: true,
    directory: false,
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  await openFiles(picked ?? []);
}

/**
 * Opens PDFs as tabs here, or, with "Separate windows", each in a window of
 * its own (the first one here if this window is still empty).
 */
async function openFiles(paths: string[]) {
  for (const path of paths) {
    if (settings.documentWindows === "windows" && tabs.list.length > 0 && !tabs.findByPath(path)) {
      await openInNewWindow(path);
    } else {
      await openPath(path);
    }
  }
}

async function openPath(path: string, password?: string) {
  const existing = tabs.findByPath(path);
  if (existing) {
    tabs.activate(existing.id);
    return;
  }
  try {
    const { docId, info } = await openDocument(path, password);
    passwordFor = null;
    const tab = tabs.add(docId, path, info);
    tab.pageLayout = settings.pageLayout;
    tab.continuous = settings.continuous;
    tab.pageTone = settings.pageTone;
    // Reopen where you left off, the way you last viewed it.
    const saved = settings.findRecent(path);
    if (saved) {
      tab.page = Math.min(saved.page, info.pageCount - 1);
      tab.zoomMode = saved.zoomMode;
      tab.zoom = saved.zoom;
      if (saved.pagesRtl !== undefined) tab.pagesRtl = saved.pagesRtl;
      if (saved.pageLayout) tab.pageLayout = saved.pageLayout;
      if (saved.continuous !== undefined) tab.continuous = saved.continuous;
      if (saved.pageTone) tab.pageTone = saved.pageTone;
    } else {
      const zoom = settings.defaultZoom;
      if (zoom === "fit-width" || zoom === "fit-page") {
        tab.zoomMode = zoom;
      } else {
        tab.zoomMode = "custom";
        tab.zoom = Number(zoom) / 100;
      }
    }
    settings.touchRecent({ path, title: tab.title, lastOpened: Date.now(), ...tab.view() });
    defaultApp.offer();
  } catch (e) {
    const err = toRivetError(e);
    if (err.code === "password-required" || err.code === "wrong-password") {
      passwordFor = { path, wrong: err.code === "wrong-password" };
    } else if (err.code === "file-not-found" && settings.findRecent(path)) {
      await recentFileMissing(path);
    } else {
      error = err;
    }
  }
}

let showProperties = $state(false);

/** The right-click menu on the pages, if open. */
let contextMenu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

async function copySelection(tab: Tab) {
  if (!tab.selection || isEmpty(tab.selection)) return;
  try {
    await writeClipboard(await getText(tab.docId, toTextRange(tab.selection)));
  } catch (e) {
    error = toRivetError(e);
  }
}

/** Selects the whole document's text. */
function selectAll(tab: Tab) {
  const last = tab.info.pageCount - 1;
  tab.selection = { anchor: { page: 0, index: 0 }, focus: { page: last, index: Number.MAX_SAFE_INTEGER } };
}

/** The selected text, if it's short enough to search for (one line, up to 100 characters). */
async function searchableSelection(tab: Tab): Promise<string | null> {
  const sel = tab.selection;
  if (!sel || isEmpty(sel) || sel.anchor.page !== sel.focus.page || !tab.info.canCopy) return null;
  const text = await getText(tab.docId, toTextRange(sel)).catch(() => "");
  const line = text.trim();
  return line && line.length <= 100 && !line.includes("\n") ? line : null;
}

/** Opens the find bar (Ctrl+F), filled with the selected text if there is a short one. */
async function openFind(tab: Tab, text?: string | null) {
  const query = text ?? (await searchableSelection(tab));
  tab.findOpen = true;
  if (query) {
    tab.search.query = query;
    tab.search.run(tab.page);
  }
  findBar?.focus();
}

function closeFind(tab: Tab) {
  tab.findOpen = false;
  tab.search.clear();
  // Back to the pages, so the keyboard scrolls them again.
  document.querySelector<HTMLElement>(".scroller")?.focus({ preventScroll: true });
}

/** F3 / Shift+F3: the next or previous result, or open the find bar. */
function findNext(tab: Tab, direction: 1 | -1) {
  if (tab.search.hits.length > 0) tab.search.step(direction);
  else openFind(tab);
}

function annotateSelection(tab: Tab, tool: "highlight" | "underline" | "strikeout") {
  markSelection(tab, tool).catch((e) => (error = toRivetError(e)));
}

/** Undo (-1) or redo (+1) an annotation change, showing the page it happened on. */
async function undoRedo(tab: Tab, direction: 1 | -1) {
  try {
    const page = await (direction < 0 ? undo(tab) : redo(tab));
    if (page !== null && (page < tab.page - 1 || page > tab.page + 1)) goTo(page);
  } catch (e) {
    error = toRivetError(e);
  }
}

function deleteSelectedAnnotation(tab: Tab) {
  const sel = tab.selectedAnnotation;
  const a = sel && tab.annotations.get(sel.page)?.find((x) => x.id === sel.id);
  if (sel && a) removeAnnotation(tab, sel.page, a).catch((e) => (error = toRivetError(e)));
}

let sidebar = $state<Sidebar>();

/** Bookmarks a page (the current one by default): opens the sidebar's bookmarks, ready to rename it. */
async function bookmarkPage(tab: Tab, page = tab.page) {
  if (!tab.info.canEditOutline) return;
  sidebarOpen = true;
  await tick();
  await sidebar?.addBookmark(page);
}

function showPageMenu(tab: Tab, e: MouseEvent) {
  const items: MenuItem[] = [];
  if (!isEmpty(tab.selection)) {
    items.push({
      label: i18n.t("copy-text"),
      shortcut: "Ctrl+C",
      disabled: !tab.info.canCopy,
      hint: i18n.t("error-copy-not-allowed"),
      action: () => copySelection(tab),
    });
    if (tab.info.canCopy) {
      items.push({ label: i18n.t("search-selection"), shortcut: "Ctrl+F", action: () => openFind(tab) });
    }
    if (tab.info.canAnnotate) {
      for (const tool of ["highlight", "underline", "strikeout"] as const) {
        items.push({ label: i18n.t(`annot-${tool}`), action: () => annotateSelection(tab, tool) });
      }
    }
  }
  items.push({ label: i18n.t("select-all"), shortcut: "Ctrl+A", action: () => selectAll(tab) });
  items.push({
    label: i18n.t("bookmark-add"),
    shortcut: "Ctrl+B",
    disabled: !tab.info.canEditOutline,
    hint: i18n.t("bookmarks-protected"),
    action: () => bookmarkPage(tab),
  });
  contextMenu = { x: e.clientX, y: e.clientY, items };
}

async function applyMetadata(tab: Tab, metadata: Metadata) {
  showProperties = false;
  try {
    await setMetadata(tab.docId, metadata);
    tab.info = { ...tab.info, title: metadata.title.trim() || null };
    tab.dirty = true;
  } catch (e) {
    error = toRivetError(e);
  }
}

/** Shows the "which pages?" dialog; printing starts from there. */
let printChoice = $state(false);

function printActive() {
  if (active && !active.info.canPrint) {
    showFinished(i18n.t("print-not-allowed"));
    return;
  }
  if (active && !printing) printChoice = true;
}

/** True while a native print job is being handed to the printer. */
let sendingToPrinter = $state(false);

async function printNative(settingsForJob: PrintSettings, printerSettings: number[] | null) {
  const tab = active;
  printChoice = false;
  if (!tab) return;
  sendingToPrinter = true;
  try {
    await printDocument(tab.docId, settingsForJob, printerSettings);
  } catch (e) {
    error = toRivetError(e);
  } finally {
    sendingToPrinter = false;
  }
}

async function printPages(pages: number[]) {
  const tab = active;
  printChoice = false;
  if (!tab || printing) return;
  const controller = new AbortController();
  printing = { done: 0, total: pages.length, controller };
  try {
    await printWithSystemDialog(
      tab.docId,
      tab.info.pageSizes,
      pages,
      (p) => {
        if (printing) printing = { ...printing, ...p };
      },
      controller.signal,
    );
  } catch (e) {
    if (!(e instanceof DOMException && e.name === "AbortError")) error = toRivetError(e);
  } finally {
    printing = null;
  }
}

/** Shows a question dialog and resolves with the chosen answer's id. */
function ask(title: string, message: string, choices: Choice[]): Promise<string> {
  return new Promise((resolve) => {
    question = {
      title,
      message,
      choices,
      answer: (id) => {
        question = null;
        resolve(id);
      },
    };
  });
}

/** Saves a tab (asking for a file name for "Save as"). Returns false if not saved. */
/** Asks before applying marked redactions (they can't be undone), then applies them. */
async function confirmRedactions(tab: Tab, action: string): Promise<boolean> {
  const answer = await ask(i18n.t("redact-confirm-title"), i18n.t("redact-confirm", { count: tab.redactions.length }), [
    { id: "apply", label: action, primary: true },
    { id: "cancel", label: i18n.t("cancel") },
  ]);
  if (answer !== "apply") return false;
  await applyRedactions(tab);
  return true;
}

async function saveTab(tab: Tab, saveAs = false): Promise<boolean> {
  let path: string | null = tab.path;
  if (saveAs) {
    path = await save({
      defaultPath: tab.path,
      filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
    });
    if (!path) return false;
    if (!path.toLowerCase().endsWith(".pdf")) path += ".pdf";
  }
  try {
    // Marked redactions are applied now: their content is removed for good.
    if (tab.redactions.length > 0 && !(await confirmRedactions(tab, i18n.t("redact-confirm-apply")))) return false;
    await saveDocument(tab.docId, path);
    tab.path = path;
    tab.dirty = false;
    settings.touchRecent({ path, title: tab.title, lastOpened: Date.now(), ...tab.view() });
    return true;
  } catch (e) {
    error = toRivetError(e);
    return false;
  }
}

const saveChoices = (): Choice[] => [
  { id: "save", label: i18n.t("save"), primary: true },
  { id: "discard", label: i18n.t("dont-save") },
  { id: "cancel", label: i18n.t("cancel") },
];

/** Closes a tab, first offering to save unsaved changes. */
async function closeTab(tab: Tab) {
  if (tab.dirty) {
    tabs.activate(tab.id);
    const answer = await ask(i18n.t("unsaved-title"), i18n.t("unsaved-message", { name: tab.title }), saveChoices());
    if (answer === "cancel") return;
    if (answer === "save" && !(await saveTab(tab))) return;
  }
  await tabs.close(tab.id);
  // With a window per document, closing its document closes the window,
  // unless it's the last one (which goes back to the start screen).
  if (settings.documentWindows === "windows" && tabs.list.length === 0 && (await hasOtherWindows())) {
    await getCurrentWindow().destroy();
  }
}

/**
 * A recent file that no longer exists: says so, and offers to remove it from
 * the list.
 */
async function recentFileMissing(path: string) {
  const name = settings.findRecent(path)?.title || fileName(path);
  const answer = await ask(i18n.t("file-not-found-title"), i18n.t("file-not-found-message", { name }), [
    { id: "remove", label: i18n.t("remove-from-recent-short"), primary: true },
    { id: "cancel", label: i18n.t("cancel") },
  ]);
  if (answer === "remove") settings.removeRecent(path);
}

/** Opens a file from the recent files (menu or start screen). */
async function openRecent(path: string) {
  const [exists] = await filesExist([path]).catch(() => [true]);
  if (exists) await openFiles([path]);
  else await recentFileMissing(path);
}

/** Shows a tab moved here from another window (its document is already open). */
async function adoptTab(state: TabState) {
  try {
    tabs.adopt(state, await documentInfo(state.docId));
  } catch (e) {
    error = toRivetError(e);
  }
}

/**
 * Applies a change of the "Open documents in" setting right away, in every
 * window (each one gets the change through the shared settings).
 * - Separate windows: this window keeps its current tab; every other tab moves
 *   to a window of its own.
 * - Tabs: every window hands its tabs to one window and closes.
 * Documents stay open in the engine meanwhile, so unsaved changes are kept.
 */
async function switchDocumentWindows(mode: DocumentWindows) {
  if (mode === "windows") {
    const others = tabs.list.filter((t) => t.id !== tabs.active?.id);
    let n = 1;
    for (const tab of others) {
      await moveToNewWindow(tab.state(), n++);
      tabs.detach(tab.id);
    }
    return;
  }
  const target = await tabsWindowLabel();
  if (target === getCurrentWindow().label) return;
  const states = tabs.list.map((t) => t.state());
  if (states.length > 0) await emitTo(target, "adopt-tabs", states);
  await getCurrentWindow().destroy();
}

// React to the setting changing after startup (here or in another window).
let modeReady = false;
let currentMode: DocumentWindows = "tabs";
$effect(() => {
  const mode = settings.documentWindows;
  if (!modeReady || mode === currentMode) return;
  currentMode = mode;
  untrack(() => switchDocumentWindows(mode));
});

/**
 * Before the app closes or restarts for an update: offers to save every
 * document with changes. Returns false if the user cancelled.
 */
async function offerToSaveAll(): Promise<boolean> {
  const unsaved = tabs.list.filter((t) => t.dirty);
  if (unsaved.length === 0) return true;
  const answer = await ask(
    i18n.t("unsaved-title"),
    i18n.t("unsaved-message-many", { count: unsaved.length }),
    saveChoices(),
  );
  if (answer === "cancel") return false;
  if (answer === "save") {
    for (const tab of unsaved) if (!(await saveTab(tab))) return false;
  }
  return true;
}

async function openPending() {
  await openFiles(await takePendingFiles());
}

/** A tool finished: says so, with Open and Show in folder for the file it wrote. */
let finished = $state<{ text: string; path?: string } | null>(null);
let finishedTimer: ReturnType<typeof setTimeout> | undefined;
function showFinished(text: string, path?: string) {
  finished = { text, path };
  clearTimeout(finishedTimer);
  finishedTimer = setTimeout(() => (finished = null), 10_000);
}

/** Opens Organize pages for a tab (when its pages can be changed). */
function organize(tab: Tab) {
  if (!tab.info.canAssemble) {
    error = { code: "assemble-not-allowed", detail: "" };
    return;
  }
  if (tab.redactions.length) {
    showFinished(i18n.t("organize-redactions-first"));
    return;
  }
  closeFind(tab);
  annotate.open = false;
  tab.selection = null;
  tab.organizing = true;
}

/** The tab whose Extract pages dialog is open. */
let extracting = $state<Tab | null>(null);

/** The Merge PDFs dialog is open. */
let merging = $state(false);
/** The tab whose Split dialog is open. */
let splitting = $state<Tab | null>(null);
/** The tab whose Password protection dialog is open. */
let protecting = $state<Tab | null>(null);

/** A tool chosen from the Tools menu or the start page. */
function runTool(tool: ToolId) {
  const tab = active;
  if (tool === "organize" && tab) organize(tab);
  else if (tool === "extract" && tab) extracting = tab;
  else if (tool === "split" && tab) splitting = tab;
  else if (tool === "protect" && tab) protecting = tab;
  else if (tool === "merge") merging = true;
}

/** A tool card on the start page: tools for a document ask for one first. */
async function startTool(tool: ToolId) {
  const info = TOOLS.find((t) => t.id === tool);
  if (!info?.needsDocument) return runTool(tool);
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (typeof path !== "string") return;
  await openPath(path);
  if (active && active.path === path) runTool(tool);
}

/**
 * Writes some pages (0-based) of a document into a new file the user names;
 * with `deleteAfter`, they're then deleted from the document (one undo step).
 */
async function extractFlow(tab: Tab, pages: number[], deleteAfter = false) {
  const name = tab.fileName.replace(/\.pdf$/i, "");
  const folder = tab.path.slice(0, tab.path.length - tab.fileName.length);
  const target = await save({
    defaultPath: folder + i18n.t("extract-name", { name, pages: pageList(pages) }) + ".pdf",
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (!target) return;
  const path = /\.pdf$/i.test(target) ? target : `${target}.pdf`;
  try {
    await extractPages(tab.docId, pages, path);
    if (deleteAfter) {
      const gone = new Set(pages);
      const kept = Array.from({ length: tab.info.pageCount }, (_, index) => index).filter((i) => !gone.has(i));
      await arrange(
        tab,
        kept.map((index) => ({ source: { kind: "page", index }, turns: 0 })),
      );
    }
    showFinished(i18n.t("extract-done", { file: fileName(path) }), path);
  } catch (e) {
    error = toRivetError(e);
  }
}

function goTo(page: number) {
  viewer?.goToPage(page);
}

function setZoomMode(mode: ZoomMode) {
  if (active) active.zoomMode = mode;
}

function zoomStep(direction: 1 | -1) {
  if (active) viewer?.zoomTo(stepZoom(active.zoom, direction));
}

function rotate() {
  if (active) active.rotation = ((active.rotation + 90) % 360) as Degrees;
}

async function toggleFullscreen() {
  const win = getCurrentWindow();
  await win.setFullscreen(!(await win.isFullscreen()));
}

// Remember where you are in each document and how you view it (saved shortly
// after you stop moving).
$effect(() => {
  if (!active) return;
  const path = active.path;
  const view = active.view();
  const timer = setTimeout(() => settings.updateView(path, view), 800);
  return () => clearTimeout(timer);
});

// Window title: "Document – PDFRivet".
$effect(() => {
  const title = active ? `${active.title} – ${i18n.t("app-name")}` : i18n.t("app-name");
  document.title = title;
  getCurrentWindow()
    .setTitle(title)
    .catch(() => {});
});

/** Keyboard shortcuts. Each returns true when it handled the key. */
// Windows: each tab gets its own preview in the taskbar (in Tabs mode; with
// separate windows each window is a taskbar entry of its own).
const onWindows = /Windows/.test(navigator.userAgent);
$effect(() => {
  if (!onWindows) return;
  const enabled = settings.taskbarTabs && settings.documentWindows === "tabs";
  const list = tabs.list.map((t) => {
    const size = rotatedSize(t.info.pageSizes[t.page] ?? t.info.pageSizes[0], t.rotation);
    return {
      id: t.id,
      title: t.title,
      docId: t.docId,
      page: t.page,
      widthPt: size.width,
      heightPt: size.height,
      rotation: t.rotation,
    };
  });
  const activeId = tabs.active?.id ?? null;
  // Wait until scrolling settles, so the preview isn't refreshed for every page.
  const timer = setTimeout(() => setTaskbarTabs(list, activeId, enabled).catch(() => {}), 300);
  // Then capture the window for this tab's preview, once its pages have rendered.
  const capture = enabled ? setTimeout(() => captureTaskbarTab().catch(() => {}), 1200) : undefined;
  return () => {
    clearTimeout(timer);
    clearTimeout(capture);
  };
});

function onKey(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey;
  const key = shortcutKey(e);
  const typing = isTyping(e.target);
  const tab = active;

  const shortcuts: [boolean, () => void][] = [
    // Our own printing (the default would print the app window).
    [mod && key === "p", () => tab && printActive()],
    [mod && key === "o", pickFiles],
    [mod && key === ",", () => (showSettings = true)],
    [e.key === "F11", toggleFullscreen],
    [!!tab && mod && key === "w", () => tab && closeTab(tab)],
    [tabs.home && mod && key === "w", () => tabs.closeHome()],
    [!!tab && mod && key === "d", () => (showProperties = true)],
    [!!tab && mod && e.shiftKey && key === "s", () => tab && saveTab(tab, true)],
    [!!tab && mod && !e.shiftKey && key === "s", () => tab?.dirty && saveTab(tab)],
    [tabs.list.length > 0 && mod && e.key === "Tab", () => tabs.cycle(e.shiftKey ? -1 : 1)],
    [tabs.list.length > 0 && mod && !e.shiftKey && key === "t", () => tabs.showHome()],
    [!!tab && mod && (key === "=" || key === "+"), () => zoomStep(1)],
    [!!tab && mod && key === "-", () => zoomStep(-1)],
    [!!tab && !typing && mod && !e.shiftKey && key === "b", () => tab && bookmarkPage(tab)],
    [!!tab && mod && e.shiftKey && key === "c", () => (commentsOpen = !commentsOpen)],
    [!!tab && mod && key === "0", () => setZoomMode("fit-width")],
    [!!tab && ((mod && key === "g") || e.key === "F6"), () => toolbar?.focusPageInput()],
    [!!tab && !typing && !mod && e.key === "Home", () => goTo(0)],
    // Find in document.
    [!!tab && mod && !e.shiftKey && key === "f", () => tab && openFind(tab)],
    [!!tab && e.key === "F3", () => tab && findNext(tab, e.shiftKey ? -1 : 1)],
    // Annotating: A shows the tools; undo and redo; Delete removes the selected annotation.
    [
      !!tab && !typing && !mod && !e.altKey && key === "a",
      () => {
        if (tab?.info.canAnnotate) annotate.open = !annotate.open;
      },
    ],
    // T: write a text box.
    [
      !!tab && !typing && !mod && !e.altKey && key === "t",
      () => {
        if (!tab?.info.canAnnotate) return;
        settings.tool = "select";
        annotate.tool = "text";
      },
    ],
    [!!tab && !typing && mod && !e.shiftKey && key === "z", () => tab && undoRedo(tab, -1)],
    [!!tab && !typing && mod && (key === "y" || (e.shiftKey && key === "z")), () => tab && undoRedo(tab, 1)],
    [
      !!tab && !typing && (e.key === "Delete" || e.key === "Backspace") && !!tab.selectedAnnotation,
      () => tab && deleteSelectedAnnotation(tab),
    ],
    [
      !!tab && !typing && e.key === "Escape" && (!!tab.selectedAnnotation || (annotate.open && !!annotate.tool)),
      () => {
        if (tab?.selectedAnnotation) tab.selectedAnnotation = null;
        else annotate.tool = null;
      },
    ],
    // Selected text: copy, select everything, clear.
    [!!tab && !typing && mod && !e.shiftKey && key === "c" && !isEmpty(tab.selection), () => tab && copySelection(tab)],
    [!!tab && !typing && mod && !e.shiftKey && key === "a", () => tab && selectAll(tab)],
    [
      !!tab && !typing && e.key === "Escape" && !!tab.selection,
      () => {
        if (tab) tab.selection = null;
      },
    ],
    // Mouse mode, with Figma's keys: V = select, H = hand.
    [!!tab && !typing && !mod && !e.altKey && key === "v", () => (settings.tool = "select")],
    [!!tab && !typing && !mod && !e.altKey && key === "h", () => (settings.tool = "hand")],
    [!!tab && !typing && !mod && e.key === "End", () => tab && goTo(tab.info.pageCount - 1)],
  ];
  // Organize pages handles its own keys; the document's shortcuts wait.
  if (tab?.organizing) return;
  const match = shortcuts.find(([when]) => when);
  if (!match) return;
  // With a dialog open, shortcuts would act on the window behind it (Ctrl+A
  // selected the page's text, Ctrl+O opened a file). The dialog keeps its own
  // keys (Escape, typing); Ctrl combinations stay blocked so the browser
  // doesn't print or search the window instead.
  if (modalOpen()) {
    if (mod) e.preventDefault();
    return;
  }
  e.preventDefault();
  match[1]();
}

onMount(() => {
  // Load settings first, so reopened files find their saved position.
  settings.init().then(() => {
    // A window opened for one document (see docWindows.ts) opens just that.
    currentMode = settings.documentWindows;
    modeReady = true;
    const request = windowRequest();
    let opening: Promise<unknown>;
    if (request.tab) {
      opening = adoptTab(request.tab);
    } else if (request.file) {
      opening = openPath(request.file);
    } else {
      opening = openPending();
      updater.startAutomaticChecks();
    }
    // The window is shown with its document already in it (see reveal.ts).
    opening.finally(() => tick().then(revealWindow));
  });
  const unlisten = [
    // Ask before closing the window with unsaved changes, then close its
    // documents (other windows may stay open).
    getCurrentWindow().onCloseRequested(async (event) => {
      event.preventDefault();
      if (!(await offerToSaveAll())) return;
      for (const tab of tabs.list) await closeDocument(tab.docId).catch(() => {});
      await getCurrentWindow().destroy();
    }),
    // Tabs handed over by other windows (switching to "Tabs").
    getCurrentWebviewWindow().listen<TabState[]>("adopt-tabs", async (event) => {
      for (const state of event.payload) await adoptTab(state);
      await getCurrentWindow().setFocus();
    }),
    // Files from "Open with" or a second launch while PDFRivet is running.
    // A tab's preview in the Windows taskbar was clicked or closed.
    getCurrentWebviewWindow().listen<{ action: "activate" | "close"; id: number }>("taskbar-tab", (event) => {
      const tab = tabs.list.find((t) => t.id === event.payload.id);
      if (!tab) return;
      if (event.payload.action === "activate") tabs.activate(tab.id);
      else closeTab(tab);
    }),
    // (Sent to one window only, so files don't open twice.)
    getCurrentWebviewWindow().listen("open-files", () => openPending()),
    getCurrentWebview().onDragDropEvent(async (event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") dragging = true;
      else if (p.type === "leave") dragging = false;
      else if (p.type === "drop") {
        dragging = false;
        await openFiles(p.paths.filter((x) => x.toLowerCase().endsWith(".pdf")));
      }
    }),
  ];
  return () => {
    for (const u of unlisten) u.then((stop) => stop());
  };
});
</script>

<svelte:window onkeydown={onKey} bind:innerWidth={windowWidth} />

<!-- With a window per document the tab bar is hidden, unless this window
     still has several tabs from before the setting changed. -->
{#if tabs.list.length > 1 || (tabs.list.length > 0 && settings.documentWindows === "tabs")}
  <TabBar onclose={closeTab} />
{/if}

<Toolbar
  bind:this={toolbar}
  tab={active}
  {sidebarOpen}
  onopen={pickFiles}
  ontogglesidebar={() => (sidebarOpen = !sidebarOpen)}
  {commentsOpen}
  ontogglecomments={() => (commentsOpen = !commentsOpen)}
  ongoto={goTo}
  onzoomstep={zoomStep}
  onzoom={(z) => viewer?.zoomTo(z)}
  onzoommode={setZoomMode}
  onrotate={rotate}
  onabout={() => (showAbout = true)}
  onsettings={() => (showSettings = true)}
  onopenrecent={openRecent}
  onclosedocument={() => active && closeTab(active)}
  oncheckupdates={() => updater.check(true)}
  onsave={() => active && saveTab(active)}
  onsaveas={() => active && saveTab(active, true)}
  onstep={(d) => viewer?.step(d)}
  onprint={printActive}
  onproperties={() => (showProperties = true)}
  onactualsize={() => viewer?.zoomTo(1)}
  onpagelayout={(layout) => {
    if (active) active.pageLayout = layout;
  }}
  onpagetone={(tone) => {
    if (active) active.pageTone = tone;
  }}
  onpagesrtl={(on) => {
    if (active) active.pagesRtl = on;
  }}
  oncontinuous={(on) => {
    if (active) active.continuous = on;
  }}
  ontool={runTool}
/>

{#if active && annotate.open}
  {@const tab = active}
  <AnnotateBar
    {tab}
    onerror={(e) => (error = e)}
    onapplyredactions={() =>
      confirmRedactions(tab, i18n.t("redact-confirm-apply-now")).catch((e) => (error = toRivetError(e)))}
  />
{/if}

{#if error}
  <div class="error" role="alert" in:rise out:out>
    <span>{i18n.t(`error-${error.code}`)}</span>
    <button onclick={() => (error = null)}>{i18n.t("dismiss")}</button>
  </div>
{/if}

{#if finished}
  <div class="print-progress finished" role="status" aria-live="polite" in:rise out:out>
    <span>{finished.text}</span>
    {#if finished.path}
      {@const path = finished.path}
      <button onclick={() => openFiles([path])}>{i18n.t("open-file-short")}</button>
      <button onclick={() => revealItemInDir(path).catch(() => {})}>{i18n.t("show-in-folder")}</button>
    {/if}
    <button class="icon" onclick={() => (finished = null)} aria-label={i18n.t("dismiss")} title={i18n.t("dismiss")}>
      <Icon name="close" />
    </button>
  </div>
{/if}

<main>
  {#if active}
    {#key active.id}
      {#if active.organizing}
        {@const tab = active}
        <OrganizeView
          {tab}
          onclose={() => (tab.organizing = false)}
          onerror={(e) => (error = e)}
          onask={ask}
          onextract={(pages) => extractFlow(tab, pages)}
        />
      {:else}
      {#if sidebarOpen}
        <div class="sidebar-wrap" in:fade out:out>
          <Sidebar
            bind:this={sidebar}
            tab={active}
            width={sidebarWidth}
            ongoto={goTo}
            bind:pane={sidebarPane}
            onfind={() => active && openFind(active)}
            onerror={(e) => (error = e)}
            ontool={(tool, page) => {
              if (!active) return;
              if (tool === "extract") extractFlow(active, [page]);
              else if (tool === "organize") organize(active);
            }}
          />
          <Splitter
            bind:value={() => sidebarWidth, (w) => (sidebarDrag = w)}
            min={sidebarMin}
            max={sidebarMax(windowWidth)}
            defaultValue={SIDEBAR_DEFAULT}
            edge="end"
            label={i18n.t("resize-sidebar")}
            oncommit={(w) => {
              sidebarDrag = null;
              settings.sidebarWidth = w;
            }}
          />
        </div>
      {/if}
      {@const tab = active}
      <Viewer bind:this={viewer} {tab} onerror={(e) => (error = e)} oncontextmenu={(e) => showPageMenu(tab, e)} />
      {#if commentsOpen}
        <div class="comments-wrap" in:fade out:out>
          <Splitter
            bind:value={() => commentsWidth, (w) => (commentsDrag = w)}
            min={COMMENTS_MIN}
            max={commentsMax(windowWidth)}
            defaultValue={COMMENTS_DEFAULT}
            edge="start"
            label={i18n.t("resize-comments")}
            oncommit={(w) => {
              commentsDrag = null;
              settings.commentsWidth = w;
            }}
          />
          <CommentsPanel
            {tab}
            width={commentsWidth}
            onreveal={(page, rect) => viewer?.revealRect(page, rect)}
            onclose={() => (commentsOpen = false)}
            onerror={(e) => (error = e)}
          />
        </div>
      {/if}
      {#if tab.findOpen}
        <FindBar
          bind:this={findBar}
          {tab}
          onclose={() => closeFind(tab)}
          onshowall={() => {
            sidebarOpen = true;
            sidebarPane = "search";
          }}
        />
      {/if}
      {/if}
    {/key}
  {:else}
    <StartScreen onopen={pickFiles} onopenpath={openRecent} ontool={startTool} />
  {/if}

  {#if dragging}
    <div class="drop-overlay" aria-hidden="true" in:fade out:out>
      <span>{i18n.t("drop-to-open")}</span>
    </div>
  {/if}
</main>

{#if merging}
  <MergeDialog
    current={active ? { docId: active.docId, path: active.path, name: active.fileName, pageCount: active.info.pageCount } : undefined}
    oncancel={() => (merging = false)}
    onerror={(e) => (error = e)}
    onfinished={(path, openIt) => {
      merging = false;
      showFinished(i18n.t("merge-done", { file: fileName(path) }), path);
      if (openIt) openFiles([path]);
    }}
  />
{/if}

{#if protecting}
  {@const tab = protecting}
  <ProtectDialog
    {tab}
    oncancel={() => (protecting = null)}
    onerror={(e) => (error = e)}
    ondone={async () => {
      protecting = null;
      // Protection is written by saving; the file then has its new permissions.
      if (await saveTab(tab)) {
        tab.info = await documentInfo(tab.docId);
        showFinished(i18n.t("protect-done"));
      }
    }}
  />
{/if}

{#if splitting}
  <SplitDialog
    tab={splitting}
    oncancel={() => (splitting = null)}
    onerror={(e) => (error = e)}
    onfinished={(count, first) => {
      splitting = null;
      showFinished(i18n.t("split-done", { count }), first);
    }}
  />
{/if}

{#if extracting}
  {@const tab = extracting}
  <ExtractDialog
    pageCount={tab.info.pageCount}
    initial={String(tab.page + 1)}
    oncancel={() => (extracting = null)}
    onrun={(pages, deleteAfter) => {
      extracting = null;
      extractFlow(tab, pages, deleteAfter);
    }}
  />
{/if}

{#if passwordFor}
  <PasswordDialog
    fileName={fileName(passwordFor.path)}
    wrong={passwordFor.wrong}
    onsubmit={(pw) => passwordFor && openPath(passwordFor.path, pw)}
    oncancel={() => (passwordFor = null)}
  />
{/if}

{#if showProperties && active}
  {@const tab = active}
  <PropertiesDialog
    docId={tab.docId}
    path={tab.path}
    pageSize={tab.info.pageSizes[tab.page] ?? tab.info.pageSizes[0]}
    onsave={(m) => applyMetadata(tab, m)}
    oncancel={() => (showProperties = false)}
    onerror={(e) => (error = e)}
  />
{/if}

{#if printChoice && active}
  <PrintDialog
    docId={active.docId}
    title={active.title}
    pageSizes={active.info.pageSizes}
    current={active.page}
    onprint={printNative}
    onsystemprint={printPages}
    oncancel={() => (printChoice = false)}
  />
{/if}

{#if sendingToPrinter}
  <div class="print-progress" role="status" aria-live="polite" in:fade out:out>
    <span>{i18n.t("printing-sending")}</span>
    <progress></progress>
  </div>
{/if}

{#if printing}
  <div class="print-progress" role="status" aria-live="polite">
    <span>{i18n.t("preparing-print", { done: printing.done, total: printing.total })}</span>
    <progress max={printing.total} value={printing.done}></progress>
    <button onclick={() => printing?.controller.abort()}>{i18n.t("cancel")}</button>
  </div>
{/if}

{#if updater.status}
  <UpdateDialog beforeinstall={offerToSaveAll} />
{/if}

{#if question}
  <ConfirmDialog
    title={question.title}
    message={question.message}
    choices={question.choices}
    onchoose={question.answer}
  />
{/if}

{#if showSettings}
  <SettingsDialog
    onclose={() => (showSettings = false)}
    oncheckupdates={() => updater.check(true)}
    onerror={(e) => (error = toRivetError(e))}
  />
{/if}

{#if defaultApp.card}
  <DefaultAppCard onerror={(e) => (error = e)} />
{/if}

{#if contextMenu}
  <ContextMenu {...contextMenu} onclose={() => (contextMenu = null)} />
{/if}

{#if showAbout}
  <AboutDialog onclose={() => (showAbout = false)} />
{/if}

<style>
  main {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-block: 8px;
    padding-inline: 16px;
    background: var(--error-bg);
    color: var(--error-fg);
  }
  .print-progress {
    position: fixed;
    inset-block-end: 24px;
    inset-inline-start: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    padding-block: 10px;
    padding-inline: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.2);
    z-index: 10;
  }
  :global([dir="rtl"]) .print-progress {
    transform: translateX(50%);
  }
  .print-progress progress {
    width: 160px;
    accent-color: var(--accent);
  }
  /* Holds the comments panel (and its splitter) so it can fade in and out. */
  .comments-wrap {
    position: relative;
    display: flex;
    flex: none;
    min-height: 0;
  }
  /* Holds the sidebar so it can fade in and out when shown or hidden. */
  .sidebar-wrap {
    position: relative;
    display: flex;
    flex: none;
    min-height: 0;
  }
  .drop-overlay {
    position: absolute;
    inset: 12px;
    display: grid;
    place-items: center;
    border: 3px dashed var(--accent);
    border-radius: 16px;
    background: color-mix(in srgb, var(--canvas) 85%, transparent);
    font-size: 18px;
    font-weight: 600;
    color: var(--accent);
    pointer-events: none;
  }
</style>
