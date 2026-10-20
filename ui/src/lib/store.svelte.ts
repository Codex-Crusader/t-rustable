// App state. Components read it, and they change it only through these methods.

import { api, errorCode, errorText, on, type AppInfo } from "./api";
import { S } from "./strings";
import { CSS_PER_POINT, schemeBase } from "./tiles";
import type { DocId, DocInfo, ModUi, PageId, Shape } from "./types";

export const ZOOMS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];

export interface Tab {
  doc: DocInfo;
  zoom: number;
  activePage: PageId | null;
  selected: PageId[];
  /** Increments to ask the viewer to scroll to `scrollTarget`. */
  scrollRequest: number;
  scrollTarget: PageId | null;
}

export interface Toast {
  id: number;
  level: "info" | "success" | "warning" | "error";
  message: string;
}

export interface Ask {
  title: string;
  message: string;
  resolve: (yes: boolean) => void;
}

export type Theme = "light" | "dark" | "high-contrast";

class AppState {
  info = $state<AppInfo>({ version: "", platform: "windows", selfTest: null, files: [] });
  tabs = $state<Tab[]>([]);
  activeDoc = $state<DocId | null>(null);
  mods = $state<ModUi[]>([]);
  /** The active tool: a mod ID and the action that started it. */
  tool = $state<{ modId: string; actionId: string; title: string } | null>(null);
  cursor = $state("default");
  overlays = $state<Record<string, Shape[]>>({});
  toasts = $state<Toast[]>([]);
  ask = $state<Ask | null>(null);
  theme = $state<Theme>("light");
  dpr = $state(window.devicePixelRatio || 1);
  liveMessage = $state("");
  private nextToast = 1;

  get tab(): Tab | undefined {
    return this.tabs.find((t) => t.doc.id === this.activeDoc);
  }

  get tileBase(): string {
    return schemeBase("trtile", this.info.platform);
  }

  get cssScale(): number {
    return (this.tab?.zoom ?? 1) * CSS_PER_POINT;
  }

  async start() {
    this.info = await api.appInfo();
    this.theme = loadTheme();
    window.matchMedia(`(resolution: ${this.dpr}dppx)`).addEventListener("change", () => {
      this.dpr = window.devicePixelRatio || 1;
      this.pushContext();
    });
    await on("document-changed", (e) => void this.refresh(e.doc));
    await on("overlay", (e) => this.setOverlay(e.modId, e.shapes));
    await on("notify", (e) => this.toast(e.level, e.message));
    await on("confirm-request", async (e) => {
      const yes = await this.confirm(e.title, e.message);
      await api.answerConfirm(e.id, yes);
    });
    await on("registry-changed", () => void this.loadRegistry());
    await this.loadRegistry();
    for (const f of this.info.files) await this.open(f);
    if (this.info.selfTest) {
      const { runSelfTest } = await import("./selftest");
      await runSelfTest(this.info.selfTest);
    }
  }

  async loadRegistry() {
    this.mods = await api.registry();
  }

