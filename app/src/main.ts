import "@fontsource/ibm-plex-sans-arabic/400.css";
import "@fontsource/ibm-plex-sans-arabic/600.css";
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

installTooltips();
mount(App, { target: document.getElementById("app") as HTMLElement });
