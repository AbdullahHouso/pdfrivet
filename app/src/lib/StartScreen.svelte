<script lang="ts">
import logoUrl from "../../../assets/brand/rivet-icon.svg";
import Icon from "./Icon.svelte";
import { i18n } from "./i18n.svelte";
import { filesExist } from "./pdf";
import { settings } from "./settings.svelte";

interface Props {
  onopen: () => void;
  onopenpath: (path: string) => void;
}
let { onopen, onopenpath }: Props = $props();

// Which recent files still exist (checked each time the start screen shows).
let missing = $state<Set<string>>(new Set());
$effect(() => {
  const paths = settings.recent.map((r) => r.path);
  if (paths.length === 0) return;
  filesExist(paths)
    .then((exists) => (missing = new Set(paths.filter((_, i) => !exists[i]))))
    .catch(() => {});
});

const relative = $derived(new Intl.RelativeTimeFormat(i18n.locale, { numeric: "auto" }));
function when(ms: number): string {
  const minutes = Math.round((ms - Date.now()) / 60_000);
  if (Math.abs(minutes) < 60) return relative.format(minutes, "minute");
  const hours = Math.round(minutes / 60);
  if (Math.abs(hours) < 24) return relative.format(hours, "hour");
  const days = Math.round(hours / 24);
  if (Math.abs(days) < 30) return relative.format(days, "day");
  return new Date(ms).toLocaleDateString(i18n.locale);
}
</script>

<div class="start">
  <div class="hero">
    <img class="logo" src={logoUrl} alt="" width="96" height="96" />
    <h1>{i18n.t("app-name")}</h1>
    <p class="tagline">{i18n.t("app-tagline")}</p>
    <button class="primary big" onclick={onopen}>{i18n.t("open-file")}</button>
    <p class="hint">{i18n.t("drop-hint")}</p>
  </div>

  {#if settings.recent.length > 0}
    <section class="recent" aria-labelledby="recent-heading">
      <h2 id="recent-heading">{i18n.t("recent-files")}</h2>
      <ul>
        {#each settings.recent as file (file.path)}
          {@const gone = missing.has(file.path)}
          <li class:gone>
            <button class="open" disabled={gone} onclick={() => onopenpath(file.path)} title={file.path}>
              <Icon name="file" />
              <span class="text">
                <span class="title">{file.title}</span>
                <span class="path" dir="ltr">{file.path}</span>
              </span>
              <span class="when">{gone ? i18n.t("file-missing") : when(file.lastOpened)}</span>
            </button>
            <button class="remove" onclick={() => settings.removeRecent(file.path)}
              aria-label={i18n.t("remove-from-recent", { name: file.title })} title={i18n.t("remove-from-recent", { name: file.title })}>
              <Icon name="close" />
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  .start {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 40px;
    padding-block: 10vh 32px;
    padding-inline: 16px;
    background: var(--canvas);
  }
  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }
  .logo {
    margin-block-end: 12px;
  }
  h1 {
    margin: 0;
    font-size: 2.25rem;
  }
  .tagline {
    margin-block: 4px 24px;
    color: var(--muted);
  }
  .big {
    padding-block: 10px;
    padding-inline: 24px;
    font-size: 15px;
  }
  .hint {
    margin-block-start: 10px;
    font-size: 13px;
    color: var(--muted);
  }
  .recent {
    width: min(640px, 100%);
  }
  h2 {
    font-size: 14px;
    color: var(--muted);
    margin-block: 0 8px;
    padding-inline: 8px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    border: none;
    text-align: start;
    padding: 8px;
  }
  .open :global(.icon) {
    flex: none;
    color: var(--accent);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .title,
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title {
    font-weight: 600;
  }
  .path,
  .when {
    font-size: 12px;
    color: var(--muted);
  }
  .path {
    text-align: start;
  }
  .when {
    flex: none;
  }
  .gone .title {
    color: var(--muted);
    font-weight: 400;
  }
  .gone .open:disabled {
    opacity: 1;
  }
  .remove {
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    display: grid;
    place-items: center;
    border: none;
    opacity: 0.6;
  }
  .remove:hover {
    opacity: 1;
  }
  .remove :global(.icon) {
    width: 14px;
    height: 14px;
  }
</style>
