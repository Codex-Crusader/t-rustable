// `--self-test <pdf>`: a scenario through the same calls that the UI uses. The Rust side
// writes the report and stops the app.

import { api, errorText } from "./api";
import { app } from "./store.svelte";
import type { DocInfo, PageInfo } from "./types";

interface Step {
  name: string;
  ok: boolean;
  ms: number;
  detail?: unknown;
}

const NO_MODS = { shift: false, ctrl: false, alt: false, meta: false };

function until(cond: () => boolean, timeout = 10000): Promise<void> {
  const start = performance.now();
  return new Promise((resolve, reject) => {
    const tick = () => {
      if (cond()) resolve();
      else if (performance.now() - start > timeout) reject(new Error("timed out"));
      else setTimeout(tick, 20);
    };
    tick();
  });
}

function frame(): Promise<number> {
  return new Promise((r) => requestAnimationFrame(r));
}

function doc(): DocInfo {
  const t = app.tab;
  if (!t) throw new Error("no open document");
  return t.doc;
}

function page(i: number): PageInfo {
  const p = doc().pages[i];
  if (!p) throw new Error(`no page ${i + 1}`);
  return p;
}

/** Waits until the revision of page `i` is above `before`. */
async function changed(i: number, before: number) {
  await until(() => page(i).revision > before);
}

async function drag(i: number, from: [number, number], to: [number, number]) {
  const d = doc().id;
  const id = page(i).id;
  const base = { page: id, button: "primary", pressure: 0.5, ...NO_MODS };
  await api.pointer(d, { ...base, kind: "down", x: from[0], y: from[1] });
  await api.pointer(d, { ...base, kind: "move", button: "none", x: (from[0] + to[0]) / 2, y: (from[1] + to[1]) / 2 });
  await api.pointer(d, { ...base, kind: "move", button: "none", x: to[0], y: to[1] });
  await api.pointer(d, { ...base, kind: "up", x: to[0], y: to[1] });
}

async function key(name: string) {
  await api.key(doc().id, { kind: "down", key: name, code: name, repeat: false, ...NO_MODS });
}

export async function runSelfTest(path: string) {
  const steps: Step[] = [];
  const step = async (name: string, f: () => Promise<unknown>) => {
    const start = performance.now();
    try {
      const detail = await f();
      steps.push({ name, ok: true, ms: Math.round(performance.now() - start), detail });
    } catch (e) {
      steps.push({ name, ok: false, ms: Math.round(performance.now() - start), detail: errorText(e) });
    }
  };

  await step("open and first tile", async () => {
    const tile = new Promise<void>((r) => window.addEventListener("tr-tile", () => r(), { once: true }));
    await app.open(path);
    await Promise.race([tile, until(() => false, 10000)]);
    return { pages: doc().pages.length };
  });

  await step("text layer", async () => {
    await until(() => document.querySelectorAll(".text-layer text").length > 0);
    return { runs: document.querySelectorAll(".text-layer text").length };
  });

  await step("scroll", async () => {
    const scroller = document.querySelector<HTMLElement>(".scroller");
    if (!scroller) throw new Error("no viewer");
    const frames: number[] = [];
    let last = await frame();
    const start = last;
    while (last - start < 3000) {
      scroller.scrollTop += 50;
      const now = await frame();
      frames.push(now - last);
      last = now;
    }
    scroller.scrollTop = 0;
    frames.sort((a, b) => a - b);
    const mean = frames.reduce((a, b) => a + b, 0) / frames.length;
    return {
      frames: frames.length,
      meanMs: Math.round(mean * 10) / 10,
      p95Ms: Math.round(frames[Math.floor(frames.length * 0.95)]! * 10) / 10,
      over33: frames.filter((f) => f > 33).length,
    };
  });

  await step("shapes: rectangle", async () => {
    const before = page(0).revision;
    await app.selectTool("t-rustable.shapes", "rectangle", "Rectangle");
    await drag(0, [60, 60], [160, 120]);
    await changed(0, before);
  });

  await step("add-text: type and Enter", async () => {
    const before = page(0).revision;
    await app.selectTool("t-rustable.add-text", "text", "Text");
    await drag(0, [60, 200], [60, 200]);
    const overlay = app.overlays["t-rustable.add-text"] ?? [];
    await api.textInput(doc().id, "Self test");
    await key("Enter");
    await changed(0, before);
    return { overlayShapes: overlay.length };
  });

  await step("add-image: pick and click", async () => {
    const before = page(1).revision;
    await app.selectTool("t-rustable.add-image", "add-image", "Add image");
    await drag(1, [100, 100], [100, 100]);
    await changed(1, before);
  });

  await step("page-tools: rotate right", async () => {
    await app.selectTool(null);
    const { width, height } = page(0);
    const tab = app.tab!;
    tab.activePage = page(0).id;
    tab.selected = [];
    app.pushContext();
    await api.runAction("t-rustable.page-tools", "rotate-right");
    await until(() => Math.abs(page(0).width - height) < 0.01 && Math.abs(page(0).height - width) < 0.01);
  });

  await step("undo and redo", async () => {
    const { width } = page(0);
    await app.undo(false);
    await until(() => Math.abs(page(0).width - width) > 0.01);
    await app.undo(true);
    await until(() => Math.abs(page(0).width - width) < 0.01);
    return { canUndo: doc().canUndo };
  });

  await step("save", async () => {
    const info = await api.save(doc().id, true, false, "self-test-out.pdf");
    if (!info?.path?.endsWith("self-test-out.pdf") || info.dirty) throw new Error("save did not finish");
    return { path: info.path };
  });

  const ok = steps.every((s) => s.ok);
  await api.selfTestReport({ ok, version: app.info.version, platform: app.info.platform, steps });
}
