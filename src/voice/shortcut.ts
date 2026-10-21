export function matchesVoiceShortcut(event: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">, shortcut: string): boolean {
  if (!shortcut.trim()) return false;
  const parts = shortcut.toLowerCase().split("+").map((part) => part.trim()).filter(Boolean);
  const key = parts.at(-1); if (!key) return false;
  const actual = event.code.toLowerCase() === key || event.key.toLowerCase() === key;
  return actual && event.ctrlKey === parts.includes("ctrl") && event.altKey === parts.includes("alt") && event.shiftKey === parts.includes("shift") && event.metaKey === parts.includes("meta");
}
