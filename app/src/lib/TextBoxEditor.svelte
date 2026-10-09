<script lang="ts">
// A text box on the page as it will look: the same font, size, line spacing,
// padding, direction and positions the engine draws with (see textBox.ts).
// Editable while writing; otherwise a preview (while a box is moved or resized).

import { toHex } from "./annotate.svelte";
import type { Color } from "./bindings/Color";
import type { TextStyle } from "./bindings/TextStyle";
import { cssFamily, LINE_HEIGHT, registerBundledFonts, TEXT_PADDING } from "./textBox";

interface Props {
  text: string;
  style: TextStyle;
  color: Color;
  /** Screen px per PDF point. */
  scale: number;
  /** The box's top corner on its start side, in px on the page. */
  x: number;
  y: number;
  /** The box grows leftwards from (x, y) (right-to-left text). */
  growLeft: boolean;
  editable?: boolean;
  /** Being saved: shown, but no longer editable. */
  saving?: boolean;
  placeholder?: string;
  oninput?: (text: string) => void;
  onkeydown?: (e: KeyboardEvent) => void;
}
let {
  text,
  style,
  color,
  scale,
  x,
  y,
  growLeft,
  editable = false,
  saving = false,
  placeholder = "",
  oninput,
  onkeydown,
}: Props = $props();

registerBundledFonts();

let fontSize = $derived(style.size * scale);
let pad = $derived(TEXT_PADDING * scale);
// A hidden copy of the text measures the size it needs.
let measuredWidth = $state(0);
let measuredHeight = $state(0);
let boxWidth = $derived(style.width !== null ? (style.width + 2 * TEXT_PADDING) * scale : measuredWidth + 2 * pad + 2);
let textHeight = $derived(Math.max(measuredHeight, fontSize * LINE_HEIGHT));
let boxHeight = $derived(
  Math.max(textHeight + 2 * pad, style.height !== null ? (style.height + 2 * TEXT_PADDING) * scale : 0),
);
let left = $derived(growLeft ? x - boxWidth : x);
let dir = $derived<"auto" | "rtl" | "ltr">(style.direction);
let align = $derived(style.align === "auto" ? "start" : style.align);
let justify = $derived(style.valign === "top" ? "flex-start" : style.valign === "middle" ? "center" : "flex-end");

function focus(el: HTMLTextAreaElement) {
  el.focus({ preventScroll: true });
  el.setSelectionRange(el.value.length, el.value.length);
}
</script>

<div
  class="text-box"
  class:editable
  style:left="{left}px"
  style:top="{y}px"
  style:width="{boxWidth}px"
  style:height="{boxHeight}px"
  style:padding="{pad}px"
  style:justify-content={justify}
  style:--font={cssFamily(style.font)}
  style:--size="{fontSize}px"
  style:--weight={style.bold ? 700 : 400}
  style:--color={toHex(color)}
  style:--align={align}
  style:--line={LINE_HEIGHT}
  style:--bidi={style.direction === "auto" ? "plaintext" : "isolate"}
>
  <div
    class="measure text"
    {dir}
    style:width={style.width !== null ? `${style.width * scale}px` : "max-content"}
    style:white-space={style.width !== null ? "pre-wrap" : "pre"}
    bind:clientWidth={measuredWidth}
    bind:clientHeight={measuredHeight}
    aria-hidden="true"
  >{text}&#8203;</div>
  {#if editable}
    <textarea
      class="text"
      {dir}
      value={text}
      {placeholder}
      readonly={saving}
      spellcheck="false"
      style:height="{textHeight}px"
      style:white-space={style.width !== null ? "pre-wrap" : "pre"}
      {@attach focus}
      oninput={(e) => oninput?.(e.currentTarget.value)}
      {onkeydown}
    ></textarea>
  {:else}
    <div class="text" {dir} style:white-space={style.width !== null ? "pre-wrap" : "pre"}>{text}</div>
  {/if}
</div>

<style>
  .text-box {
    position: absolute;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    pointer-events: none;
  }
  .text-box.editable {
    outline: 1.5px dashed var(--accent);
    outline-offset: 0;
    pointer-events: auto;
  }
  .text {
    margin: 0;
    padding: 0;
    border: none;
    font-family: var(--font);
    font-size: var(--size);
    font-weight: var(--weight);
    line-height: var(--line);
    color: var(--color);
    text-align: var(--align);
    unicode-bidi: var(--bidi);
    overflow-wrap: anywhere;
    background: transparent;
  }
  textarea.text {
    width: 100%;
    resize: none;
    overflow: hidden;
    outline: none;
    caret-color: var(--color);
  }
  textarea.text::placeholder {
    color: color-mix(in srgb, var(--color) 45%, transparent);
  }
  .measure {
    position: absolute;
    visibility: hidden;
    inset-inline-start: 0;
    top: 0;
  }
</style>
