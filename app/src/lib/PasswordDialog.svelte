<script lang="ts">
// Asks for the password of a protected PDF. Uses the native <dialog>, which
// handles focus, Escape and screen readers for us.
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";

interface Props {
  fileName: string;
  wrong: boolean;
  onsubmit: (password: string) => void;
  oncancel: () => void;
}
let { fileName, wrong, onsubmit, oncancel }: Props = $props();

let dialog: HTMLDialogElement;
let password = $state("");

$effect(() => {
  dialog.showModal();
  return () => dialog.close();
});

// After a wrong password, clear the box so the user can retype.
$effect(() => {
  if (wrong) password = "";
});
</script>

<dialog bind:this={dialog} aria-labelledby="pw-title" oncancel={(e) => { e.preventDefault(); oncancel(); }}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      if (password) onsubmit(password);
    }}
  >
    <div class="head">
      <Icon name="lock" />
      <h2 id="pw-title">{i18n.t("password-title")}</h2>
    </div>
    <p>{i18n.t("password-prompt", { name: fileName })}</p>
    <!-- svelte-ignore a11y_autofocus -->
    <input type="password" bind:value={password} autofocus aria-label={i18n.t("password")}
      aria-invalid={wrong} aria-describedby={wrong ? "pw-error" : undefined} />
    {#if wrong}
      <p id="pw-error" class="error" role="alert">{i18n.t("error-wrong-password")}</p>
    {/if}
    <div class="actions">
      <button type="button" onclick={oncancel}>{i18n.t("cancel")}</button>
      <button type="submit" class="primary" disabled={!password}>{i18n.t("open")}</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    width: min(400px, calc(100vw - 32px));
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.25);
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
  p {
    overflow-wrap: anywhere;
  }
  input {
    width: 100%;
    font: inherit;
    color: inherit;
    background: var(--canvas);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
  }
  input[aria-invalid="true"] {
    border-color: var(--error-fg);
  }
  .error {
    color: var(--error-fg);
    margin-block: 8px 0;
    font-size: 13px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-block-start: 16px;
  }
</style>
