import { describe, expect, it } from "vitest";
import { filter, score } from "./palette";

describe("palette filter", () => {
  const items = ["Rotate left", "Rotate right", "Save as...", "Save", "Zoom in"];

  it("puts prefix matches first, then word matches, then fuzzy matches", () => {
    expect(filter(items, "save", (s) => s)).toEqual(["Save as...", "Save"]);
    expect(filter(items, "right", (s) => s)).toEqual(["Rotate right"]);
    expect(filter(items, "rr", (s) => s)).toEqual(["Rotate right"]);
    expect(filter(items, "rot", (s) => s)).toEqual(["Rotate left", "Rotate right"]);
  });

  it("matches all items for an empty query, and none for a missing letter", () => {
    expect(filter(items, "  ", (s) => s)).toEqual(items);
    expect(filter(items, "xq", (s) => s)).toEqual([]);
  });

  it("gives fewer gaps a better score", () => {
    expect(score("zi", "Zoom in")!).toBeLessThan(score("zn", "Zoom in")!);
  });
});
