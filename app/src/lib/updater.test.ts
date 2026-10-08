import { beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
const relaunch = vi.fn();
vi.mock("@tauri-apps/plugin-updater", () => ({ check: (...a: unknown[]) => check(...a) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => relaunch() }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.1.0" }));
vi.mock("@tauri-apps/plugin-store", () => ({ load: vi.fn() }));

const { CHECK_INTERVAL, isCheckDue } = await import("./updater.svelte");
// Fresh copies for every test, so no update dialog state carries over.
let updater: typeof import("./updater.svelte").updater;
let settings: typeof import("./settings.svelte").settings;

function fakeUpdate(version = "0.2.0") {
  return {
    version,
    currentVersion: "0.1.0",
    body: "Notes",
    close: vi.fn(async () => {}),
    install: vi.fn(async () => {}),
    download: vi.fn(async (onEvent: (e: unknown) => void) => {
      onEvent({ event: "Started", data: { contentLength: 100 } });
      onEvent({ event: "Progress", data: { chunkLength: 60 } });
      onEvent({ event: "Progress", data: { chunkLength: 40 } });
      onEvent({ event: "Finished" });
    }),
  };
}

beforeEach(async () => {
  vi.resetModules();
  ({ updater } = await import("./updater.svelte"));
  ({ settings } = await import("./settings.svelte"));
  check.mockReset();
  relaunch.mockReset();
});

describe("isCheckDue", () => {
  it("checks once a day", () => {
    expect(isCheckDue(0, CHECK_INTERVAL)).toBe(true);
    expect(isCheckDue(1000, 1000 + CHECK_INTERVAL - 1)).toBe(false);
  });
  it("checks when the clock went backwards", () => {
    expect(isCheckDue(5000, 1000)).toBe(true);
  });
});

describe("updater", () => {
  it("tells a manual check that the app is up to date", async () => {
    check.mockResolvedValue(null);
    await updater.check(true);
    expect(updater.status).toEqual({ kind: "up-to-date", version: "0.1.0" });
  });

  it("stays silent when an automatic check finds nothing or fails", async () => {
    check.mockResolvedValue(null);
    await updater.check(false);
    expect(updater.status).toBeNull();
    check.mockRejectedValue(new Error("offline"));
    await updater.check(false);
    expect(updater.status).toBeNull();
  });

  it("shows errors from a manual check", async () => {
    check.mockRejectedValue(new Error("offline"));
    await updater.check(true);
    expect(updater.status).toEqual({ kind: "error", detail: "offline" });
  });

  it("offers a new version, but not a skipped one on automatic checks", async () => {
    check.mockResolvedValue(fakeUpdate());
    await updater.check(false);
    expect(updater.status?.kind).toBe("available");

    await updater.dismiss(true);
    expect(settings.skippedVersion).toBe("0.2.0");
    const skipped = fakeUpdate();
    check.mockResolvedValue(skipped);
    await updater.check(false);
    expect(updater.status).toBeNull();
    expect(skipped.close).toHaveBeenCalled();

    // Asking explicitly still offers it.
    check.mockResolvedValue(fakeUpdate());
    await updater.check(true);
    expect(updater.status?.kind).toBe("available");
  });

  it("ignores the result of a check the user cancelled", async () => {
    const update = fakeUpdate();
    let resolve: (u: unknown) => void = () => {};
    check.mockReturnValue(new Promise((r) => (resolve = r)));
    const pending = updater.check(true);
    await updater.dismiss();
    resolve(update);
    await pending;
    expect(updater.status).toBeNull();
    expect(update.close).toHaveBeenCalled();
  });

  it("downloads, installs and restarts", async () => {
    const update = fakeUpdate();
    check.mockResolvedValue(update);
    await updater.check(true);
    const seen: unknown[] = [];
    await updater.install(async () => {
      seen.push(updater.status);
      return true;
    });
    expect(seen[0]).toMatchObject({ kind: "downloading", downloaded: 100, total: 100 });
    expect(update.install).toHaveBeenCalled();
    expect(relaunch).toHaveBeenCalled();
  });

  it("keeps the offer when the user cancels saving before the restart", async () => {
    const update = fakeUpdate();
    check.mockResolvedValue(update);
    await updater.check(true);
    await updater.install(async () => false);
    expect(update.install).not.toHaveBeenCalled();
    expect(updater.status?.kind).toBe("available");
  });
});
