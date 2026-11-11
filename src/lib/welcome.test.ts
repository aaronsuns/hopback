import { describe, expect, it } from "vitest";
import { notificationsDone } from "./welcome";

describe("notificationsDone", () => {
  it("trusts macOS when it answers", () => {
    expect(notificationsDone(true, false)).toBe(true);
    expect(notificationsDone(false, true)).toBe(false);
  });
  it("falls back to the test banner when macOS has not been asked", () => {
    expect(notificationsDone(null, false)).toBe(false);
    expect(notificationsDone(null, true)).toBe(true);
  });
});
