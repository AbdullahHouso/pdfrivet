<script lang="ts">
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import AboutDialog from "./lib/AboutDialog.svelte";
import ConfirmDialog, { type Choice } from "./lib/ConfirmDialog.svelte";
import { i18n } from "./lib/i18n.svelte";
import { type Degrees, stepZoom } from "./lib/layout";
import PasswordDialog from "./lib/PasswordDialog.svelte";
import PrintDialog from "./lib/PrintDialog.svelte";
import { openDocument, type RivetError, saveDocument, takePendingFiles, toRivetError } from "./lib/pdf";
import { type PrintProgress, printDocument } from "./lib/print";
import type { ZoomMode } from "./lib/recent";
import Sidebar from "./lib/Sidebar.svelte";
import StartScreen from "./lib/StartScreen.svelte";
import { settings } from "./lib/settings.svelte";
import TabBar from "./lib/TabBar.svelte";
import Toolbar from "./lib/Toolbar.svelte";
import { type Tab, tabs } from "./lib/tabs.svelte";
import Viewer from "./lib/Viewer.svelte";

let error = $state<RivetError | null>(null);
let sidebarOpen = $state(true);
let showAbout = $state(false);
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
  for (const path of picked ?? []) await openPath(path);
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
    // Reopen where you left off.
    const saved = settings.findRecent(path);
    if (saved) {
      tab.page = Math.min(saved.page, info.pageCount - 1);
      tab.zoomMode = saved.zoomMode;
      tab.zoom = saved.zoom;
    }
    settings.touchRecent({
      path,
      title: tab.title,
      lastOpened: Date.now(),
      page: tab.page,
      zoom: tab.zoom,
      zoomMode: tab.zoomMode,
    });
  } catch (e) {
    const err = toRivetError(e);
    if (err.code === "password-required" || err.code === "wrong-password") {
      passwordFor = { path, wrong: err.code === "wrong-password" };
    } else {
      error = err;
    }
  }
}

/** Shows the "which pages?" dialog; printing starts from there. */
let printChoice = $state(false);

function printActive() {
  if (active && !printing) printChoice = true;
}

