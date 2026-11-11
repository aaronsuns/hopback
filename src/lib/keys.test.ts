import { describe, expect, it } from "vitest";
import { accelFromEvent, displayAccel } from "./keys";

const ev = (code: string, mods: Partial<{ ctrl: boolean; alt: boolean; shift: boolean; meta: boolean }> = {}) => ({
  code,
  ctrlKey: !!mods.ctrl,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
  metaKey: !!mods.meta,
});

describe("accelFromEvent", () => {
  it("builds canonical accelerators in modifier order", () => {
    expect(accelFromEvent(ev("KeyC", { meta: true, ctrl: true, shift: true }))).toBe("Ctrl+Shift+Cmd+KeyC");
    expect(accelFromEvent(ev("Digit1", { ctrl: true, alt: true }))).toBe("Ctrl+Alt+Digit1");
    expect(accelFromEvent(ev("Tab", { ctrl: true, alt: true, shift: true }))).toBe("Ctrl+Alt+Shift+Tab");
  });
  it("waits while only modifiers are down", () => {
    expect(accelFromEvent(ev("ControlLeft", { ctrl: true }))).toBeNull();
    expect(accelFromEvent(ev("MetaRight", { meta: true }))).toBeNull();
    expect(accelFromEvent(ev(""))).toBeNull();
  });
  it("keeps keys without modifiers so the backend can explain the rejection", () => {
    expect(accelFromEvent(ev("KeyK"))).toBe("KeyK");
  });
});

describe("displayAccel", () => {
  it("renders Mac glyphs", () => {
    expect(displayAccel("Ctrl+Shift+Cmd+KeyC")).toBe("⌃⇧⌘C");
    expect(displayAccel("Ctrl+Alt+Digit0")).toBe("⌃⌥0");
    expect(displayAccel("Ctrl+Alt+Shift+Tab")).toBe("⌃⌥⇧⇥");
    expect(displayAccel("Alt+F5")).toBe("⌥F5");
    expect(displayAccel(null)).toBe("");
  });
});
