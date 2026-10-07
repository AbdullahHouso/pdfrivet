<script lang="ts">
// Makes a page's form fields interactive. PDFium draws the fields; this layer
// puts invisible controls on top of them. Clicking a checkbox or radio button
// toggles it, choosing an option changes a drop-down, and clicking a text field
// opens a real text box (so typing Arabic, IME input and spell-check all work).
// After each change the page re-renders with PDFium's updated appearance.
import type { FieldChange } from "./bindings/FieldChange";
import type { FormField } from "./bindings/FormField";
import { i18n } from "./i18n.svelte";
import { type Degrees, rotateRect } from "./layout";
import { changeField, getFormFields, type RivetError, toRivetError } from "./pdf";

interface Props {
  docId: number;
  index: number;
  rotation: Degrees;
  /** Height of the page on screen (px), to size text boxes. */
  pageHeight: number;
  /** Changes whenever the document changed, so values are re-read. */
  revision: number;
  onchanged: () => void;
  onerror?: (e: RivetError) => void;
}
let { docId, index, rotation, pageHeight, revision, onchanged, onerror }: Props = $props();

let fields = $state<FormField[]>([]);
let editing = $state<FormField | null>(null);
let draft = $state("");

$effect(() => {
  void revision;
  let cancelled = false;
  getFormFields(docId, index)
    .then((found) => {
      if (!cancelled) fields = found.filter((f) => !f.readOnly && f.field.kind !== "other");
    })
    .catch(() => {});
  return () => {
    cancelled = true;
  };
});

async function apply(field: FormField, change: FieldChange) {
  try {
    await changeField(docId, index, field.index, change);
    onchanged();
  } catch (e) {
    onerror?.(toRivetError(e));
  }
}

function startEditing(field: FormField) {
  if (field.field.kind !== "text") return;
  editing = field;
  draft = field.field.value;
}

function finishEditing(save: boolean) {
  const field = editing;
  editing = null;
  if (save && field?.field.kind === "text" && draft !== field.field.value) {
    apply(field, { kind: "text", value: draft });
  }
}

function label(field: FormField): string {
  return field.name ?? i18n.t("form-field");
}

// Text boxes are sized from the field's height on screen.
function fontSize(field: FormField): number {
  const h = (field.bottom - field.top) * pageHeight;
  return Math.max(10, Math.min(18, h * 0.6));
}

function focusOnMount(node: HTMLElement) {
  node.focus();
  if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) node.select();
}
</script>

{#each fields as field (field.index)}
  {@const r = rotateRect(field, rotation)}
  {@const style = `left:${r.left * 100}%;top:${r.top * 100}%;width:${(r.right - r.left) * 100}%;height:${(r.bottom - r.top) * 100}%`}
  {#if field.field.kind === "checkbox" || field.field.kind === "radio"}
    <button
      class="field"
      {style}
      role={field.field.kind}
      aria-checked={field.field.checked}
      aria-label={label(field)}
      title={label(field)}
      onclick={() => apply(field, { kind: "toggle" })}
    ></button>
  {:else if field.field.kind === "choice"}
    <select
      class="field choice"
      {style}
      aria-label={label(field)}
      title={label(field)}
      value={field.field.selected ?? -1}
      onchange={(e) => apply(field, { kind: "select", option: Number(e.currentTarget.value) })}
    >
      {#each field.field.options as option, i (i)}
        <option value={i}>{option}</option>
      {/each}
    </select>
  {:else if field.field.kind === "text"}
    {#if editing?.index === field.index}
      {#if field.field.multiline}
        <textarea
          class="editor"
          style="{style};font-size:{fontSize(field)}px"
          dir="auto"
          aria-label={label(field)}
          bind:value={draft}
          use:focusOnMount
          onblur={() => finishEditing(true)}
          onkeydown={(e) => e.key === "Escape" && finishEditing(false)}
        ></textarea>
      {:else}
        <input
          class="editor"
          style="{style};font-size:{fontSize(field)}px"
          type={field.field.password ? "password" : "text"}
          dir="auto"
          aria-label={label(field)}
          bind:value={draft}
          use:focusOnMount
          onblur={() => finishEditing(true)}
          onkeydown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") finishEditing(false);
          }}
        />
      {/if}
    {:else}
      <button
        class="field text"
        {style}
        aria-label={label(field)}
        title={label(field)}
        onclick={() => startEditing(field)}
      ></button>
    {/if}
  {/if}
{/each}

<style>
  .field {
    position: absolute;
    padding: 0;
    margin: 0;
    border: none;
    border-radius: 2px;
    background: transparent;
    cursor: pointer;
  }
  .field.text {
    cursor: text;
  }
  .field:hover,
  .field:focus-visible {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    outline: 1px solid color-mix(in srgb, var(--accent) 60%, transparent);
  }
  /* Invisible, but still opens the system's option list when clicked. */
  .choice {
    opacity: 0;
    appearance: none;
  }
  .choice:focus-visible {
    opacity: 0.0001;
  }
  .editor {
    position: absolute;
    margin: 0;
    padding-block: 0;
    padding-inline: 4px;
    border: 2px solid var(--accent);
    border-radius: 2px;
    background: white;
    color: black;
    font-family: system-ui, sans-serif;
    resize: none;
  }
</style>
