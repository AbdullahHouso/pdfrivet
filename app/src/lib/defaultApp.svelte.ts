// Encourages making PDFRivet the default PDF app. After a file opens, a small
// card offers it when another app opens PDFs (DefaultAppCard.svelte); Settings →
// General shows the same thing. On Windows the user finishes in the system
// settings, so we look again whenever they come back to PDFRivet.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { isDefaultPdfApp, makeDefaultPdfApp } from "./pdf";
import { settings } from "./settings.svelte";

/** How long "Not now" keeps the offer away. */
const SNOOZE_MS = 7 * 24 * 60 * 60 * 1000;

/** null: unknown, or nothing to change here (e.g. a development build on Linux). */
let isDefault = $state<boolean | null>(null);
/** The card: offering, or explaining what to do in the system settings. */
let card = $state<"offer" | "settings" | null>(null);
// Offered at most once per window run.
let offered = false;
let unwatch: Promise<() => void> | null = null;

async function check(): Promise<boolean | null> {
  try {
    isDefault = await isDefaultPdfApp();
  } catch {
    isDefault = null;
  }
  return isDefault;
}

/** Looks again each time the window gets focus back, until PDFRivet is the default. */
function watchFocus() {
  unwatch ??= getCurrentWindow().onFocusChanged(async ({ payload: focused }) => {
    if (!focused || !(await check())) return;
    card = null;
    unwatch?.then((stop) => stop());
    unwatch = null;
  });
}

export const defaultApp = {
  get isDefault() {
    return isDefault;
  },
  get card() {
    return card;
  },
  check,

  /** After a file opened: offers once per run, unless declined. */
  async offer() {
    if (offered || !settings.defaultAppAsk || Date.now() < settings.defaultAppSnooze) return;
    offered = true;
    if ((await check()) === false) card = "offer";
  },

  /** Makes PDFRivet the default, or opens the system settings for it. Throws a RivetError. */
  async make() {
    if ((await makeDefaultPdfApp()) === "done") {
      await check();
      card = null;
    } else {
      if (card) card = "settings";
      watchFocus();
    }
  },

  notNow() {
    card = null;
    settings.defaultAppSnooze = Date.now() + SNOOZE_MS;
  },
  never() {
    card = null;
    settings.defaultAppAsk = false;
  },
  close() {
    card = null;
  },
};
