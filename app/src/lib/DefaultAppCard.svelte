<script lang="ts">
// The offer to make PDFRivet the default PDF app, in a corner so the pages
// don't move (see defaultApp.svelte.ts).
import { defaultApp } from "./defaultApp.svelte";
import { i18n } from "./i18n.svelte";
import { out, rise } from "./motion";
import { type RivetError, toRivetError } from "./pdf";

let { onerror }: { onerror: (e: RivetError) => void } = $props();

async function make() {
  try {
    await defaultApp.make();
  } catch (e) {
    onerror(toRivetError(e));
  }
}
</script>

<div class="card" role="status" aria-live="polite" in:rise={{ y: 6 }} out:out>
  {#if defaultApp.card === "settings"}
    <p>{i18n.t("default-app-windows-steps")}</p>
    <div class="actions">
      <button onclick={make}>{i18n.t("default-app-open-settings")}</button>
      <button onclick={defaultApp.close}>{i18n.t("close")}</button>
    </div>
  {:else}
    <strong>{i18n.t("default-app-title")}</strong>
    <p>{i18n.t("default-app-text")}</p>
    <div class="actions">
      <button class="primary" onclick={make}>{i18n.t("default-app-make")}</button>
      <button onclick={defaultApp.notNow}>{i18n.t("default-app-not-now")}</button>
      <button class="link" onclick={defaultApp.never}>{i18n.t("default-app-never")}</button>
    </div>
  {/if}
</div>

<style>
  .card {
    position: fixed;
    inset-block-end: 24px;
    inset-inline-end: 24px;
    width: min(360px, calc(100vw - 48px));
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.2);
    z-index: 10;
  }
  p {
    margin: 0;
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-block-start: 6px;
  }
  .link {
    margin-inline-start: auto;
    border: none;
    background: none;
    color: var(--muted);
    padding-inline: 4px;
  }
  .link:hover {
    color: var(--text);
  }
</style>
