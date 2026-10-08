<script lang="ts">
// Small stroke icons used across the UI.
interface Props {
  name: keyof typeof paths;
}
let { name }: Props = $props();

const paths = {
  "chevron-up": "M6 15l6-6 6 6",
  "chevron-down": "M6 9l6 6 6-6",
  // Points to the end of the line; add class="flip-rtl" to its parent in RTL layouts.
  "chevron-end": "M9 6l6 6-6 6",
  minus: "M5 12h14",
  plus: "M12 5v14M5 12h14",
  close: "M6 6l12 12M18 6L6 18",
  rotate: "M20 12a8 8 0 1 1-2.34-5.66M20 4v5h-5",
  menu: "M4 7h16M4 12h16M4 17h16",
  sidebar: "M4 5h16v14H4zM9 5v14",
  file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5",
  lock: "M7 11V8a5 5 0 0 1 10 0v3M5 11h14v10H5z",
  save: "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6",
  info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5M12 8v.01",
  folder: "M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  pointer: "M5 3l14 7-6 2-2 6z",
  hand: "M8 13V5.5a1.5 1.5 0 0 1 3 0V12M11 11V4.5a1.5 1.5 0 0 1 3 0V12M14 11.5V6.5a1.5 1.5 0 0 1 3 0V15a6 6 0 0 1-6 6h-1a6 6 0 0 1-5-2.7L3.5 15a1.5 1.5 0 0 1 2.4-1.8L8 15",
  print: "M7 9V4h10v5M7 17H5a1 1 0 0 1-1-1v-5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v5a1 1 0 0 1-1 1h-2M7 14h10v6H7z",
  "fit-page":
    "M7 3H5a2 2 0 0 0-2 2v2M17 3h2a2 2 0 0 1 2 2v2M7 21H5a2 2 0 0 1-2-2v-2M17 21h2a2 2 0 0 0 2-2v-2M8 7h8v10H8z",
  "fit-width": "M3 5v14M21 5v14M7 12h10M7 12l3-3M7 12l3 3M17 12l-3-3M17 12l-3 3",
  "actual-size": "M4 5h16v14H4zM8 10l2-1v6M14 10l2-1v6M12 11v.01M12 14v.01",
  "one-page": "M7 3h10v18H7zM10 7h4M10 10h4",
  "two-pages": "M3 4h8v16H3zM13 4h8v16h-8z",
  sliders: "M4 6h9M17 6h3M15 4v4M4 12h3M11 12h9M9 10v4M4 18h11M19 18h1M17 16v4",
  contrast: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 3v18M12 7h5M12 11h8M12 15h7",
  globe: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18",
  code: "M8 6l-6 6 6 6M16 6l6 6-6 6",
  search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4.2-4.2",
  list: "M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01",
  // Annotating.
  pen: "M4 20l4-1L19 8a2.1 2.1 0 0 0-3-3L5 16zM14 7l3 3",
  highlight: "M9 15l-3 3h-3l2-2M9 15l9-9-3-3-9 9zM9 15l-3-3M13 21h8",
  underline: "M7 4v6a5 5 0 0 0 10 0V4M5 20h14",
  strikeout: "M5 12h14M16 7c-.5-1.5-2-3-4.5-3S7 5.5 7 7.5c0 4 10 3 10 8 0 2-2 4.5-5 4.5s-4.5-1.5-5-3",
  rectangle: "M4 6h16v12H4z",
  ellipse: "M12 19c4.4 0 8-3.1 8-7s-3.6-7-8-7-8 3.1-8 7 3.6 7 8 7z",
  line: "M5 19L19 5",
  arrow: "M5 19L19 5M10 5h9v9",
  note: "M5 4h14v11l-5 5H5zM14 20v-5h5M8 9h8M8 13h4",
  eraser: "M8 20h12M5.5 13.5l7-7a2 2 0 0 1 2.8 0l3.2 3.2a2 2 0 0 1 0 2.8L12 19H8.5l-3-3a1.8 1.8 0 0 1 0-2.5zM9 10l6 6",
  undo: "M9 14L4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3",
  redo: "M15 14l5-5-5-5M20 9H10a6 6 0 0 0 0 12h3",
  comment: "M4 5h16v11H9l-5 4z",
  trash: "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3",
  signature: "M3 17c3-1 5-9 7-9s-1 9 1 9 3-4 4-4 1 3 3 3 2-1 3-1M3 21h18",
  book: "M3 5h6a3 3 0 0 1 3 3v12a2 2 0 0 0-2-2H3zM21 5h-6a3 3 0 0 0-3 3v12a2 2 0 0 1 2-2h7z",
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
