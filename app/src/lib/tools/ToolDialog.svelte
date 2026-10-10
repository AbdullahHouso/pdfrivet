<script lang="ts">
// The frame every tool dialog shares: a title with an icon, the tool's
// options, an optional progress bar, and the buttons. A native modal <dialog>
// (focus, Escape and screen readers come with it), closed by the app.
import type { ComponentProps, Snippet } from "svelte";
import Icon from "../Icon.svelte";
import { dialogOut } from "../motion";

interface Props {
  title: string;
  icon: ComponentProps<typeof Icon>["name"];
  /** The tool's options. */
  children: Snippet;
  /** Buttons, in reading order (the main one last). */
  actions: Snippet;
  /** Escape or the close button. */
  oncancel: () => void;
  /** Runs the tool (Enter in a field, or a submit button). */
  onsubmit?: () => void;
  /** 0–1 while working, or null. */
  progress?: number | null;
  wide?: boolean;
}
let { title, icon, children, actions, oncancel, onsubmit, progress = null, wide = false }: Props = $props();

let dialog: HTMLDialogElement;
const id = `tool-${Math.random().toString(36).slice(2, 8)}`;

$effect(() => {
  dialog.showModal();
  return () => dialog.close();
});
</script>

<dialog out:dialogOut|global bind:this={dialog} class:wide aria-labelledby={id}
  oncancel={(e) => { e.preventDefault(); oncancel(); }}>
  <form onsubmit={(e) => { e.preventDefault(); onsubmit?.(); }}>
    <div class="head">
      <Icon name={icon} />
      <h2 {id}>{title}</h2>
    </div>
    <div class="body">
      {@render children()}
    </div>
    {#if progress !== null}
      <progress value={progress} max="1"></progress>
    {/if}
    <div class="actions">
      {@render actions()}
    </div>
  </form>
</dialog>

<style>
  dialog {
    width: min(440px, calc(100vw - 32px));
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.25);
  }
  dialog.wide {
    width: min(620px, calc(100vw - 32px));
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
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
    gap: 12px;
    margin-block-start: 16px;
  }
  .body :global(label.field) {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .body :global(.hint) {
    color: var(--muted);
    font-size: 12px;
  }
  .body :global(input[type="text"]),
  .body :global(input[type="password"]),
  .body :global(input[type="number"]),
  .body :global(input[type="url"]) {
    width: 100%;
    font: inherit;
    color: inherit;
    background: var(--field);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
  }
  .body :global(input[aria-invalid="true"]) {
    border-color: var(--error-fg);
  }
  .body :global(.check) {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .body :global(.check input) {
    accent-color: var(--accent);
  }
  .body :global(.error) {
    color: var(--error-fg);
    font-size: 13px;
    margin: 0;
  }
  progress {
    width: 100%;
    margin-block-start: 16px;
    accent-color: var(--accent);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-block-start: 20px;
  }
</style>
