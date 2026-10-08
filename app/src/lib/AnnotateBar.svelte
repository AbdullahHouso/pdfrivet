<script lang="ts">
// The Annotate toolbar: a second row under the main toolbar with the
// annotation tools, the current tool's colour, width and opacity, and undo/redo.
import type { ComponentProps } from "svelte";
import { type AnnotTool, annotate, PALETTE, redo, toHex, undo } from "./annotate.svelte";
import type { Color } from "./bindings/Color";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { type RivetError, toRivetError } from "./pdf";
import { settings } from "./settings.svelte";
import type { Tab } from "./tabs.svelte";

interface Props {
  tab: Tab;
  onerror: (e: RivetError) => void;
}
let { tab, onerror }: Props = $props();

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
    { tool: "note", icon: "note", label: "annot-note" },
    { tool: "eraser", icon: "eraser", label: "annot-eraser" },
  ],
];

function choose(t: AnnotTool | null) {
  annotate.tool = annotate.tool === t ? null : t;
  // Drawing needs the Select mouse mode (Hand drags the page instead).
  if (t) settings.tool = "select";
  tab.selectedAnnotation = null;
}

let current = $derived(annotate.tool);
let style = $derived(current && current !== "eraser" ? annotate.style(current) : null);
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

async function run(action: (t: Tab) => Promise<unknown>) {
  try {
    await action(tab);
  } catch (e) {
    onerror(toRivetError(e));
  }
}
</script>

<div class="annotate-bar" role="toolbar" aria-label={i18n.t("annotate")}>
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

  {#if style && current}
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
    overflow-x: auto;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .group.end {
    margin-inline-start: auto;
  }
  .icon[aria-checked="true"] {
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
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
</style>
