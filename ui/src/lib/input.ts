// Pointer, key, and text input for the active tool.

import { api } from "./api";
import { app } from "./store.svelte";
import type { DocId, PageId } from "./types";

type Kind = "down" | "move" | "up" | "enter" | "leave" | "double-click";

interface Pending {
  doc: DocId;
  event: Parameters<typeof api.pointer>[1];
}

let busy = false;
let queued: Pending | null = null;
let textInput: HTMLTextAreaElement | null = null;

function button(b: number): string {
  return ["primary", "middle", "secondary"][b] ?? "none";
}

/** Sends a pointer event. Moves wait while a call runs, and only the last one goes. */
export function pointer(doc: DocId, page: PageId, kind: Kind, x: number, y: number, e: PointerEvent | MouseEvent) {
  const event = {
    kind,
    page,
    x,
    y,
    button: kind === "move" ? "none" : button(e.button),
    pressure: "pressure" in e ? e.pressure : 0,
    shift: e.shiftKey,
    ctrl: e.ctrlKey,
    alt: e.altKey,
    meta: e.metaKey,
  };
  if (busy) {
    // A move can replace a queued move. Other events wait in order after it.
    if (kind === "move" && (queued === null || queued.event.kind === "move")) {
      queued = { doc, event };
      return;
    }
    void sendAfter({ doc, event });
    return;
  }
  void send({ doc, event });
}

let chain: Promise<void> = Promise.resolve();

function sendAfter(p: Pending): Promise<void> {
  chain = chain.then(() => send(p));
  return chain;
}

async function send(p: Pending): Promise<void> {
  busy = true;
  try {
    const reply = await api.pointer(p.doc, p.event);
    app.cursor = reply.cursor;
    if (app.tool) app.setOverlay(app.tool.modId, reply.overlay);
  } catch (e) {
    app.fail(e);
  } finally {
    busy = false;
  }
  const next = queued;
  queued = null;
  if (next) await send(next);
}

export function registerTextInput(el: HTMLTextAreaElement | null) {
  textInput = el;
}

/** Gives the keyboard to the active tool, so typed text goes to `on-text-input`. */
export function focusToolInput() {
  textInput?.focus({ preventScroll: true });
}

export async function key(e: KeyboardEvent): Promise<boolean> {
  const doc = app.activeDoc;
  if (!app.tool || doc === null || e.isComposing) return false;
  try {
    return await api.key(doc, {
      kind: e.type === "keyup" ? "up" : "down",
      key: e.key,
      code: e.code,
      repeat: e.repeat,
      shift: e.shiftKey,
      ctrl: e.ctrlKey,
      alt: e.altKey,
      meta: e.metaKey,
    });
  } catch (err) {
    app.fail(err);
    return false;
  }
}

export async function text(value: string) {
  const doc = app.activeDoc;
  if (!app.tool || doc === null || !value) return;
  try {
    await api.textInput(doc, value);
  } catch (e) {
    app.fail(e);
  }
}
