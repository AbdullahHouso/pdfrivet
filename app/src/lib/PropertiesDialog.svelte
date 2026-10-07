<script lang="ts">
// Document properties: the description (editable), file details and what the
// document allows. Changes are kept in the tab and written into the PDF on save.
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { DocProperties } from "./bindings/DocProperties";
import type { Metadata } from "./bindings/Metadata";
import type { PageSize } from "./bindings/PageSize";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { documentProperties, type RivetError, toRivetError } from "./pdf";

interface Props {
  docId: number;
  /** Size of the page you're on, in points. */
  pageSize: PageSize;
  path: string;
  onsave: (metadata: Metadata) => void;
  oncancel: () => void;
  onerror: (e: RivetError) => void;
}
let { docId, pageSize, path, onsave, oncancel, onerror }: Props = $props();

type Unit = "cm" | "mm" | "in" | "pt";
const PER_POINT: Record<Unit, number> = { cm: 2.54 / 72, mm: 25.4 / 72, in: 1 / 72, pt: 1 };

let dialog: HTMLDialogElement;
let details = $state<DocProperties | null>(null);
let form = $state<Metadata>({ title: "", author: "", subject: "", keywords: "" });
let original = $state<Metadata | null>(null);
let unit = $state<Unit>(readUnit());

function readUnit(): Unit {
  try {
    const saved = localStorage.getItem("rivet.unit");
    if (saved === "cm" || saved === "mm" || saved === "in" || saved === "pt") return saved;
  } catch {
    // Not critical.
  }
  return "cm";
}

$effect(() => {
  try {
    localStorage.setItem("rivet.unit", unit);
  } catch {
    // Not critical.
  }
});

$effect(() => {
  dialog.showModal();
  documentProperties(docId)
    .then((p) => {
      details = p;
      form = { ...p.metadata };
      original = { ...p.metadata };
    })
    .catch((e) => {
      onerror(toRivetError(e));
      oncancel();
    });
  return () => dialog.close();
});

let changed = $derived(
  original !== null &&
    (form.title !== original.title ||
      form.author !== original.author ||
      form.subject !== original.subject ||
      form.keywords !== original.keywords),
);

function submit(e: Event) {
  e.preventDefault();
  if (changed && details?.canEditMetadata) onsave({ ...form });
  else oncancel();
}

function size(value: number): string {
  const n = value * PER_POINT[unit];
  return n.toLocaleString(i18n.locale, { maximumFractionDigits: unit === "pt" ? 0 : 2 });
}

function fileSize(bytes: number): string {
  const units = ["byte", "kilobyte", "megabyte", "gigabyte"] as const;
  let value = bytes;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return new Intl.NumberFormat(i18n.locale, {
    style: "unit",
    unit: units[i],
    unitDisplay: "short",
    maximumFractionDigits: i === 0 ? 0 : 2,
  }).format(value);
}

function date(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? iso
    : new Intl.DateTimeFormat(i18n.locale, { dateStyle: "medium", timeStyle: "short" }).format(d);
}

const yesNo = (v: boolean) => i18n.t(v ? "yes" : "no");
</script>

