<script lang="ts">
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { pop } from "./motion";
import { type Tab, tabs } from "./tabs.svelte";

interface Props {
  onopen: () => void;
  /** Closes a tab (asking about unsaved changes first). */
  onclose: (tab: Tab) => void;
}
let { onopen, onclose }: Props = $props();
</script>

<div class="tabbar" role="tablist" aria-label={i18n.t("open-documents")}>
  {#each tabs.list as tab (tab.id)}
    {@const active = tab.id === tabs.active?.id}
    <div
      in:pop
      class="tab"
      class:active
      role="tab"
      tabindex={active ? 0 : -1}
      aria-selected={active}
      title={tab.path}
      onclick={() => tabs.activate(tab.id)}
      onauxclick={(e) => e.button === 1 && onclose(tab)}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && tabs.activate(tab.id)}
    >
      <Icon name="file" />
      <span class="name" dir="auto">{tab.title}</span>
      {#if tab.dirty}
        <span class="dirty" title={i18n.t("unsaved-changes")} aria-label={i18n.t("unsaved-changes")}>●</span>
      {/if}
      <button
        class="close"
        onclick={(e) => {
          e.stopPropagation();
          onclose(tab);
        }}
        aria-label={i18n.t("close-tab", { name: tab.title })}
        title={i18n.t("close-tab", { name: tab.title })}
      >
        <Icon name="close" />
      </button>
    </div>
  {/each}
  <button class="new" onclick={onopen} aria-label={i18n.t("open-file")} title={i18n.t("open-file")}>
    <Icon name="plus" />
  </button>
</div>

<style>
  .tabbar {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    padding-block-start: 6px;
    padding-inline: 6px;
    background: var(--chrome);
    border-block-end: 1px solid var(--border);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 220px;
    flex: 0 1 220px;
    padding-block: 6px;
    padding-inline: 10px 4px;
    border: 1px solid transparent;
    border-block-end: none;
    border-start-start-radius: 8px;
    border-start-end-radius: 8px;
    color: var(--muted);
    cursor: default;
    user-select: none;
  }
  .tab:hover {
    background: var(--hover);
  }
  .tab.active {
    font-weight: 500;
    background: var(--surface);
    border-color: var(--border);
    color: var(--text);
    margin-block-end: -1px;
    padding-block-end: 7px;
  }
  .tab :global(.icon) {
    flex: none;
    width: 15px;
    height: 15px;
    color: var(--accent);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
  .dirty {
    flex: none;
    color: var(--accent);
    font-size: 10px;
  }
  .close,
  .new {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    background: none;
    border-radius: 4px;
  }
  .close :global(.icon) {
    width: 13px;
    height: 13px;
    color: inherit;
  }
  .new {
    margin-block-end: 5px;
    margin-inline-start: 4px;
  }
</style>
