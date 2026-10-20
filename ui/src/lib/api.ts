// Typed calls to the Rust side (ARCHITECTURE section 11). One function per command.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DocId, DocInfo, Hit, ModUi, OutlineItem, PageId, PointerReply, Shape, TextRun } from "./types";

export interface AppInfo {
  version: string;
  platform: "windows" | "linux";
  selfTest: string | null;
  files: string[];
}

export interface Mods {
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
  meta: boolean;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  openFile: (path: string | null = null, password: string | null = null) =>
    invoke<DocInfo | null>("open_file", { path, password }),
  document: (doc: DocId) => invoke<DocInfo>("document", { doc }),
  closeDoc: (doc: DocId, force: boolean) => invoke<void>("close_doc", { doc, force }),
  setContext: (context: {
    doc: DocId | null;
    activePage: PageId | null;
    selectedPages: PageId[];
    zoom: number;
    devicePixelRatio: number;
  }) => invoke<void>("set_context", { context }),
  pageText: (doc: DocId, page: PageId) => invoke<TextRun[]>("page_text", { doc, page }),
  search: (doc: DocId, query: string, matchCase: boolean, wholeWord: boolean) =>
    invoke<Hit[]>("search", { doc, query, matchCase, wholeWord }),
  registry: () => invoke<ModUi[]>("registry"),
  selectTool: (tool: string | null) => invoke<void>("select_tool", { tool }),
  runAction: (modId: string, actionId: string, args: string | null = null) =>
    invoke<void>("run_action", { modId, actionId, args }),
  cancelAction: () => invoke<void>("cancel_action"),
  pointer: (
    doc: DocId,
    event: { kind: string; page: PageId; x: number; y: number; button: string; pressure: number } & Mods,
  ) => invoke<PointerReply>("pointer", { doc, event }),
  key: (doc: DocId, event: { kind: "down" | "up"; key: string; code: string; repeat: boolean } & Mods) =>
    invoke<boolean>("key", { doc, event }),
  textInput: (doc: DocId, text: string) => invoke<void>("text_input", { doc, text }),
  answerConfirm: (id: number, yes: boolean) => invoke<void>("answer_confirm", { id, yes }),
  undo: (doc: DocId, redo: boolean) => invoke<void>("undo", { doc, redo }),
  save: (doc: DocId, saveAs: boolean, allowInvalidateSignatures: boolean, target: string | null = null) =>
    invoke<DocInfo | null>("save", { doc, saveAs, allowInvalidateSignatures, target }),
  selfTestReport: (report: unknown) => invoke<void>("self_test_report", { report }),
  cancelTiles: (paths: string[]) => invoke<void>("cancel_tiles", { paths }),
  outline: (doc: DocId) => invoke<OutlineItem[]>("outline", { doc }),
  panelOpen: (modId: string, panelId: string, open: boolean) =>
    invoke<void>("panel_open", { modId, panelId, open }),
  panelMessage: (modId: string, panelId: string, message: string) =>
    invoke<void>("panel_message", { modId, panelId, message }),
  setPanelTheme: (css: string) => invoke<void>("set_panel_theme", { css }),
  selfTestSecretCalls: () => invoke<number>("self_test_secret_calls"),
  toPdf: (doc: DocId, page: PageId, x: number, y: number) => invoke<[number, number]>("to_pdf", { doc, page, x, y }),
};

export interface Events {
  "document-changed": { doc: DocId; pages: PageId[]; structure: boolean };
  overlay: { modId: string; shapes: Shape[] };
  notify: { modId: string; level: "info" | "success" | "warning" | "error"; message: string };
  "confirm-request": { id: number; modId: string; title: string; message: string };
  progress: { modId: string; fraction: number; message: string };
  "registry-changed": null;
  "panel-message": { modId: string; panelId: string; message: string };
}

export function on<K extends keyof Events>(event: K, handler: (payload: Events[K]) => void): Promise<UnlistenFn> {
  return listen<Events[K]>(event, (e) => handler(e.payload));
}

/** The error object of a failed command. */
export function errorText(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    const err = e as { message: string; hint?: string | null };
    return err.hint ? `${err.message} ${err.hint}` : err.message;
  }
  return String(e);
}

export function errorCode(e: unknown): string | null {
  return e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : null;
}
