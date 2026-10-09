<script lang="ts">
// The Annotate toolbar: a second row under the main toolbar with the
// annotation tools, the current tool's colour, width and opacity, and undo/redo.
import type { ComponentProps } from "svelte";
import { type AnnotTool, annotate, changeText, PALETTE, redo, toHex, undo, update } from "./annotate.svelte";
import type { Color } from "./bindings/Color";
import type { TextStyle } from "./bindings/TextStyle";
import FontPicker from "./FontPicker.svelte";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { fade, out } from "./motion";
import { type RivetError, toRivetError } from "./pdf";
import SignatureDialog from "./SignatureDialog.svelte";
import SizeField from "./SizeField.svelte";
import { settings } from "./settings.svelte";
import { type Signature, signatures } from "./signatures.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  onerror: (e: RivetError) => void;
  /** Applies the marked redactions after asking (they can't be undone). */
  onapplyredactions: () => void;
}
let { tab, onerror, onapplyredactions }: Props = $props();

type ToolButton = { tool: AnnotTool; icon: ComponentProps<typeof Icon>["name"]; label: string };
const GROUPS: ToolButton[][] = [
  [
    { tool: "highlight", icon: "highlight", label: "annot-highlight" },
    { tool: "underline", icon: "underline", label: "annot-underline" },
    { tool: "strikeout", icon: "strikeout", label: "annot-strikeout" },
  ],
  [
    { tool: "pen", icon: "pen", label: "annot-pen" },
    { tool: "rectangle", icon: "rectangle", label: "annot-rectangle" },
    { tool: "ellipse", icon: "ellipse", label: "annot-ellipse" },
    { tool: "line", icon: "line", label: "annot-line" },
    { tool: "arrow", icon: "arrow", label: "annot-arrow" },
  ],
  [
    { tool: "text", icon: "text", label: "annot-text" },
    { tool: "note", icon: "note", label: "annot-note" },
    { tool: "eraser", icon: "eraser", label: "annot-eraser" },
  ],
  [{ tool: "redact", icon: "redact", label: "annot-redact" }],
];

function choose(t: AnnotTool | null) {
  annotate.tool = annotate.tool === t ? null : t;
  // Drawing needs the Select mouse mode (Hand drags the page instead).
  if (t) settings.tool = "select";
  tab.selectedAnnotation = null;
}

let current = $derived(annotate.tool);

// Signatures: a menu of saved ones, and a dialog to make a new one.
let signatureMenu: HTMLDivElement | undefined = $state();
let signatureButton: HTMLButtonElement | undefined = $state();

/** Opens the saved signatures under their button (mirrored in right-to-left layouts). */
function showSignatureMenu() {
  if (!signatureMenu || !signatureButton) return;
  signatureMenu.showPopover();
  const button = signatureButton.getBoundingClientRect();
  const width = signatureMenu.offsetWidth;
  const rtl = document.documentElement.dir === "rtl";
  const left = rtl ? button.right - width : button.left;
  signatureMenu.style.left = `${Math.max(8, Math.min(left, innerWidth - width - 8))}px`;
  signatureMenu.style.top = `${button.bottom + 6}px`;
}
let makingSignature = $state(false);

function openSignatures() {
  if (annotate.tool === "signature") {
    choose(null);
    return;
  }
  signatures.load().then(() => {
    if (signatures.list.length === 0) makingSignature = true;
    else showSignatureMenu();
  });
}

function useSignature(sig: Signature) {
  signatureMenu?.hidePopover();
  settings.tool = "select";
  tab.selectedAnnotation = null;
  annotate.place(sig);
}

/** An SVG path for a drawn signature's preview. */
function inkPath(sig: Extract<Signature, { kind: "ink" }>) {
  return sig.strokes.map((s) => `M${s.map((p) => `${p.x} ${p.y}`).join("L")}`).join(" ");
}
let style = $derived(
  current && !["eraser", "redact", "signature", "text"].includes(current) ? annotate.style(current) : null,
);
let marked = $derived(tab.redactions.length);
let hasWidth = $derived(current !== null && ["pen", "rectangle", "ellipse", "line", "arrow"].includes(current));
let hasFill = $derived(current === "rectangle" || current === "ellipse");
let hasOpacity = $derived(current !== null && current !== "note" && current !== "eraser");

