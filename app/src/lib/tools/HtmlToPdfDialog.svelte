<script lang="ts">
// Web page or HTML file to PDF. The page is printed by the system's web
// engine in a hidden window sealed off from PDFRivet (html_pdf.rs).
import { open, save } from "@tauri-apps/plugin-dialog";
import { i18n } from "../i18n.svelte";
import { type HtmlSource, htmlToPdf, type RivetError, toRivetError } from "../pdf";
import { fileNameOf, folderOf, separatorOf } from "./files";
import ToolDialog from "./ToolDialog.svelte";

interface Props {
  /** Where to suggest saving (the open document's folder), if any. */
  folder?: string;
  onfinished: (path: string, open: boolean) => void;
  onerror: (e: RivetError) => void;
  oncancel: () => void;
}
let { folder, onfinished, onerror, oncancel }: Props = $props();

let kind = $state<"url" | "file">("url");
let url = $state("");
let file = $state("");
let paper = $state<"a4" | "letter">("a4");
let landscape = $state(false);
let margin = $state(10);
let backgrounds = $state(true);
let openAfter = $state(true);
let working = $state(false);

let source = $derived.by((): HtmlSource | null => {
  if (kind === "url") return url.trim() ? { kind: "url", url: url.trim() } : null;
  return file ? { kind: "file", path: file } : null;
});

async function pickFile() {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "HTML", extensions: ["html", "htm"] }],
  });
  if (typeof picked === "string") file = picked;
}

/** A file name from the page: the HTML file's name, or the site's address. */
function suggestedName(): string {
  if (kind === "file") return fileNameOf(file).replace(/\.html?$/i, "");
  try {
    const u = new URL(url.includes("://") ? url : `https://${url}`);
    const last = u.pathname.split("/").filter(Boolean).at(-1);
    return (last ? `${u.hostname} - ${decodeURIComponent(last)}` : u.hostname).replace(/[\\/:*?"<>|]/g, " ");
  } catch {
    return "page";
  }
}

async function run() {
  if (!source || working) return;
  const where = kind === "file" ? folderOf(file) + separatorOf(file) : folder ? folder : "";
  const target = await save({
    defaultPath: `${where}${suggestedName()}.pdf`,
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (!target) return;
  const path = /\.pdf$/i.test(target) ? target : `${target}.pdf`;
  working = true;
  try {
    await htmlToPdf(source, { paper, landscape, marginMm: margin, backgrounds }, path);
    onfinished(path, openAfter);
  } catch (e) {
    onerror(toRivetError(e));
  } finally {
    working = false;
  }
}
</script>

<ToolDialog title={i18n.t("html-title")} icon="globe" oncancel={() => !working && oncancel()} onsubmit={run}
  progress={working ? 0.5 : null}>
  <fieldset disabled={working}>
    <div class="choice" role="radiogroup" aria-label={i18n.t("html-title")}>
      <label class="check"><input type="radio" value="url" bind:group={kind} />{i18n.t("html-from-url")}</label>
      <label class="check"><input type="radio" value="file" bind:group={kind} />{i18n.t("html-from-file")}</label>
    </div>
    {#if kind === "url"}
      <label class="field">
        {i18n.t("html-address")}
        <!-- svelte-ignore a11y_autofocus -->
        <input type="url" bind:value={url} dir="ltr" placeholder="https://example.com/page" autofocus />
        <span class="hint">{i18n.t("html-privacy")}</span>
      </label>
    {:else}
      <div class="file">
        <bdi class="path" dir="ltr" title={file}>{file || i18n.t("html-no-file")}</bdi>
        <button type="button" onclick={pickFile}>{i18n.t("split-choose-folder")}</button>
      </div>
      <span class="hint">{i18n.t("html-file-hint")}</span>
    {/if}
    <div class="row">
      <label class="field">
        {i18n.t("images-page-size")}
        <select bind:value={paper}>
          <option value="a4">A4</option>
          <option value="letter">{i18n.t("images-letter")}</option>
        </select>
      </label>
      <label class="field">
        {i18n.t("html-orientation")}
        <select bind:value={landscape}>
          <option value={false}>{i18n.t("html-portrait")}</option>
          <option value={true}>{i18n.t("html-landscape")}</option>
        </select>
      </label>
      <label class="field">
        {i18n.t("images-margin")}
        <select bind:value={margin}>
          <option value={0}>{i18n.t("images-margin-none")}</option>
          <option value={10}>{i18n.t("images-margin-small")}</option>
          <option value={20}>{i18n.t("images-margin-large")}</option>
        </select>
      </label>
    </div>
    <label class="check"><input type="checkbox" bind:checked={backgrounds} />{i18n.t("html-backgrounds")}</label>
    <label class="check"><input type="checkbox" bind:checked={openAfter} />{i18n.t("images-open-after")}</label>
    {#if working}<p class="hint">{i18n.t("html-working")}</p>{/if}
  </fieldset>
  {#snippet actions()}
    <button type="button" onclick={oncancel} disabled={working}>{i18n.t("cancel")}</button>
    <button type="submit" class="primary" disabled={!source || working}>{i18n.t("html-run")}</button>
  {/snippet}
</ToolDialog>

<style>
  fieldset {
    min-width: 0;
    border: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .choice {
    display: flex;
    gap: 20px;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .file .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font-size: 13px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 12px;
  }
  select {
    width: 100%;
  }
  p.hint {
    margin: 0;
  }
</style>
