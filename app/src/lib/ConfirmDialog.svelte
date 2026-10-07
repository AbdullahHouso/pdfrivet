<script lang="ts">
// A small question dialog with a few answers (e.g. Save / Don't save / Cancel).
// Escape picks the last choice, which should be the safe "Cancel".
export interface Choice {
  id: string;
  label: string;
  primary?: boolean;
}

interface Props {
  title: string;
  message: string;
  choices: Choice[];
  onchoose: (id: string) => void;
}
let { title, message, choices, onchoose }: Props = $props();

let dialog: HTMLDialogElement;

$effect(() => {
  dialog.showModal();
  return () => dialog.close();
});
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="confirm-title"
  aria-describedby="confirm-message"
  oncancel={(e) => {
    e.preventDefault();
    onchoose(choices.at(-1)?.id ?? "cancel");
  }}
>
  <h2 id="confirm-title">{title}</h2>
  <p id="confirm-message">{message}</p>
  <div class="actions">
    {#each choices as choice (choice.id)}
      <!-- svelte-ignore a11y_autofocus -->
      <button class:primary={choice.primary} autofocus={choice.primary} onclick={() => onchoose(choice.id)}>
        {choice.label}
      </button>
    {/each}
  </div>
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
  dialog::backdrop {
    background: rgb(0 0 0 / 0.35);
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  p {
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-block-start: 16px;
  }
</style>
