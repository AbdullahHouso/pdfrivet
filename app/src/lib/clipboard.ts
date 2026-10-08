// Copying to the clipboard with the webview's own API (no plugin needed).
// If the async Clipboard API is missing or refuses (some webviews only allow
// it in certain conditions), the older copy command is used instead.

export async function writeClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // Fall through to the copy command.
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.append(area);
  const focused = document.activeElement as HTMLElement | null;
  area.select();
  const ok = document.execCommand("copy");
  area.remove();
  focused?.focus({ preventScroll: true });
  if (!ok) throw new Error("copy refused");
}
