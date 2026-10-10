<script lang="ts">
// Compress: a smaller copy of the document, at one of three levels.
import { save } from "@tauri-apps/plugin-dialog";
import type { CompressLevel } from "../bindings/CompressLevel";
import type { CompressReport } from "../bindings/CompressReport";
import { i18n } from "../i18n.svelte";
import { compressPdf, type RivetError, toRivetError } from "../pdf";
import type { Tab } from "../tabs.svelte";
import { folderOf, separatorOf } from "./files";
import ToolDialog from "./ToolDialog.svelte";

interface Props {
  tab: Tab;
  /** Done: `path` was written, or (when the file can't get smaller) nothing was. */
  onfinished: (report: CompressReport, path: string | null) => void;
  onerror: (e: RivetError) => void;
  oncancel: () => void;
}
let { tab, onfinished, onerror, oncancel }: Props = $props();

let level = $state<CompressLevel>("balanced");
let working = $state(false);

const levels = [
  ["lossless", "compress-lossless", "compress-lossless-hint"],
  ["balanced", "compress-balanced", "compress-balanced-hint"],
  ["strong", "compress-strong", "compress-strong-hint"],
] as const;

async function run() {
  if (working) return;
  const base = tab.fileName.replace(/\.pdf$/i, "");
  const target = await save({
    defaultPath: `${folderOf(tab.path)}${separatorOf(tab.path)}${i18n.t("compress-name", { name: base })}.pdf`,
    filters: [{ name: i18n.t("pdf-files"), extensions: ["pdf"] }],
  });
  if (!target) return;
  const path = /\.pdf$/i.test(target) ? target : `${target}.pdf`;
  working = true;
  try {
    const report = await compressPdf(tab.docId, level, path);
    onfinished(report, report.after < report.before ? path : null);
  } catch (e) {
    onerror(toRivetError(e));
  } finally {
    working = false;
  }
}
</script>

<ToolDialog title={i18n.t("compress-title")} icon="compress" oncancel={() => !working && oncancel()} onsubmit={run}
  progress={working ? 0.5 : null}>
  <div class="levels" role="radiogroup" aria-label={i18n.t("compress-title")}>
    {#each levels as [value, label, hint] (value)}
      <label class="level" class:chosen={level === value}>
        <input type="radio" name="compress-level" {value} bind:group={level} disabled={working} />
        <span>
          <strong>{i18n.t(label)}</strong>
          <span class="hint">{i18n.t(hint)}</span>
        </span>
      </label>
    {/each}
  </div>
  {#if working}<p class="hint">{i18n.t("compress-working")}</p>{/if}
  {#snippet actions()}
    <button type="button" onclick={oncancel} disabled={working}>{i18n.t("cancel")}</button>
    <button type="submit" class="primary" disabled={working}>{i18n.t("compress-run")}</button>
  {/snippet}
</ToolDialog>

<style>
  .levels {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .level {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .level.chosen {
    border-color: var(--accent);
  }
  .level input {
    margin-block-start: 3px;
    accent-color: var(--accent);
  }
  .level span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  p.hint {
    margin: 0;
  }
</style>