function setColor(color: Color) {
  if (current) annotate.setStyle(current, { color });
}

function fromHex(value: string): Color {
  return {
    r: Number.parseInt(value.slice(1, 3), 16),
    g: Number.parseInt(value.slice(3, 5), 16),
    b: Number.parseInt(value.slice(5, 7), 16),
  };
}

// --- Text boxes: their style controls ---------------------------------------
// They change the box being written, or the selected box, and become the
// style of new boxes (like the other tools remember theirs).

let editingText = $derived(annotate.textEdit?.tab === tab ? annotate.textEdit : null);
let selectedText = $derived.by(() => {
  const sel = tab.selectedAnnotation;
  const a = sel ? tab.annotations.get(sel.page)?.find((x) => x.id === sel.id) : undefined;
  return sel && a?.kind.kind === "freeText" && a.editable ? { page: sel.page, a } : null;
});
let textTarget = $derived.by((): { style: TextStyle; color: Color } | null => {
  if (editingText) return { style: editingText.style, color: editingText.color };
  if (selectedText?.a.kind.kind === "freeText")
    return { style: selectedText.a.kind.style, color: selectedText.a.color };
  if (current === "text") return { style: settings.textStyle, color: annotate.style("text").color };
  return null;
});

function applyText(change: Partial<TextStyle>, color?: Color) {
  settings.textStyle = { ...settings.textStyle, ...change };
  if (color) annotate.setStyle("text", { color });
  if (editingText) {
    changeText({ style: { ...editingText.style, ...change }, color: color ?? editingText.color });
    // Back to writing.
    requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>(".text-box textarea")?.focus());
  } else if (selectedText) {
    const { page, a } = selectedText;
    if (a.kind.kind !== "freeText") return;
    const kind = { ...a.kind, style: { ...a.kind.style, ...change } };
    run((t) => update(t, page, a, { ...a, kind, color: color ?? a.color }));
  }
}

const ALIGNS = [
  { value: "auto", icon: "align-auto", label: "text-align-auto" },
  { value: "left", icon: "align-left", label: "text-align-left" },
  { value: "center", icon: "align-center", label: "text-align-center" },
  { value: "right", icon: "align-right", label: "text-align-right" },
] as const;
const VALIGNS = [
  { value: "top", icon: "valign-top", label: "text-valign-top" },
  { value: "middle", icon: "valign-middle", label: "text-valign-middle" },
  { value: "bottom", icon: "valign-bottom", label: "text-valign-bottom" },
] as const;
const DIRECTIONS = [
  { value: "auto", icon: "dir-auto", label: "text-dir-auto" },
  { value: "rtl", icon: "dir-rtl", label: "text-dir-rtl" },
  { value: "ltr", icon: "dir-ltr", label: "text-dir-ltr" },
] as const;

async function run(action: (t: Tab) => Promise<unknown>) {
  try {
    await action(tab);
  } catch (e) {
    onerror(toRivetError(e));
  }
}
</script>

