<script lang="ts">
// One annotation in the comments panel: what it is (a small picture of a
// drawing or shape, or the text a highlight marks), its comment, and the
// replies to it. Comments and replies are written and changed in place.

import { toHex } from "./annotate.svelte";
import type { Annotation } from "./bindings/Annotation";
import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
import { commentKind, markedText, type Thread } from "./comments.svelte";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { loadPageText } from "./pageText";

interface Props {
  thread: Thread;
  docId: number;
  /** The page's width divided by its height (to draw shapes in proportion). */
  aspect: number;
  selected: boolean;
  /** Can write comments and replies. */
  canEdit: boolean;
  canReply: boolean;
  onselect: () => void;
  oncomment: (a: Annotation, text: string) => Promise<unknown>;
  onreply: (text: string) => Promise<unknown>;
  ondelete: (a: Annotation) => void;
}
let { thread, docId, aspect, selected, canEdit, canReply, onselect, oncomment, onreply, ondelete }: Props = $props();

let a = $derived(thread.annotation);
let kind = $derived(commentKind(a));
let color = $derived(toHex(a.color));
/** What is being written: the comment, a reply (by id), or a new reply. */
let editing = $state<string | null>(null);
let draft = $state("");
let busy = $state(false);
let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
let card: HTMLElement;

// The text a highlight, underline or strikeout marks.
let quote = $state("");
$effect(() => {
  const k = a.kind;
  if (k.kind !== "markup") {
    quote = "";
    return;
  }
  let cancelled = false;
  loadPageText(docId, thread.page)
    .then((text) => {
      if (!cancelled) quote = markedText(text, k.quads);
    })
    .catch(() => {});
  return () => {
    cancelled = true;
  };
});

// Shown when selected on the page.
$effect(() => {
  if (selected) card.scrollIntoView({ block: "nearest" });
});

const ICONS = {
  highlight: "highlight",
  underline: "underline",
  strikeout: "strikeout",
  ink: "pen",
  shape: "rectangle",
  note: "note",
  stamp: "signature",
  other: "comment",
} as const;

/** A drawing or shape, redrawn small inside its own box. */
let preview = $derived.by(() => {
  const r = a.rect;
  const w = Math.max((r.right - r.left) * aspect, 1e-6);
  const h = Math.max(r.bottom - r.top, 1e-6);
  const scale = 40 / Math.max(w, h);
  const x = (v: number) => ((v - r.left) * aspect * scale).toFixed(1);
  const y = (v: number) => ((v - r.top) * scale).toFixed(1);
  const k = a.kind;
  const box = { width: w * scale, height: h * scale };
  if (k.kind === "ink") {
    return { ...box, paths: k.strokes.map((s) => s.map((p, i) => `${i ? "L" : "M"}${x(p.x)} ${y(p.y)}`).join("")) };
  }
  if (k.kind === "line") return { ...box, paths: [`M${x(k.from.x)} ${y(k.from.y)}L${x(k.to.x)} ${y(k.to.y)}`] };
  if (k.kind === "square") return { ...box, paths: [`M0 0H${box.width}V${box.height}H0Z`] };
  if (k.kind === "circle") {
    const rx = box.width / 2;
    const ry = box.height / 2;
    return { ...box, paths: [`M0 ${ry}A${rx} ${ry} 0 1 0 ${box.width} ${ry}A${rx} ${ry} 0 1 0 0 ${ry}Z`] };
  }
  return null;
});

function relative(iso: string | null): string {
  if (!iso) return "";
  const then = Date.parse(iso);
  if (!then) return "";
  const seconds = (then - Date.now()) / 1000;
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ["year", 31536000],
    ["month", 2592000],
    ["week", 604800],
    ["day", 86400],
    ["hour", 3600],
    ["minute", 60],
  ];
  const format = new Intl.RelativeTimeFormat(i18n.locale, { numeric: "auto" });
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) return format.format(Math.round(seconds / size), unit);
  }
  return i18n.t("just-now");
}

function exact(iso: string | null): string {
  return iso ? new Date(iso).toLocaleString(i18n.locale) : "";
}

function start(what: string, text: string) {
  editing = what;
  draft = text;
}

async function finish(save: boolean) {
  const what = editing;
  const text = draft.trim();
  if (!what || busy) return;
  if (!save) {
    editing = null;
    return;
  }
  busy = true;
  try {
    if (what === "new-reply") {
      if (text) await onreply(text);
    } else {
      const target = what === "comment" ? a : thread.replies.find((r) => r.id === what);
      if (target && target.contents !== text) await oncomment(target, text);
    }
    editing = null;
  } finally {
    busy = false;
  }
}

function onkey(e: KeyboardEvent) {
  e.stopPropagation();
  if (e.key === "Escape") finish(false);
  else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) finish(true);
}

function focusField(field: HTMLTextAreaElement) {
  field.focus();
  field.setSelectionRange(field.value.length, field.value.length);
}

function showMenu(e: MouseEvent, target: Annotation) {
  e.preventDefault();
  e.stopPropagation();
  const isReply = target !== a;
  const items: MenuItem[] = [];
  if (canEdit && target.editable) {
    items.push({
      label: i18n.t(isReply ? "reply-edit" : "comment-edit"),
      action: () => start(isReply ? target.id : "comment", target.contents),
    });
  }
  if (!isReply && canReply) items.push({ label: i18n.t("reply"), action: () => start("new-reply", "") });
  items.push({ label: i18n.t(isReply ? "reply-delete" : "annot-delete"), action: () => ondelete(target) });
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
  menu = {
    x: e.type === "contextmenu" ? e.clientX : r.left,
    y: e.type === "contextmenu" ? e.clientY : r.bottom,
    items,
  };
}

