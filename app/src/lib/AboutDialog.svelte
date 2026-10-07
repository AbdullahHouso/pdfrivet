<script lang="ts">
import { getVersion } from "@tauri-apps/api/app";
import logoUrl from "../../../assets/brand/rivet-icon.svg";
import { i18n } from "./i18n.svelte";

interface Props {
  onclose: () => void;
}
let { onclose }: Props = $props();

let dialog: HTMLDialogElement;
let version = $state("");

$effect(() => {
  dialog.showModal();
  getVersion()
    .then((v) => (version = v))
    .catch(() => {});
  return () => dialog.close();
});
</script>

<dialog bind:this={dialog} aria-labelledby="about-title" onclose={onclose}>
  <img src={logoUrl} alt="" width="72" height="72" />
  <h2 id="about-title">{i18n.t("app-name")}</h2>
  {#if version}<p class="version">{i18n.t("version", { version })}</p>{/if}
  <p>{i18n.t("app-tagline")}</p>
  <p class="small">{i18n.t("about-license")}</p>
  <p class="small" dir="ltr">github.com/AbdullahHouso/rivet-pdf</p>
  <form method="dialog">
    <button class="primary">{i18n.t("close")}</button>
  </form>
</dialog>

<style>
  dialog {
    width: min(360px, calc(100vw - 32px));
    padding: 24px;
    text-align: center;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  h2 {
    margin-block: 8px 0;
  }
  .version,
  .small {
    color: var(--muted);
    font-size: 13px;
  }
  form {
    margin-block-start: 16px;
  }
</style>
