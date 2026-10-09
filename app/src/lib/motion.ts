// Animations for elements that appear and disappear with Svelte's {#if}
// (menus and popovers that stay in the page are animated in app.css). Short,
// opacity and transform only; nothing moves when animations are turned off
// (Settings → Appearance → Animations).

import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

/** Entry and exit lengths, in ms (as in app.css). */
const IN = 140;
const OUT = 90;

/** Whether animations are turned off (see settings.svelte.ts). */
export function motionOff(): boolean {
  return typeof document === "undefined" || document.documentElement.dataset.motion === "off";
}

/** Fades in while sliding a few pixels into place (`y` > 0 comes up from below). */
export function rise(_node: Element, { y = -6 }: { y?: number } = {}): TransitionConfig {
  return {
    duration: motionOff() ? 0 : IN,
    easing: cubicOut,
    css: (t, u) => `opacity: ${t}; transform: translateY(${u * y}px)`,
  };
}

/** Fades in while growing slightly from 96% (menus attached to something). */
export function pop(_node: Element): TransitionConfig {
  return {
    duration: motionOff() ? 0 : IN,
    easing: cubicOut,
    css: (t) => `opacity: ${t}; transform: scale(${0.96 + 0.04 * t})`,
  };
}

/** Just fades (for elements that are positioned with a transform of their own). */
export function fade(_node: Element, { enter = true }: { enter?: boolean } = {}): TransitionConfig {
  return {
    duration: motionOff() ? 0 : enter ? IN : OUT,
    easing: cubicOut,
    css: (t) => `opacity: ${t}`,
  };
}

/** Quickly fades out (the exit of `rise` and `pop`). */
export function out(_node: Element): TransitionConfig {
  return { duration: motionOff() ? 0 : OUT, css: (t) => `opacity: ${t}` };
}

/** Closing a dialog: it and its backdrop fade out together. */
export function dialogOut(node: Element): TransitionConfig {
  const el = node as HTMLDialogElement;
  return {
    duration: motionOff() ? 0 : OUT,
    css: (t) => `opacity: ${t}; transform: scale(${0.98 + 0.02 * t})`,
    // The backdrop isn't the dialog itself; it follows through a CSS variable.
    tick: (t) => el.style.setProperty("--backdrop-opacity", String(t)),
  };
}
