<script lang="ts">
// Small stroke icons used across the UI.
interface Props {
  name: keyof typeof paths;
}
let { name }: Props = $props();

const paths = {
  "chevron-up": "M6 15l6-6 6 6",
  "chevron-down": "M6 9l6 6 6-6",
  minus: "M5 12h14",
  plus: "M12 5v14M5 12h14",
  close: "M6 6l12 12M18 6L6 18",
  rotate: "M20 12a8 8 0 1 1-2.34-5.66M20 4v5h-5",
  menu: "M4 7h16M4 12h16M4 17h16",
  sidebar: "M4 5h16v14H4zM9 5v14",
  file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5",
  lock: "M7 11V8a5 5 0 0 1 10 0v3M5 11h14v10H5z",
  save: "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6",
  print: "M7 9V4h10v5M7 17H5a1 1 0 0 1-1-1v-5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v5a1 1 0 0 1-1 1h-2M7 14h10v6H7z",
  "fit-page":
    "M7 3H5a2 2 0 0 0-2 2v2M17 3h2a2 2 0 0 1 2 2v2M7 21H5a2 2 0 0 1-2-2v-2M17 21h2a2 2 0 0 0 2-2v-2M8 7h8v10H8z",
  "fit-width": "M3 5v14M21 5v14M7 12h10M7 12l3-3M7 12l3 3M17 12l-3-3M17 12l-3 3",
  "actual-size": "M4 5h16v14H4zM8 10l2-1v6M14 10l2-1v6M12 11v.01M12 14v.01",
  "one-page": "M7 3h10v18H7zM10 7h4M10 10h4",
  "two-pages": "M3 4h8v16H3zM13 4h8v16h-8z",
} as const;
</script>

<svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
  <path d={paths[name]} />
</svg>

<style>
  .icon {
    width: 18px;
    height: 18px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