  toast(level: Toast["level"], message: string) {
    const id = this.nextToast++;
    this.toasts.push({ id, level, message });
    this.liveMessage = message;
    setTimeout(() => this.dismiss(id), level === "error" ? 10000 : 5000);
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  fail(e: unknown) {
    this.toast("error", errorText(e));
  }

  confirm(title: string, message: string): Promise<boolean> {
    return new Promise((resolve) => {
      this.ask = {
        title,
        message,
        resolve: (yes) => {
          this.ask = null;
          resolve(yes);
        },
      };
    });
  }

  setOverlay(modId: string, shapes: Shape[]) {
    this.overlays[modId] = shapes;
    const labels = shapes.map((s) => s.label).filter(Boolean);
    if (labels.length) this.liveMessage = labels.join(", ");
  }

  pushContext() {
    const t = this.tab;
    void api.setContext({
      doc: t?.doc.id ?? null,
      activePage: t?.activePage ?? null,
      selectedPages: t ? (t.selected.length ? t.selected : t.activePage ? [t.activePage] : []) : [],
      zoom: t?.zoom ?? 1,
      devicePixelRatio: this.dpr,
    });
  }

  activate(doc: DocId) {
    this.activeDoc = doc;
    this.pushContext();
  }

  async open(path: string | null = null) {
    try {
      let info: DocInfo | null = null;
      try {
        info = await api.openFile(path);
      } catch (e) {
        if (errorCode(e) !== "password-required") throw e;
        const password = window.prompt(S.dialog.passwordMessage(path ?? ""));
        if (password === null) return;
        info = await api.openFile(path, password);
      }
      if (!info) return;
      this.tabs.push({
        doc: info,
        zoom: 1,
        activePage: info.pages[0]?.id ?? null,
        selected: [],
        scrollRequest: 0,
        scrollTarget: null,
      });
      this.activate(info.id);
    } catch (e) {
      this.fail(e);
    }
  }

  async refresh(doc: DocId) {
    const tab = this.tabs.find((t) => t.doc.id === doc);
    if (!tab) return;
    tab.doc = await api.document(doc);
    const ids = new Set(tab.doc.pages.map((p) => p.id));
    tab.selected = tab.selected.filter((p) => ids.has(p));
    if (!tab.activePage || !ids.has(tab.activePage)) tab.activePage = tab.doc.pages[0]?.id ?? null;
    this.pushContext();
  }

  async close(doc: DocId) {
    const tab = this.tabs.find((t) => t.doc.id === doc);
    if (!tab) return;
    let force = false;
    if (tab.doc.dirty) {
      force = await this.confirm(S.dialog.unsavedTitle, S.dialog.unsavedMessage(tab.doc.title));
      if (!force) return;
    }
    try {
      await api.closeDoc(doc, force);
    } catch (e) {
      this.fail(e);
      return;
    }
    const i = this.tabs.indexOf(tab);
    this.tabs.splice(i, 1);
    if (this.activeDoc === doc) this.activeDoc = this.tabs[Math.max(0, i - 1)]?.doc.id ?? null;
    this.pushContext();
  }

  async save(saveAs = false) {
    const tab = this.tab;
    if (!tab) return;
    try {
      let info: DocInfo | null;
      try {
        info = await api.save(tab.doc.id, saveAs, false);
      } catch (e) {
        if (errorCode(e) !== "signed-document") throw e;
        if (!(await this.confirm(S.dialog.signedTitle, S.dialog.signedMessage))) return;
        info = await api.save(tab.doc.id, saveAs, true);
      }
      if (info) tab.doc = info;
    } catch (e) {
      this.fail(e);
    }
  }

  async undo(redo = false) {
    const tab = this.tab;
    if (!tab) return;
    try {
      await api.undo(tab.doc.id, redo);
    } catch (e) {
      this.fail(e);
    }
  }

  setZoom(zoom: number) {
    const tab = this.tab;
    if (!tab) return;
    tab.zoom = Math.min(8, Math.max(0.1, zoom));
    this.pushContext();
  }

  zoomStep(dir: 1 | -1) {
    const z = this.tab?.zoom ?? 1;
    const next = dir > 0 ? ZOOMS.find((v) => v > z + 1e-6) : [...ZOOMS].reverse().find((v) => v < z - 1e-6);
    this.setZoom(next ?? z);
  }

  async selectTool(modId: string | null, actionId = "", title = "") {
    try {
      if (modId === null) {
        await api.selectTool(null);
        this.tool = null;
      } else {
        await api.runAction(modId, actionId);
        this.tool = { modId, actionId, title };
      }
      this.cursor = "default";
    } catch (e) {
      this.fail(e);
    }
  }

  async runAction(modId: string, actionId: string, title: string, tool: boolean) {
    if (tool) return this.selectTool(modId, actionId, title);
    try {
      await api.runAction(modId, actionId);
    } catch (e) {
      this.fail(e);
    }
  }

  setTheme(theme: Theme) {
    this.theme = theme;
    try {
      localStorage.setItem("theme", theme);
    } catch {
      // Storage can be blocked. The theme then lasts until the app closes.
    }
  }
}

function loadTheme(): Theme {
  try {
    const t = localStorage.getItem("theme");
    if (t === "light" || t === "dark" || t === "high-contrast") return t;
  } catch {
    // Use the system setting below.
  }
  if (window.matchMedia("(forced-colors: active)").matches) return "high-contrast";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export const app = new AppState();
