// "Separate windows" mode: each PDF opens in its own window (and gets its
// own taskbar/Dock entry). Every window runs this same app; a new one is told
// what to show through its URL: `?open=<path>` for a file, or `?tab=<state>`
// for a tab moved from another window (its document is already open).

import { getAllWebviewWindows, WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { TabState } from "./tabs.svelte";

/** Each new window appears a little below and to the side of the current one. */
const CASCADE = 32;

async function openWindow(query: string, offset: number): Promise<void> {
  const current = getCurrentWindow();
  const [position, size, scale] = await Promise.all([
    current.outerPosition(),
    current.innerSize(),
    current.scaleFactor(),
  ]);
  const label = `doc-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
  const shift = CASCADE * offset;
  const window = new WebviewWindow(label, {
    url: `index.html?${query}`,
    title: "PDFRivet",
    x: position.x / scale + shift,
    y: position.y / scale + shift,
    width: size.width / scale,
    height: size.height / scale,
    minWidth: 480,
    minHeight: 360,
    dragDropEnabled: true,
    // Lets touchpad pinches reach the page (see main.ts); WebView2 ties both to this.
    zoomHotkeysEnabled: true,
    // Shown once its document is on screen (see reveal.ts).
    visible: false,
  });
  window.once("tauri://error", (e) => console.warn("[windows] could not open a window", e));
}

/** Opens a PDF in a new window, the same size as this one. */
export function openInNewWindow(path: string): Promise<void> {
  return openWindow(`open=${encodeURIComponent(path)}`, 1);
}

/** Moves a tab to a new window (the nth one moved is placed n steps further). */
export function moveToNewWindow(tab: TabState, n = 1): Promise<void> {
  return openWindow(`tab=${encodeURIComponent(JSON.stringify(tab))}`, n);
}

/** What this window was opened for, if it was created by one of the functions above. */
export function windowRequest(): { file: string | null; tab: TabState | null } {
  const params = new URLSearchParams(window.location.search);
  let tab: TabState | null = null;
  try {
    tab = JSON.parse(params.get("tab") ?? "null");
  } catch {}
  return { file: params.get("open"), tab };
}

/** Whether other PDFRivet windows are open besides this one. */
export async function hasOtherWindows(): Promise<boolean> {
  return (await getAllWebviewWindows()).length > 1;
}

/**
 * The window that collects every tab when switching back to tabs: the main
 * window if it's still open, otherwise the oldest one. Every window computes
 * the same answer.
 */
export async function tabsWindowLabel(): Promise<string> {
  const labels = (await getAllWebviewWindows()).map((w) => w.label);
  if (labels.includes("main")) return "main";
  return labels.sort()[0] ?? "main";
}