<dialog bind:this={dialog} aria-labelledby="props-title" oncancel={(e) => { e.preventDefault(); oncancel(); }}>
  <form onsubmit={submit}>
    <header class="head">
      <Icon name="info" />
      <h2 id="props-title">{i18n.t("document-properties-title")}</h2>
    </header>

    {#if details}
      <div class="body">
        <section>
          <h3>{i18n.t("props-description")}</h3>
          {#if !details.canEditMetadata}
            <p class="note">{i18n.t("props-protected-note")}</p>
          {/if}
          <label class="field">
            <span>{i18n.t("props-title")}</span>
            <input dir="auto" bind:value={form.title} readonly={!details.canEditMetadata} />
          </label>
          <label class="field">
            <span>{i18n.t("props-author")}</span>
            <input dir="auto" bind:value={form.author} readonly={!details.canEditMetadata} />
          </label>
          <label class="field">
            <span>{i18n.t("props-subject")}</span>
            <input dir="auto" bind:value={form.subject} readonly={!details.canEditMetadata} />
          </label>
          <label class="field">
            <span>{i18n.t("props-keywords")}</span>
            <textarea dir="auto" rows="3" bind:value={form.keywords} readonly={!details.canEditMetadata}></textarea>
          </label>
          <dl>
            <dt>{i18n.t("props-creator")}</dt><dd dir="auto">{details.creator || "—"}</dd>
            <dt>{i18n.t("props-producer")}</dt><dd dir="auto">{details.producer || "—"}</dd>
            <dt>{i18n.t("props-created")}</dt><dd>{date(details.created)}</dd>
            <dt>{i18n.t("props-modified")}</dt><dd>{date(details.modified)}</dd>
          </dl>
        </section>

        <section>
          <h3>{i18n.t("props-file")}</h3>
          <dl>
            <dt>{i18n.t("props-file-name")}</dt><dd dir="auto">{details.fileName}</dd>
            <dt>{i18n.t("props-location")}</dt>
            <dd>
              <button type="button" class="link" dir="auto" title={i18n.t("show-in-folder")}
                onclick={() => revealItemInDir(path).catch(() => {})}>
                {details.folder}
              </button>
            </dd>
            <dt>{i18n.t("props-file-size")}</dt><dd>{fileSize(details.fileSize)}</dd>
            <dt>{i18n.t("props-pages")}</dt><dd>{details.pageCount.toLocaleString(i18n.locale)}</dd>
            <dt>{i18n.t("props-page-size")}</dt>
            <dd class="page-size">
              <span dir="ltr">{size(pageSize.width)} × {size(pageSize.height)}</span>
              <select bind:value={unit} aria-label={i18n.t("props-unit")}>
                <option value="cm">cm</option>
                <option value="mm">mm</option>
                <option value="in">in</option>
                <option value="pt">pt</option>
              </select>
            </dd>
            <dt>{i18n.t("props-pdf-version")}</dt><dd>{details.pdfVersion}</dd>
          </dl>
        </section>

        <section>
          <h3>{i18n.t("props-advanced")}</h3>
          <dl>
            <dt>{i18n.t("props-tagged")}</dt><dd>{yesNo(details.tagged)}</dd>
            <dt>{i18n.t("props-protected")}</dt><dd>{yesNo(details.encrypted)}</dd>
          </dl>
          <ul class="permissions" aria-label={i18n.t("props-allowed")}>
            {#each [["print", "perm-print"], ["copy", "perm-copy"], ["modify", "perm-modify"], ["fillForms", "perm-fill-forms"], ["annotate", "perm-annotate"]] as const as [key, label] (key)}
              <li class:no={!details.permissions[key]}>
                <span aria-hidden="true">{details.permissions[key] ? "✓" : "✕"}</span>
                {i18n.t(label)}
                <span class="visually-hidden">{yesNo(details.permissions[key])}</span>
              </li>
            {/each}
          </ul>
        </section>
      </div>
    {:else}
      <p class="loading">…</p>
    {/if}

    <footer class="actions">
      <button type="button" onclick={oncancel}>{i18n.t("cancel")}</button>
      <button type="submit" class="primary" disabled={!details}>{i18n.t("ok")}</button>
    </footer>
  </form>
</dialog>

<style>
  dialog {
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.25);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  form {
    display: flex;
    flex-direction: column;
    max-height: calc(100vh - 34px);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-block: 16px 8px;
    padding-inline: 20px;
    color: var(--accent);
  }
  h2 {
    margin: 0;
    font-size: 17px;
    color: var(--text);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-inline: 20px;
    overflow: auto;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
  }
  .note {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
  }
  .field {
    display: grid;
    grid-template-columns: 7em minmax(0, 1fr);
    align-items: start;
    gap: 10px;
  }
  .field span {
    padding-block-start: 6px;
    color: var(--muted);
  }
  input,
  textarea {
    font: inherit;
    color: inherit;
    background: var(--canvas);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding-block: 5px;
    padding-inline: 8px;
    resize: vertical;
  }
  input[readonly],
  textarea[readonly] {
    opacity: 0.75;
  }
  dl {
    display: grid;
    grid-template-columns: 7em minmax(0, 1fr);
    gap: 8px 10px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .page-size {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .page-size select {
    padding-block: 2px;
    padding-inline: 6px;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    text-align: start;
    overflow-wrap: anywhere;
    text-decoration: underline;
  }
  .permissions {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12em, 1fr));
    gap: 6px;
  }
  .permissions li span:first-child {
    display: inline-block;
    width: 1.2em;
    color: var(--accent);
    font-weight: 600;
  }
  .permissions li.no {
    color: var(--muted);
  }
  .permissions li.no span:first-child {
    color: var(--error-fg);
  }
  .loading {
    padding: 20px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 16px 20px;
  }
</style>
