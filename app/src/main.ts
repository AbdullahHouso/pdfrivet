import "./fonts.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { installTooltips } from "./lib/tooltip";

// Release builds are an app, not a web page: no browser right-click menu
// (Refresh, Save as, Print…) and no reload shortcuts that would lose unsaved
// changes. Text boxes keep their menu (cut, copy, paste, spell-check).
// Development builds keep everything, including Inspect, for debugging.
if (!import.meta.env.DEV) {
  window.addEventListener("contextmenu", (e) => {
    const target = e.target as HTMLElement | null;
    const editable = target?.closest("input, textarea, [contenteditable='true']");
    if (!editable) e.preventDefault();
  });
  window.addEventListener("keydown", (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === "F5" || (mod && e.key.toLowerCase() === "r")) e.preventDefault();
  });
}

// The browser's own zoom would scale the whole interface. It stays on (WebView2
// only delivers touchpad pinches when it's enabled), so block it here: the
// viewer zooms the pages on Ctrl+wheel and pinch, App.svelte on Ctrl +/-/0.
window.addEventListener(
  "wheel",
  (e) => {
    if (e.ctrlKey) e.preventDefault();
  },
  { passive: false },
);
const zoomKeys = new Set(["Equal", "Minus", "Digit0", "NumpadAdd", "NumpadSubtract", "Numpad0"]);
window.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && zoomKeys.has(e.code)) e.preventDefault();
});

installTooltips();
mount(App, { target: document.getElementById("app") as HTMLElement });
