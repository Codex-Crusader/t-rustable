import { describe, expect, it } from "vitest";
import { bucketFor, renderScale, schemeBase, tilePath, visibleTiles, TILE } from "./tiles";

describe("tiles", () => {
  it("picks the bucket at or above the zoom", () => {
    expect(bucketFor(0.1)).toBe(0.25);
    expect(bucketFor(1)).toBe(1);
    expect(bucketFor(1.01)).toBe(2);
    expect(bucketFor(9)).toBe(4);
  });

  it("returns only tiles in the visible part", () => {
    // 600 x 800 points at 2 device pixels per point: 1200 x 1600 pixels, 3 x 4 tiles.
    const all = visibleTiles(600, 800, 2, 2, { top: 0, bottom: 1e9, left: 0, right: 1e9 });
    expect(all.length).toBe(12);
    expect(all[all.length - 1]!.width).toBe(1200 - 2 * TILE);
    const top = visibleTiles(600, 800, 2, 2, { top: 0, bottom: 100, left: 0, right: 100 });
    expect(top.map((t) => [t.tx, t.ty])).toEqual([[0, 0]]);
  });

  it("scales the CSS size when the render scale differs", () => {
    const [t] = visibleTiles(600, 800, 1, 2, { top: 0, bottom: 10, left: 0, right: 10 });
    expect(t!.width).toBe(TILE / 2);
  });

  it("builds URLs like the Rust parser expects", () => {
    expect(schemeBase("trtile", "windows")).toBe("http://trtile.localhost");
    expect(schemeBase("trtile", "linux")).toBe("trtile://localhost");
    expect(tilePath(3, "ab", 2, renderScale(1, 1), 4, 5)).toBe("/3/ab/2/1333/4_5.png");
  });
});
