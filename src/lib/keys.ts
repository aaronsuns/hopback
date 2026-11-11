// Shortcut recorder helpers. The canonical form matches the Rust side:
// "Ctrl+Alt+Shift+Cmd+<KeyboardEvent.code>", modifiers in that order.

export interface KeyLike {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight",
  "MetaLeft", "MetaRight", "CapsLock", "Fn", "OSLeft", "OSRight",
]);

/** The accelerator for a keydown, or null while only modifiers are held. */
export function accelFromEvent(e: KeyLike): string | null {
  if (!e.code || MODIFIER_CODES.has(e.code)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Cmd");
  parts.push(e.code);
  return parts.join("+");
}

const KEY_LABELS: Record<string, string> = {
  Tab: "⇥", Enter: "↩", Escape: "⎋", Backspace: "⌫", Delete: "⌦", Space: "Space",
  ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→",
  Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Backslash: "\\",
  Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/", Backquote: "`",
};

/** "Ctrl+Shift+Cmd+KeyC" → "⌃⇧⌘C". */
export function displayAccel(accel: string | null | undefined): string {
  if (!accel) return "";
  const parts = accel.split("+");
  const code = parts.pop() ?? "";
  const glyph: Record<string, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Cmd: "⌘" };
  const mods = ["Ctrl", "Alt", "Shift", "Cmd"].filter((m) => parts.includes(m)).map((m) => glyph[m]).join("");
  const key = code.replace(/^Key/, "").replace(/^Digit/, "");
  return mods + (KEY_LABELS[code] ?? key);
}
