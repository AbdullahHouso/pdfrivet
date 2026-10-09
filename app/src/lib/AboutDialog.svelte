<script lang="ts">
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import logoUrl from "../../../assets/brand/rivet-icon.svg";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { dialogOut } from "./motion";

const WEBSITE = "https://pdfrivet.com";
const GITHUB = "https://github.com/AbdullahHouso/pdfrivet";

function open(url: string) {
  openUrl(url).catch((e) => console.warn("[about] could not open", url, e));
}

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

<dialog
  out:dialogOut|global bind:this={dialog} aria-labelledby="about-title"
  oncancel={(e) => {
    // Closed by the app (not the browser), so it can fade out.
    e.preventDefault();
    onclose();
  }}>
  <img src={logoUrl} alt="" width="72" height="72" />
  <h2 id="about-title">{i18n.t("app-name")}</h2>
  {#if version}<p class="version">{i18n.t("version", { version })}</p>{/if}
  <p>{i18n.t("app-tagline")}</p>
  <p class="small">{i18n.t("about-license")}</p>
  <div class="links">
    <button onclick={() => open(WEBSITE)}><Icon name="globe" />{i18n.t("website")}</button>
    <button onclick={() => open(GITHUB)}><Icon name="code" />GitHub</button>
  </div>
  <div class="close">
    <button class="primary" onclick={onclose}>{i18n.t("close")}</button>
  </div>
  <p class="small url" dir="ltr">pdfrivet.com</p>
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
  .links {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-block-start: 16px;
  }
  .links button {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .close {
    margin-block-start: 16px;
  }
  .url {
    margin-block: 16px 0;
  }
</style>
