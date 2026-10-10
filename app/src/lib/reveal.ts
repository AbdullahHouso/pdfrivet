// Windows start hidden (tauri.conf.json, docWindows.ts) and are shown here
// once the app has drawn what they open with: the document from "Open with",
// or the start page. Showing them earlier flashed a white window, the start
// page before the document, and (with several screens) the window jumping
// from the default screen to the remembered one.

import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Shown anyway after this long, so a slow or failed start never leaves no window. */
const AT_THE_LATEST = 4000;

let revealed = false;

/**
 * Paints the window's background (behind the page) in the theme's colour, so
 * resizing or a frame drawn before the page shows no white.
 */
export async function matchBackground(): Promise<void> {
  const [r, g, b] = (getComputedStyle(document.body).backgroundColor.match(/\d+/g) ?? []).map(Number);
  if ([r, g, b].some((v) => v === undefined || Number.isNaN(v))) return;
  try {
    await getCurrentWebviewWindow().setBackgroundColor([r, g, b, 255]);
  } catch {
    // Not in the app (tests), or not allowed: only resizing would show it.
  }
}

/** Shows this window (once), after the next frame is drawn. */
export async function revealWindow(): Promise<void> {
  if (revealed) return;
  revealed = true;
  await matchBackground();
  // Let the browser lay out and paint what's there first.
  await new Promise((resolve) => setTimeout(resolve, 16));
  const window = getCurrentWebviewWindow();
  await window.show().catch(() => {});
  await window.setFocus().catch(() => {});
}

/** Called once at start-up. */
export function installReveal(): void {
  setTimeout(revealWindow, AT_THE_LATEST);
  // The System theme follows the computer's light or dark mode.
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => matchBackground());
}
