<script lang="ts">
// Password protection: a password to open the file, and/or restrictions on
// printing, copying and editing (lifted by a separate permissions password).
// Applied when the file is saved; "Protect and save" does both.
import { onMount } from "svelte";
import type { Allowed } from "../bindings/Allowed";
import type { DocProperties } from "../bindings/DocProperties";
import type { Protection } from "../bindings/Protection";
import { i18n } from "../i18n.svelte";
import { documentProperties, type RivetError, setProtection, toRivetError, unlockOwner } from "../pdf";
import type { Tab } from "../tabs.svelte";
import ToolDialog from "./ToolDialog.svelte";

interface Props {
  tab: Tab;
  /** The protection is set; save the file to apply it. */
  ondone: () => void;
  onerror: (e: RivetError) => void;
  oncancel: () => void;
}
let { tab, ondone, onerror, oncancel }: Props = $props();

let facts = $state<DocProperties | null>(null);
async function load() {
  facts = await documentProperties(tab.docId).catch(() => null);
}
onMount(load);

// Unlocking a file someone else restricted (its owner password).
let ownerTry = $state("");
let ownerWrong = $state(false);
async function unlock() {
  try {
    ownerWrong = !(await unlockOwner(tab.docId, ownerTry));
    if (!ownerWrong) await load();
  } catch (e) {
    onerror(toRivetError(e));
  }
}

let needOpen = $state(true);
let open1 = $state("");
let open2 = $state("");
let restrict = $state(false);
let owner1 = $state("");
let owner2 = $state("");
let allowed = $state<Allowed>({ print: true, copy: false, edit: false, fillForms: true, annotate: false });

let problem = $derived.by((): string | null => {
  if (!needOpen && !restrict) return i18n.t("protect-choose-one");
  if (needOpen && !open1) return null;
  if (needOpen && open1 !== open2) return i18n.t("protect-mismatch");
  if (restrict && owner1 !== owner2) return i18n.t("protect-mismatch");
  if (restrict && needOpen && owner1 && owner1 === open1) return i18n.t("protect-same");
  return null;
});
let ready = $derived(problem === null && (!needOpen || open1.length > 0) && (!restrict || owner1.length > 0));
let weak = $derived(needOpen && open1.length > 0 && open1.length < 8);

async function apply(protection: Protection) {
  try {
    await setProtection(tab.docId, protection);
    tab.dirty = true;
    ondone();
  } catch (e) {
    onerror(toRivetError(e));
  }
}

function protect() {
  if (!ready) return;
  const all: Allowed = { print: true, copy: true, edit: true, fillForms: true, annotate: true };
  apply({
    kind: "set",
    openPassword: needOpen ? open1 : "",
    ownerPassword: restrict ? owner1 : "",
    allowed: restrict ? allowed : all,
  });
}

/** Focuses a field as it appears (they show once the document's facts are read). */
function focus(node: HTMLInputElement) {
  node.focus();
}

const choices = [
  ["print", "protect-allow-print"],
  ["copy", "protect-allow-copy"],
  ["edit", "protect-allow-edit"],
  ["fillForms", "protect-allow-forms"],
  ["annotate", "protect-allow-comments"],
] as const;
</script>

<ToolDialog title={i18n.t("protect-title")} icon="lock" {oncancel} onsubmit={protect}>
  {#if facts === null}
    <p class="hint">…</p>
  {:else if facts.encrypted && !facts.canChangeProtection}
    <p>{i18n.t("protect-locked")}</p>
    <label class="field">
      {i18n.t("protect-owner-password")}
      <input type="password" bind:value={ownerTry} use:focus aria-invalid={ownerWrong}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            unlock();
          }
        }} />
    </label>
    {#if ownerWrong}<p class="error">{i18n.t("error-wrong-password")}</p>{/if}
  {:else}
    {#if facts.encrypted}
      <p class="hint">{facts.needsOpenPassword ? i18n.t("protect-now-password") : i18n.t("protect-now-restricted")}</p>
    {/if}
    <label class="check">
      <input type="checkbox" bind:checked={needOpen} />
      {i18n.t("protect-need-open")}
    </label>
    {#if needOpen}
      <div class="sub">
        <input type="password" bind:value={open1} use:focus placeholder={i18n.t("password")} aria-label={i18n.t("password")} />
        <input type="password" bind:value={open2} placeholder={i18n.t("protect-repeat")} aria-label={i18n.t("protect-repeat")}
          aria-invalid={open2.length > 0 && open1 !== open2} />
        {#if weak}<span class="hint">{i18n.t("protect-weak")}</span>{/if}
      </div>
    {/if}
    <label class="check">
      <input type="checkbox" bind:checked={restrict} />
      {i18n.t("protect-restrict")}
    </label>
    {#if restrict}
      <div class="sub">
        <input type="password" bind:value={owner1} placeholder={i18n.t("protect-owner-password")}
          aria-label={i18n.t("protect-owner-password")} />
        <input type="password" bind:value={owner2} placeholder={i18n.t("protect-repeat")} aria-label={i18n.t("protect-repeat")}
          aria-invalid={owner2.length > 0 && owner1 !== owner2} />
        <span class="hint">{i18n.t("protect-owner-hint")}</span>
        {#each choices as [key, label] (key)}
          <label class="check">
            <input type="checkbox" bind:checked={allowed[key]} />
            {i18n.t(label)}
          </label>
        {/each}
      </div>
    {/if}
    {#if problem}<p class="error">{problem}</p>{/if}
    <p class="hint">{i18n.t("protect-keep-safe")}</p>
  {/if}
  {#snippet actions()}
    {#if facts?.encrypted && facts.canChangeProtection}
      <button type="button" class="remove" onclick={() => apply({ kind: "remove" })}>{i18n.t("protect-remove")}</button>
    {/if}
    <button type="button" onclick={oncancel}>{i18n.t("cancel")}</button>
    {#if facts?.encrypted && !facts.canChangeProtection}
      <button type="button" class="primary" onclick={unlock} disabled={!ownerTry}>{i18n.t("merge-unlock")}</button>
    {:else}
      <button type="submit" class="primary" disabled={!ready}>{i18n.t("protect-run")}</button>
    {/if}
  {/snippet}
</ToolDialog>

<style>
  .sub {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-inline-start: 26px;
  }
  p {
    margin: 0;
  }
  .remove {
    margin-inline-end: auto;
  }
</style>
