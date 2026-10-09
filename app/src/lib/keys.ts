// Keyboard shortcuts that work in every keyboard layout.

/**
 * The key a shortcut is matched against. Latin layouts (English, French…)
 * use the character they type, so Ctrl+Z is wherever Z is on that keyboard.
 * Other layouts (Arabic, Persian, Hebrew, Russian…) type other letters, so
 * the key's position on a US keyboard is used instead: Ctrl+S still saves
 * when the Arabic layout is active and the key types "س".
 */
export function shortcutKey(e: Pick<KeyboardEvent, "key" | "code">): string {
  const key = e.key.toLowerCase();
  if (key.length === 1 && key >= " " && key <= "~") return key;
  if (e.code.startsWith("Key")) return e.code.slice(3).toLowerCase();
  if (e.code.startsWith("Digit")) return e.code.slice(5);
  if (e.code === "Equal") return "=";
  if (e.code === "Minus") return "-";
  if (e.code === "Comma") return ",";
  return key;
}

/** A modal dialog (Settings, Print…) is open: the window's shortcuts wait until it closes. */
export function modalOpen(): boolean {
  return document.querySelector("dialog:modal") !== null;
}

/** The key goes to something the user types in (a field or editable text). */
export function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLSelectElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}
