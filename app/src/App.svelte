<script lang="ts">
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import AboutDialog from "./lib/AboutDialog.svelte";
import { i18n } from "./lib/i18n.svelte";
import { type Degrees, stepZoom } from "./lib/layout";
import PasswordDialog from "./lib/PasswordDialog.svelte";
import { openDocument, type RivetError, takePendingFiles, toRivetError } from "./lib/pdf";
import type { ZoomMode } from "./lib/recent";
import Sidebar from "./lib/Sidebar.svelte";
import StartScreen from "./lib/StartScreen.svelte";
import { settings } from "./lib/settings.svelte";
import TabBar from "./lib/TabBar.svelte";
import Toolbar from "./lib/Toolbar.svelte";
import { tabs } from "./lib/tabs.svelte";
import Viewer from "./lib/Viewer.svelte";

let error = $state<RivetError | null>(null);
let sidebarOpen = $state(true);
let showAbout = $state(false);
let dragging = $state(false);
/** A protected PDF waiting for its password. */
let passwordFor = $state<{ path: string; wrong: boolean } | null>(null);

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
    [mod && key === "o", pickFiles],
    [e.key === "F11", toggleFullscreen],
    [!!tab && mod && key === "w", () => tab && tabs.close(tab.id)],
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
  <TabBar onopen={pickFiles} />
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
