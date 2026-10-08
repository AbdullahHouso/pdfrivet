// Self-update: looks for a new version, downloads it and restarts into it.
//
// Releases publish a `latest.json` (written by tauri-action) next to the
// installers. Every installer is signed with the project's update key and
// the app refuses anything whose signature doesn't match the public key in
// tauri.conf.json, so a tampered download is never installed.

import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { settings } from "./settings.svelte";

/** Automatic checks run at most this often. */
export const CHECK_INTERVAL = 24 * 60 * 60 * 1000;
/** Waits this long after startup so the check never slows down opening a file. */
const STARTUP_DELAY = 5000;

export type UpdateStatus =
  | { kind: "checking" }
  | { kind: "up-to-date"; version: string }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; update: Update; downloaded: number; total: number | null }
  | { kind: "installing"; update: Update }
  | { kind: "error"; detail: string };

/** Whether an automatic check is due (also when the clock went backwards). */
export function isCheckDue(lastCheck: number, now: number): boolean {
  return now - lastCheck >= CHECK_INTERVAL || now < lastCheck;
}

/** What the update dialog shows; null when it is closed. */
let status = $state<UpdateStatus | null>(null);

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export const updater = {
  get status() {
    return status;
  },

  /**
   * Looks for a new version. Manual checks always show the result; automatic
   * ones stay silent unless there is a version the user hasn't skipped.
   */
  async check(manual: boolean) {
    if (status && status.kind !== "up-to-date" && status.kind !== "error") return;
    if (manual) status = { kind: "checking" };
    try {
      const update = await check({ timeout: 30_000 });
      settings.lastUpdateCheck = Date.now();
      // The user closed the "Checking…" dialog while waiting.
      if (manual && status?.kind !== "checking") {
        await update?.close();
        return;
      }
      if (!update) {
        status = manual ? { kind: "up-to-date", version: await getVersion() } : null;
      } else if (!manual && update.version === settings.skippedVersion) {
        await update.close();
      } else {
        status = { kind: "available", update };
      }
    } catch (e) {
      console.warn("[updater] check failed", e);
      if (manual && status?.kind === "checking") status = { kind: "error", detail: errorText(e) };
    }
  },

  /** Schedules the daily automatic check (release builds only). */
  startAutomaticChecks() {
    if (import.meta.env.DEV || !settings.autoUpdate) return;
    if (!isCheckDue(settings.lastUpdateCheck, Date.now())) return;
    setTimeout(() => settings.autoUpdate && updater.check(false), STARTUP_DELAY);
  },

  /**
   * Downloads and installs the offered update, then restarts.
   * `beforeInstall` deals with unsaved documents and returns false to stop.
   */
  async install(beforeInstall: () => Promise<boolean>) {
    if (status?.kind !== "available") return;
    const update = status.update;
    try {
      status = { kind: "downloading", update, downloaded: 0, total: null };
      let downloaded = 0;
      await update.download((event) => {
        if (event.event === "Started") {
          status = { kind: "downloading", update, downloaded: 0, total: event.data.contentLength ?? null };
        } else if (event.event === "Progress" && status?.kind === "downloading") {
          downloaded += event.data.chunkLength;
          status = { ...status, downloaded };
        }
      });
      if (!(await beforeInstall())) {
        status = { kind: "available", update };
        return;
      }
      status = { kind: "installing", update };
      // On Windows the installer closes the app and starts the new version itself.
      await update.install();
      await relaunch();
    } catch (e) {
      console.warn("[updater] install failed", e);
      status = { kind: "error", detail: errorText(e) };
    }
  },

  /** Hides the dialog; "skip" also stops automatic checks offering this version. */
  async dismiss(skip = false) {
    if (status?.kind === "downloading" || status?.kind === "installing") return;
    if (status?.kind === "available") {
      if (skip) settings.skippedVersion = status.update.version;
      await status.update.close().catch(() => {});
    }
    status = null;
  },
};