async function printPages(pages: number[]) {
  const tab = active;
  printChoice = false;
  if (!tab || printing) return;
  const controller = new AbortController();
  printing = { done: 0, total: pages.length, controller };
  try {
    await printDocument(
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
    await saveDocument(tab.docId, path);
    tab.path = path;
    tab.dirty = false;
    settings.touchRecent({
      path,
      title: tab.title,
      lastOpened: Date.now(),
      page: tab.page,
      zoom: tab.zoom,
      zoomMode: tab.zoomMode,
    });
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
}

async function openPending() {
  for (const path of await takePendingFiles()) await openPath(path);
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

// Remember where you are in each document (saved shortly after you stop moving).
$effect(() => {
  if (!active) return;
  const { path, page, zoom, zoomMode } = active;
  const timer = setTimeout(() => settings.updatePosition(path, { page, zoom, zoomMode }), 800);
  return () => clearTimeout(timer);
});

// Window title: "Document – Rivet".
$effect(() => {
  const title = active ? `${active.title} – ${i18n.t("app-name")}` : i18n.t("app-name");
  document.title = title;
  getCurrentWindow()
    .setTitle(title)
    .catch(() => {});
});

/** Keyboard shortcuts. Each returns true when it handled the key. */
function onKey(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
  const tab = active;

  const shortcuts: [boolean, () => void][] = [
    // Our own printing (the default would print the app window).
    [mod && key === "p", () => tab && printActive()],
    [mod && key === "o", pickFiles],
    [e.key === "F11", toggleFullscreen],
    [!!tab && mod && key === "w", () => tab && closeTab(tab)],
    [!!tab && mod && e.shiftKey && key === "s", () => tab && saveTab(tab, true)],
    [!!tab && mod && !e.shiftKey && key === "s", () => tab?.dirty && saveTab(tab)],
    [!!tab && mod && e.key === "Tab", () => tabs.cycle(e.shiftKey ? -1 : 1)],
    [!!tab && mod && (key === "=" || key === "+"), () => zoomStep(1)],
    [!!tab && mod && key === "-", () => zoomStep(-1)],
    [!!tab && mod && key === "0", () => setZoomMode("fit-width")],
    [!!tab && ((mod && key === "g") || e.key === "F6"), () => toolbar?.focusPageInput()],
    [!!tab && !typing && !mod && e.key === "Home", () => goTo(0)],
    [!!tab && !typing && !mod && e.key === "End", () => tab && goTo(tab.info.pageCount - 1)],
  ];
  const match = shortcuts.find(([when]) => when);
  if (match) {
    e.preventDefault();
    match[1]();
  }
}

onMount(() => {
  // Load settings first, so reopened files find their saved position.
  settings.init().then(openPending);
  const unlisten = [
    // Ask before closing the window with unsaved changes.
    getCurrentWindow().onCloseRequested(async (event) => {
      const unsaved = tabs.list.filter((t) => t.dirty);
      if (unsaved.length === 0) return;
      event.preventDefault();
      const answer = await ask(
        i18n.t("unsaved-title"),
        i18n.t("unsaved-message-many", { count: unsaved.length }),
        saveChoices(),
      );
      if (answer === "cancel") return;
      if (answer === "save") {
        for (const tab of unsaved) if (!(await saveTab(tab))) return;
      }
      await getCurrentWindow().destroy();
    }),
    // Files from "Open with" or a second launch while Rivet is running.
    listen("open-files", () => openPending()),
    getCurrentWebview().onDragDropEvent(async (event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") dragging = true;
      else if (p.type === "leave") dragging = false;
      else if (p.type === "drop") {
        dragging = false;
        for (const path of p.paths.filter((x) => x.toLowerCase().endsWith(".pdf"))) await openPath(path);
      }
    }),
  ];
  return () => {
    for (const u of unlisten) u.then((stop) => stop());
  };
});
</script>

<svelte:window onkeydown={onKey} />

{#if tabs.list.length > 0}
  <TabBar onopen={pickFiles} onclose={closeTab} />
{/if}

<Toolbar
  bind:this={toolbar}
  tab={active}
  {sidebarOpen}
  onopen={pickFiles}
  ontogglesidebar={() => (sidebarOpen = !sidebarOpen)}
  ongoto={goTo}
  onzoomstep={zoomStep}
  onzoom={(z) => viewer?.zoomTo(z)}
  onzoommode={setZoomMode}
  onrotate={rotate}
  onabout={() => (showAbout = true)}
  onsave={() => active && saveTab(active)}
  onsaveas={() => active && saveTab(active, true)}
  onstep={(d) => viewer?.step(d)}
  onprint={printActive}
  onactualsize={() => viewer?.zoomTo(1)}
  onpagelayout={(layout) => {
    if (active) active.pageLayout = layout;
    settings.pageLayout = layout;
  }}
  oncontinuous={(on) => {
    if (active) active.continuous = on;
    settings.continuous = on;
  }}
/>

{#if error}
  <div class="error" role="alert">
    <span>{i18n.t(`error-${error.code}`)}</span>
    <button onclick={() => (error = null)}>{i18n.t("dismiss")}</button>
  </div>
{/if}

<main>
  {#if active}
    {#key active.id}
      {#if sidebarOpen}
        <Sidebar tab={active} ongoto={goTo} />
      {/if}
      <Viewer bind:this={viewer} tab={active} onerror={(e) => (error = e)} />
    {/key}
  {:else}
    <StartScreen onopen={pickFiles} onopenpath={(p) => openPath(p)} />
  {/if}

  {#if dragging}
    <div class="drop-overlay" aria-hidden="true">
      <span>{i18n.t("drop-to-open")}</span>
    </div>
  {/if}
</main>

{#if passwordFor}
  <PasswordDialog
    fileName={fileName(passwordFor.path)}
    wrong={passwordFor.wrong}
    onsubmit={(pw) => passwordFor && openPath(passwordFor.path, pw)}
    oncancel={() => (passwordFor = null)}
  />
{/if}

{#if printChoice && active}
  <PrintDialog
    pageCount={active.info.pageCount}
    current={active.page}
    onprint={printPages}
    oncancel={() => (printChoice = false)}
  />
{/if}

{#if printing}
  <div class="print-progress" role="status" aria-live="polite">
    <span>{i18n.t("preparing-print", { done: printing.done, total: printing.total })}</span>
    <progress max={printing.total} value={printing.done}></progress>
    <button onclick={() => printing?.controller.abort()}>{i18n.t("cancel")}</button>
  </div>
{/if}

{#if question}
  <ConfirmDialog
    title={question.title}
    message={question.message}
    choices={question.choices}
    onchoose={question.answer}
  />
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