/** A click on the card (not on its buttons or fields) shows the annotation. */
function onclick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest("button, textarea, a")) return;
  onselect();
}
</script>

{#snippet editor(placeholder: string)}
  <div class="editor">
    <textarea bind:value={draft} dir="auto" rows="3" {placeholder} use:focusField onkeydown={onkey} disabled={busy}></textarea>
    <div class="actions">
      <button onclick={() => finish(false)} disabled={busy}>{i18n.t("cancel")}</button>
      <button class="primary" onclick={() => finish(true)} disabled={busy} title="Ctrl+Enter">{i18n.t("save")}</button>
    </div>
  </div>
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<article
  bind:this={card}
  class="card"
  class:selected
  style:--annot={color}
  {onclick}
  oncontextmenu={(e) => showMenu(e, a)}
  aria-current={selected || undefined}
>
  <header>
    <button class="what" onclick={onselect} aria-label={i18n.t(`comment-kind-${kind}`)} title={i18n.t(`comment-kind-${kind}`)}>
      {#if preview}
        <svg viewBox="-3 -3 {preview.width + 6} {preview.height + 6}" aria-hidden="true">
          {#each preview.paths as d, i (i)}
            <path {d} />
          {/each}
        </svg>
      {:else}
        <Icon name={ICONS[kind]} />
      {/if}
    </button>
    <div class="who">
      <span class="author" dir="auto">{a.author || i18n.t("comment-no-author")}</span>
      <span class="when" title={exact(a.modified)}>{relative(a.modified)}</span>
    </div>
    <button class="icon more" onclick={(e) => showMenu(e, a)} aria-label={i18n.t("more-actions")} title={i18n.t("more-actions")}>
      <Icon name="more" />
    </button>
  </header>

  {#if quote}
    <blockquote dir="auto">{quote}</blockquote>
  {/if}

  {#if editing === "comment"}
    {@render editor(i18n.t("annot-comment-placeholder"))}
  {:else if a.contents}
    <p class="text" dir="auto">{a.contents}</p>
  {:else if canEdit && a.editable}
    <button class="add" onclick={() => start("comment", "")}>{i18n.t("annot-comment-placeholder")}</button>
  {/if}

  {#if thread.replies.length}
    <ul class="replies">
      {#each thread.replies as r (r.id)}
        <li oncontextmenu={(e) => showMenu(e, r)}>
          <div class="who">
            <span class="author" dir="auto">{r.author || i18n.t("comment-no-author")}</span>
            <span class="when" title={exact(r.modified)}>{relative(r.modified)}</span>
            <button class="icon more" onclick={(e) => showMenu(e, r)} aria-label={i18n.t("more-actions")} title={i18n.t("more-actions")}>
              <Icon name="more" />
            </button>
          </div>
          {#if editing === r.id}
            {@render editor(i18n.t("reply-placeholder"))}
          {:else}
            <p class="text" dir="auto">{r.contents}</p>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if editing === "new-reply"}
    {@render editor(i18n.t("reply-placeholder"))}
  {:else if canReply && !a.replyTo && (a.contents || thread.replies.length)}
    <button class="add" onclick={() => start("new-reply", "")}>{i18n.t("reply")}</button>
  {/if}
</article>

{#if menu}
  <ContextMenu {...menu} onclose={() => (menu = null)} />
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--border);
    border-inline-start: 3px solid var(--annot);
    border-radius: 8px;
    background: var(--surface);
    cursor: pointer;
    content-visibility: auto;
    contain-intrinsic-size: auto 80px;
  }
  .card:hover {
    background: var(--hover);
  }
  .card.selected {
    border-color: var(--accent);
    border-inline-start-color: var(--annot);
    box-shadow: 0 0 0 1px var(--accent);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .what {
    flex: none;
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--canvas);
    color: var(--annot);
  }
  .what svg {
    width: 24px;
    height: 24px;
    fill: none;
    stroke: var(--annot);
    stroke-width: 2.5;
    stroke-linecap: round;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }
  .what svg path {
    vector-effect: non-scaling-stroke;
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 0 8px;
    font-size: 12px;
  }
  .author {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    color: var(--muted);
  }
  .more {
    flex: none;
    border: none;
    opacity: 0.6;
  }
  .more:hover,
  .more:focus-visible {
    opacity: 1;
  }
  blockquote {
    margin: 0;
    padding-inline-start: 8px;
    border-inline-start: 2px solid color-mix(in srgb, var(--annot) 60%, transparent);
    color: var(--muted);
    font-size: 12px;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  /* Text runs in its own direction (dir="auto") but lines up with the panel's start edge. */
  .text,
  blockquote {
    text-align: left;
  }
  :global([dir="rtl"]) .text,
  :global([dir="rtl"]) blockquote {
    text-align: right;
  }
  .text {
    margin: 0;
    font-size: 13px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .add {
    align-self: flex-start;
    padding: 2px 6px;
    border: none;
    background: none;
    color: var(--muted);
    font-size: 12px;
  }
  .add:hover {
    color: var(--accent);
  }
  .replies {
    list-style: none;
    margin: 0;
    padding: 0;
    padding-inline-start: 10px;
    border-inline-start: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .replies .who {
    align-items: center;
  }
  .replies .more {
    margin-inline-start: auto;
  }
  .editor {
    display: flex;
    flex-direction: column;
    gap: 6px;
    cursor: auto;
  }
  textarea {
    resize: vertical;
    min-height: 56px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--canvas);
    color: var(--text);
    font-size: 13px;
  }
  textarea:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .actions button {
    padding: 3px 10px;
    font-size: 12px;
  }
  .primary {
    background: var(--accent);
    color: var(--accent-text);
    border-color: var(--accent);
  }
</style>
