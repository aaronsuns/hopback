import { describe, expect, it } from "vitest";
import { askQuestion, moveSelection, otherMode, parseRoute } from "./search";

describe("askQuestion", () => {
  it("detects ? queries", () => {
    expect(askQuestion("?the one about flaky deploys")).toBe("the one about flaky deploys");
    expect(askQuestion("  ? retry logic ")).toBe("retry logic");
    expect(askQuestion("?")).toBeNull();
    expect(askQuestion("why?")).toBeNull();
  });
});

describe("moveSelection", () => {
  it("wraps both ways", () => {
    expect(moveSelection(0, -1, 3)).toBe(2);
    expect(moveSelection(2, 1, 3)).toBe(0);
    expect(moveSelection(1, 1, 3)).toBe(2);
    expect(moveSelection(5, 1, 0)).toBe(0);
  });
});

describe("routes and modes", () => {
  it("parses hashes", () => {
    expect(parseRoute("#search/history")).toEqual({ view: "search", arg: "history" });
    expect(parseRoute("#settings")).toEqual({ view: "settings", arg: null });
    expect(parseRoute("")).toEqual({ view: "search", arg: null });
  });
  it("toggles modes", () => {
    expect(otherMode("open")).toBe("history");
    expect(otherMode("history")).toBe("open");
  });
});
