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
  return key;
}
