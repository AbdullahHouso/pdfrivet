<script lang="ts">
// Shows the update check: checking, up to date, a new version with its notes,
// download progress, or an error.
import { i18n } from "./i18n.svelte";
import { updater } from "./updater.svelte";

interface Props {
  /** Deals with unsaved documents before the app restarts; false stops the update. */
  beforeinstall: () => Promise<boolean>;
}
let { beforeinstall }: Props = $props();

let dialog: HTMLDialogElement;
let status = $derived(updater.status);
let busy = $derived(status?.kind === "downloading" || status?.kind === "installing");

$effect(() => {
  dialog.showModal();
  return () => dialog.close();
});
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="update-title"
  aria-busy={busy || status?.kind === "checking"}
  oncancel={(e) => {
    e.preventDefault();
    updater.dismiss();
  }}
>
  {#if status?.kind === "checking"}
    <h2 id="update-title">{i18n.t("update-checking")}</h2>
    <progress></progress>
    <div class="actions">
      <button onclick={() => updater.dismiss()}>{i18n.t("cancel")}</button>
    </div>
  {:else if status?.kind === "up-to-date"}
    <h2 id="update-title">{i18n.t("update-up-to-date-title")}</h2>
    <p>{i18n.t("update-up-to-date", { version: status.version })}</p>
    <div class="actions">
      <!-- svelte-ignore a11y_autofocus -->
      <button class="primary" autofocus onclick={() => updater.dismiss()}>{i18n.t("close")}</button>
    </div>
  {:else if status?.kind === "available"}
    {@const update = status.update}
    <h2 id="update-title">{i18n.t("update-available-title")}</h2>
    <p>{i18n.t("update-available", { version: update.version, current: update.currentVersion })}</p>
    {#if update.body?.trim()}
      <h3>{i18n.t("update-notes")}</h3>
      <!-- Release notes are plain text and usually English. -->
      <div class="notes" dir="auto">{update.body.trim()}</div>
    {/if}
    <div class="actions">
      <button class="skip" onclick={() => updater.dismiss(true)}>{i18n.t("update-skip")}</button>
      <button onclick={() => updater.dismiss()}>{i18n.t("update-later")}</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="primary" autofocus onclick={() => updater.install(beforeinstall)}>
        {i18n.t("update-now")}
      </button>
    </div>
  {:else if status?.kind === "downloading"}
    <h2 id="update-title">{i18n.t("update-available-title")}</h2>
    {#if status.total}
      {@const progress = Math.min(status.downloaded / status.total, 1)}
      <p role="status">{i18n.t("update-downloading", { progress })}</p>
      <progress max="1" value={progress}></progress>
    {:else}
      <p role="status">{i18n.t("update-downloading-unknown")}</p>
      <progress></progress>
    {/if}
  {:else if status?.kind === "installing"}
    <h2 id="update-title">{i18n.t("update-available-title")}</h2>
    <p role="status">{i18n.t("update-installing")}</p>
    <progress></progress>
  {:else if status?.kind === "error"}
    <h2 id="update-title">
      {i18n.t(status.during === "check" ? "update-check-failed" : "update-install-failed")}
    </h2>
    <p>{i18n.t(`update-error-${status.reason}`)}</p>
    <p class="detail" dir="auto">{status.detail}</p>
    <div class="actions">
      <button onclick={() => updater.dismiss()}>{i18n.t("close")}</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="primary" autofocus onclick={() => updater.check(true)}>{i18n.t("try-again")}</button>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    width: min(460px, calc(100vw - 32px));
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.25);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 17px;
  }
  h3 {
    margin: 16px 0 6px;
    font-size: 13px;
    color: var(--muted);
  }
  .notes {
    max-height: 220px;
    overflow: auto;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 13px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .detail {
    color: var(--muted);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  progress {
    width: 100%;
    accent-color: var(--accent);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-block-start: 16px;
  }
  /* "Skip this version" sits apart from the main answers. */
  .skip {
    margin-inline-end: auto;
  }
</style>