<div class="annotate-bar" in:fade|global out:out|global role="toolbar" aria-label={i18n.t("annotate")}>
  <div class="group" role="radiogroup" aria-label={i18n.t("annotate-tools")}>
    <button class="icon" role="radio" aria-checked={current === null} onclick={() => choose(null)}
      title={i18n.t("annot-select")} aria-label={i18n.t("annot-select")} data-shortcut="Escape">
      <Icon name="pointer" />
    </button>
    {#each GROUPS as group, g (g)}
      <span class="sep" aria-hidden="true"></span>
      {#each group as b (b.tool)}
        <button class="icon" role="radio" aria-checked={current === b.tool} onclick={() => choose(b.tool)}
          title={i18n.t(b.label)} aria-label={i18n.t(b.label)}>
          <Icon name={b.icon} />
        </button>
      {/each}
    {/each}
  </div>

  <div class="group">
    <button class="icon" bind:this={signatureButton} aria-pressed={current === "signature"} onclick={openSignatures}
      title={i18n.t("signature")} aria-label={i18n.t("signature")} aria-haspopup="menu">
      <Icon name="signature" />
    </button>
    <div class="signature-menu" popover bind:this={signatureMenu} role="menu" aria-label={i18n.t("signature")}>
      {#each signatures.list as sig (sig.id)}
        <div class="saved">
          <button class="use" role="menuitem" onclick={() => useSignature(sig)} aria-label={i18n.t("signature-use")}>
            {#if sig.kind === "image"}
              <img src={sig.png} alt="" />
            {:else}
              <svg viewBox="0 0 {sig.width} {sig.height}" aria-hidden="true">
                <path d={inkPath(sig)} stroke="rgb({sig.color.r} {sig.color.g} {sig.color.b})"
                  stroke-width={sig.lineWidth} />
              </svg>
            {/if}
          </button>
          <button class="icon remove" onclick={() => signatures.remove(sig.id)} title={i18n.t("signature-delete")}
            aria-label={i18n.t("signature-delete")}>
            <Icon name="trash" />
          </button>
        </div>
      {/each}
      <button class="new" role="menuitem" onclick={() => {
        signatureMenu?.hidePopover();
        makingSignature = true;
      }}>
        <Icon name="plus" />
        {i18n.t("signature-new")}
      </button>
    </div>
  </div>

  {#if textTarget}
    {@const ts = textTarget.style}
    <div class="group style text-style" aria-label={i18n.t("annot-style")}>
      <FontPicker value={ts.font} onchange={(font) => applyText({ font })} />
      <SizeField value={ts.size} onchange={(size) => applyText({ size })} />
      <button class="icon" aria-pressed={ts.bold} onclick={() => applyText({ bold: !ts.bold })}
        title={i18n.t("text-bold")} aria-label={i18n.t("text-bold")}>
        <Icon name="bold" />
      </button>
      <span class="sep" aria-hidden="true"></span>
      <div class="segmented" role="radiogroup" aria-label={i18n.t("text-align")}>
        {#each ALIGNS as o (o.value)}
          <button class="icon" role="radio" aria-checked={ts.align === o.value} onclick={() => applyText({ align: o.value })}
            title={i18n.t(o.label)} aria-label={i18n.t(o.label)}>
            <Icon name={o.icon} />
          </button>
        {/each}
      </div>
      <div class="segmented" role="radiogroup" aria-label={i18n.t("text-valign")}>
        {#each VALIGNS as o (o.value)}
          <button class="icon" role="radio" aria-checked={ts.valign === o.value} onclick={() => applyText({ valign: o.value })}
            title={i18n.t(o.label)} aria-label={i18n.t(o.label)}>
            <Icon name={o.icon} />
          </button>
        {/each}
      </div>
      <div class="segmented" role="radiogroup" aria-label={i18n.t("text-dir")}>
        {#each DIRECTIONS as o (o.value)}
          <button class="icon" role="radio" aria-checked={ts.direction === o.value}
            onclick={() => applyText({ direction: o.value })} title={i18n.t(o.label)} aria-label={i18n.t(o.label)}>
            <Icon name={o.icon} />
          </button>
        {/each}
      </div>
      <span class="sep" aria-hidden="true"></span>
      {#each PALETTE as color (toHex(color))}
        <button class="swatch" style:background={toHex(color)} aria-pressed={toHex(color) === toHex(textTarget.color)}
          aria-label={toHex(color)} onclick={() => applyText({}, color)}></button>
      {/each}
      <label class="custom" title={i18n.t("annot-custom-color")}>
        <input type="color" value={toHex(textTarget.color)} aria-label={i18n.t("annot-custom-color")}
          onchange={(e) => applyText({}, fromHex(e.currentTarget.value))} />
      </label>
    </div>
  {:else if style && current}
    <div class="group style" aria-label={i18n.t("annot-style")}>
      {#each PALETTE as color (toHex(color))}
        <button class="swatch" style:background={toHex(color)} aria-pressed={toHex(color) === toHex(style.color)}
          aria-label={toHex(color)} onclick={() => setColor(color)}></button>
      {/each}
      <label class="custom" title={i18n.t("annot-custom-color")}>
        <input type="color" value={toHex(style.color)} aria-label={i18n.t("annot-custom-color")}
          onchange={(e) => setColor(fromHex(e.currentTarget.value))} />
      </label>
      {#if hasWidth}
        <label class="slider" title={i18n.t("annot-width")}>
          <span>{i18n.t("annot-width")}</span>
          <input type="range" min="0.5" max="12" step="0.5" value={style.width}
            oninput={(e) => current && annotate.setStyle(current, { width: Number(e.currentTarget.value) })} />
        </label>
      {/if}
      {#if hasOpacity}
        <label class="slider" title={i18n.t("annot-opacity")}>
          <span>{i18n.t("annot-opacity")}</span>
          <input type="range" min="0.1" max="1" step="0.05" value={style.opacity}
            oninput={(e) => current && annotate.setStyle(current, { opacity: Number(e.currentTarget.value) })} />
        </label>
      {/if}
      {#if hasFill}
        <label class="check">
          <input type="checkbox" checked={style.fill}
            onchange={(e) => current && annotate.setStyle(current, { fill: e.currentTarget.checked })} />
          {i18n.t("annot-fill")}
        </label>
      {/if}
    </div>
  {:else if current === "redact" || marked > 0}
    <div class="group redact-info">
      <p class="hint">{i18n.t("redact-hint")}</p>
      {#if marked > 0}
        <button class="danger" onclick={onapplyredactions}>
          {i18n.t("redact-apply", { count: marked })}
        </button>
      {/if}
    </div>
  {:else if current === null}
    <p class="hint">{i18n.t("annot-hint")}</p>
  {/if}

  <div class="group end">
    <button class="icon" disabled={!tab.history.canUndo} onclick={() => run(undo)}
      title={i18n.t("undo")} aria-label={i18n.t("undo")} data-shortcut="Ctrl+Z">
      <Icon name="undo" />
    </button>
    <button class="icon" disabled={!tab.history.canRedo} onclick={() => run(redo)}
      title={i18n.t("redo")} aria-label={i18n.t("redo")} data-shortcut="Ctrl+Y">
      <Icon name="redo" />
    </button>
    <span class="sep" aria-hidden="true"></span>
    <button class="icon" onclick={() => (annotate.open = false)} title={i18n.t("annotate-close")}
      aria-label={i18n.t("annotate-close")}>
      <Icon name="close" />
    </button>
  </div>
</div>

{#if makingSignature}
  <SignatureDialog
    onuse={(sig, keep) => {
      makingSignature = false;
      if (keep) signatures.add(sig);
      useSignature(sig);
    }}
    oncancel={() => (makingSignature = false)}
  />
{/if}

<style>
  .annotate-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding-block: 4px;
    padding-inline: 8px;
    background: var(--surface);
    border-block-end: 1px solid var(--border);
    /* Long rows (a text box's controls) continue on a second line. */
    flex-wrap: wrap;
  }
  .segmented {
    display: flex;
    gap: 1px;
  }
  .text-style .icon {
    width: 30px;
    height: 30px;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .group.end {
    margin-inline-start: auto;
  }
  .icon[aria-checked="true"],
  .icon[aria-pressed="true"] {
    background: var(--accent);
    color: var(--accent-text);
    border-color: var(--accent);
  }
  .sep {
    width: 1px;
    height: 22px;
    margin-inline: 6px;
    background: var(--border);
  }
  .style {
    gap: 6px;
  }
  .swatch {
    width: 22px;
    height: 22px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid rgb(0 0 0 / 0.2);
  }
  .swatch[aria-pressed="true"] {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .custom input {
    width: 26px;
    height: 26px;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
  }
  .slider,
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-inline-start: 8px;
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
  }
  .slider input {
    width: 80px;
    accent-color: var(--accent);
  }
  .check input {
    accent-color: var(--accent);
  }
  .signature-menu {
    position: fixed;
    margin: 0;
    inset: auto;
    width: 280px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.18);
  }
  .saved {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-block-end: 6px;
  }
  /* Signature previews sit on white, like paper. */
  .use {
    flex: 1;
    display: grid;
    place-items: center;
    height: 64px;
    padding: 6px;
    background: #ffffff;
  }
  .use img,
  .use svg {
    max-width: 100%;
    max-height: 100%;
  }
  .use svg {
    width: 100%;
    height: 100%;
    fill: none;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .new {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
  }
  .redact-info {
    gap: 10px;
  }
  /* Applying redactions can't be undone: the button says so with its colour. */
  .danger {
    border-color: #d32f2f;
    background: #d32f2f;
    color: #ffffff;
    white-space: nowrap;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
</style>
