// Tooltips drawn by the app instead of the system: they follow the theme and
// language, look the same on every OS, and can show a keyboard shortcut.
//
// Any element with a `title` gets one automatically. Add
// `data-shortcut="Ctrl+O"` to show the shortcut too (written with Ctrl; Macs
// show ⌘). A labelled button whose label is the same text only gets a tooltip
// when there is a shortcut to show.

const DELAY = 500;
/** Like the system's tooltips, they go away on their own after a while. */
const LIFETIME = 5000;

const isMac = /Mac/.test(navigator.userAgent);

/** "Ctrl+Shift+S" as shown on this system: "⌘⇧S" on a Mac. */
export function formatShortcut(shortcut: string, mac = isMac): string {
  if (!mac) return shortcut;
  return shortcut
    .replace(/Ctrl\+/g, "⌘")
    .replace(/Shift\+/g, "⇧")
    .replace(/Alt\+/g, "⌥");
}

/**
 * What the tooltip says. A label that is already visible isn't worth a
 * tooltip of its own, but it stays next to its shortcut ("Open PDF… Ctrl+O").
 */
export function tooltipContent(
  tip: string,
  visibleText: string,
  shortcut: string | null,
): { text: string | null; shortcut: string | null } | null {
  const text = tip.trim() || null;
  if (!text && !shortcut) return null;
  if (text && !shortcut && text === visibleText.trim()) return null;
  return { text, shortcut };
}

let tooltip: HTMLDivElement | null = null;
let target: Element | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
// Checks that the pointer is still on the element. Browsers can stop telling
// us the pointer left (a dialog opened, the element got disabled), and after a
// dialog WebView2 keeps the old element "hovered", so the pointer's position
// is tracked here instead.
let watch: ReturnType<typeof setInterval> | undefined;
let pointer = { x: -1, y: -1, movedAt: 0 };

function pointerIsOn(el: Element): boolean {
  if (pointer.movedAt === 0) return false;
  const under = document.elementFromPoint(pointer.x, pointer.y);
  return !!under && (under === el || el.contains(under));
}

function element(): HTMLDivElement {
  if (!tooltip) {
    tooltip = document.createElement("div");
    tooltip.className = "tooltip";
    tooltip.setAttribute("role", "tooltip");
    // A popover sits in the top layer, above menus and dialogs.
    tooltip.popover = "manual";
    document.body.append(tooltip);
  }
  return tooltip;
}

function show(el: Element) {
  const content = tooltipContent(
    el.getAttribute("data-tip") ?? "",
    (el as HTMLElement).innerText ?? "",
    el.getAttribute("data-shortcut"),
  );
  if (!content || !el.isConnected) return;
  const tip = element();
  tip.replaceChildren();
  if (content.text) tip.append(content.text);
  if (content.shortcut) {
    const kbd = document.createElement("kbd");
    kbd.dir = "ltr";
    kbd.textContent = formatShortcut(content.shortcut);
    tip.append(kbd);
  }
  tip.showPopover();
  // Below the element, centred and kept on screen; above it if there's no room.
  const box = el.getBoundingClientRect();
  const size = tip.getBoundingClientRect();
  const margin = 6;
  const left = Math.min(Math.max(margin, box.left + box.width / 2 - size.width / 2), innerWidth - size.width - margin);
  const below = box.bottom + margin;
  const top = below + size.height <= innerHeight ? below : box.top - size.height - margin;
  tip.style.left = `${left}px`;
  tip.style.top = `${Math.max(margin, top)}px`;
  tip.classList.add("visible");
  const shownAt = Date.now();
  clearInterval(watch);
  watch = setInterval(() => {
    if (!el.isConnected || !pointerIsOn(el) || !document.hasFocus() || Date.now() - shownAt > LIFETIME) hide();
  }, 200);
}

function hide() {
  clearTimeout(timer);
  clearInterval(watch);
  target = null;
  if (tooltip?.classList.contains("visible")) {
    tooltip.classList.remove("visible");
    tooltip.hidePopover();
  }
}

function onOver(e: PointerEvent) {
  const el = (e.target as Element | null)?.closest?.("[title], [data-tip], [data-shortcut]");
  if (!el || el === target) return;
  // Take over the title, so the system tooltip doesn't appear as well, while
  // keeping it as the accessible name.
  const title = el.getAttribute("title");
  if (title !== null) {
    el.setAttribute("data-tip", title);
    el.removeAttribute("title");
    if (!el.hasAttribute("aria-label") && !(el as HTMLElement).innerText?.trim()) el.setAttribute("aria-label", title);
  }
  const wasVisible = tooltip?.classList.contains("visible");
  hide();
  target = el;
  // Moving from one tooltip to the next shows the next one right away.
  // Only after real pointer movement: not when a dialog closes over a button.
  timer = setTimeout(
    () => target === el && pointerIsOn(el) && Date.now() - pointer.movedAt < 1500 && show(el),
    wasVisible ? 0 : DELAY,
  );
}

function onOut(e: PointerEvent) {
  if (!target) return;
  const to = e.relatedTarget as Node | null;
  if (to && target.contains(to)) return;
  hide();
}

/** Turns on app-drawn tooltips for the whole window. Call once at startup. */
export function installTooltips() {
  document.addEventListener(
    "pointermove",
    (e) => {
      pointer = { x: e.clientX, y: e.clientY, movedAt: Date.now() };
    },
    { passive: true },
  );
  document.documentElement.addEventListener("pointerleave", hide);
  document.addEventListener("pointerover", onOver);
  document.addEventListener("pointerout", onOut);
  for (const event of ["pointerdown", "keydown", "wheel", "blur"]) {
    window.addEventListener(event, hide, true);
  }
  document.addEventListener("visibilitychange", hide);
}
