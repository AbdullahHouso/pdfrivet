// "Separate windows" mode: each PDF opens in its own window (and gets its
// own taskbar/Dock entry). Every window runs this same app; a new one is told
// which file to open through its URL (`?open=<path>`).

import { getAllWebviewWindows, WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Each new window appears a little below and to the side of the current one. */
const CASCADE = 32;

/** Opens a PDF in a new window, the same size as this one. */
export async function openInNewWindow(path: string): Promise<void> {
  const current = getCurrentWindow();
  const [position, size, scale] = await Promise.all([
    current.outerPosition(),
    current.innerSize(),
    current.scaleFactor(),
  ]);
  const label = `doc-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
  const window = new WebviewWindow(label, {
    url: `index.html?open=${encodeURIComponent(path)}`,
    title: "PDFRivet",
    x: position.x / scale + CASCADE,
    y: position.y / scale + CASCADE,
    width: size.width / scale,
    height: size.height / scale,
    minWidth: 480,
    minHeight: 360,
    dragDropEnabled: true,
  });
  window.once("tauri://error", (e) => console.warn("[windows] could not open a window", e));
}

/** The file this window was opened for, if it was created by `openInNewWindow`. */
export function fileFromUrl(): string | null {
  return new URLSearchParams(window.location.search).get("open");
}

/** Whether other PDFRivet windows are open besides this one. */
export async function hasOtherWindows(): Promise<boolean> {
  return (await getAllWebviewWindows()).length > 1;
}
