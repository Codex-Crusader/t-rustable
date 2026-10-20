import { describe, expect, it } from "vitest";
import { buildKeymap, fromEvent, normalize } from "./keymap";

const event = (key: string, code: string, mods: Partial<Record<"ctrl" | "alt" | "shift" | "meta", boolean>> = {}) => ({
  key,
  code,
  ctrlKey: !!mods.ctrl,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
  metaKey: !!mods.meta,
});

describe("normalize", () => {
  it("puts modifiers in one order and names keys one way", () => {
    expect(normalize("shift+ctrl+r")).toBe("Ctrl+Shift+R");
    expect(normalize("Control + Esc")).toBe("Ctrl+Escape");
    expect(normalize("Ctrl++")).toBe("Ctrl+=");
    expect(normalize("Ctrl+Plus")).toBe("Ctrl+=");
    expect(normalize("f4")).toBe("F4");
  });

  it("refuses shortcuts that would take typed text", () => {
    expect(normalize("R")).toBeNull();
    expect(normalize("Shift+R")).toBeNull();
  });

  it("refuses unknown parts", () => {
    expect(normalize("Hyper+R")).toBeNull();
    expect(normalize("Ctrl+Ctrl+R")).toBeNull();
    expect(normalize("Ctrl+Banana")).toBeNull();
    expect(normalize("")).toBeNull();
  });
});

describe("fromEvent", () => {
  it("uses the physical key for letters and digits", () => {
    expect(fromEvent(event("!", "Digit1", { ctrl: true, shift: true }))).toBe("Ctrl+Shift+1");
    expect(fromEvent(event("й", "KeyQ", { ctrl: true }))).toBe("Ctrl+Q");
    expect(fromEvent(event("+", "Equal", { ctrl: true, shift: true }))).toBe("Ctrl+Shift+=");
  });

  it("ignores modifier keys and plain typing", () => {
    expect(fromEvent(event("Control", "ControlLeft", { ctrl: true }))).toBeNull();
    expect(fromEvent(event("a", "KeyA"))).toBeNull();
    expect(fromEvent(event("F6", "F6"))).toBe("F6");
  });
});

describe("buildKeymap", () => {
  const defaults = { "app.save": "Ctrl+S", "app.undo": "Ctrl+Z" };

  it("lets the user file win over defaults", () => {
    const user = JSON.stringify({ bindings: { "m.x/act": "Ctrl+S", "app.undo": null } });
    const map = buildKeymap(defaults, user, {});
    expect(map.byKey.get("Ctrl+S")).toBe("m.x/act");
    expect(map.byAction.has("app.save")).toBe(false);
    expect(map.byAction.has("app.undo")).toBe(false);
    expect(map.problems).toEqual([]);
  });

  it("does not bind a mod suggestion that conflicts", () => {
    const map = buildKeymap(defaults, null, { "m.x/a": "Ctrl+S", "m.x/b": "Ctrl+Shift+R", "m.x/c": "R" });
    expect(map.byKey.get("Ctrl+S")).toBe("app.save");
    expect(map.byKey.get("Ctrl+Shift+R")).toBe("m.x/b");
    expect(map.notBound).toEqual(["m.x/a", "m.x/c"]);
  });

  it("keeps the defaults when the user file is bad", () => {
    for (const bad of ["{", "[]", '{"bindings": 3}']) {
      const map = buildKeymap(defaults, bad, {});
      expect(map.problems.length).toBe(1);
      expect(map.byKey.get("Ctrl+S")).toBe("app.save");
    }
    const map = buildKeymap(defaults, JSON.stringify({ bindings: { "app.save": "S" } }), {});
    expect(map.problems.length).toBe(1);
    expect(map.byKey.get("Ctrl+S")).toBe("app.save");
  });

  it("gives a user binding to a mod action before its suggestion", () => {
    const user = JSON.stringify({ bindings: { "m.x/a": "Alt+A" } });
    const map = buildKeymap(defaults, user, { "m.x/a": "Ctrl+Shift+A" });
    expect(map.byAction.get("m.x/a")).toBe("Alt+A");
    expect(map.byKey.has("Ctrl+Shift+A")).toBe(false);
  });
});
