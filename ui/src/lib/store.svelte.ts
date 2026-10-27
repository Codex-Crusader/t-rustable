// App state. Components read it, and they change it only through these methods.

import { api, errorCode, errorText, on, type AppInfo } from "./api";
import { buildKeymap, DEFAULT_KEYS, type Keymap } from "./keymap";
import { S } from "./strings";
import { CSS_PER_POINT, schemeBase } from "./tiles";
import type { DocId, DocInfo, Hit, ModRow, ModUi, Offer, PageId, Shape } from "./types";

/** A mod that sends many messages must not fill the window. */
const MAX_TOASTS = 5;

export const ZOOMS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];

export interface Tab {
  doc: DocInfo;
  zoom: number;
  activePage: PageId | null;
  selected: PageId[];
  /** Increments to ask the viewer to scroll to `scrollTarget`. */
  scrollRequest: number;
  scrollTarget: PageId | null;
  /** The anchor of Shift selection in the thumbnails. */
  anchor: PageId | null;
  hits: Hit[];
  activeHit: number;
}

/** The theme variables that panels get in their theme file. */
const PANEL_VARS = [
  "--tr-bg",
  "--tr-surface",
  "--tr-surface-2",
  "--tr-text",
  "--tr-text-muted",
  "--tr-border",
  "--tr-accent",
  "--tr-accent-text",
  "--tr-focus",
  "--tr-danger",
  "--tr-warning",
  "--tr-success",
  "--tr-radius",
  "--tr-font-size",
];

export interface Toast {
  id: number;
  level: "info" | "success" | "warning" | "error";
  message: string;
}

export interface Ask {
  title: string;
  message: string;
  /** Ask for a password. The answer is the text, or null after Cancel. */
  password?: boolean;
  /** A package to install. The dialog shows its permissions. */
  offer?: Offer;
  resolve: (yes: boolean, value?: string) => void;
}

export type Theme = "light" | "dark" | "high-contrast";

class AppState {
  info = $state<AppInfo>({ version: "", platform: "windows", selfTest: null, files: [], packages: [] });
  tabs = $state<Tab[]>([]);
  activeDoc = $state<DocId | null>(null);
  mods = $state<ModUi[]>([]);
  /** All mods with their source and state, for the mod manager. */
  modRows = $state<ModRow[]>([]);
  developer = $state(false);
  managerOpen = $state(false);
  /** Increments for a mod after its developer folder loaded again, so its panels load again. */
  panelReloads = $state<Record<string, number>>({});
  /** The active tool: a mod ID and the action that started it. */
  tool = $state<{ modId: string; actionId: string; title: string } | null>(null);
  cursor = $state("default");
  overlays = $state<Record<string, Shape[]>>({});
  toasts = $state<Toast[]>([]);
  /** The question on screen. Others wait in `asks`. */
  ask = $state<Ask | null>(null);
  private asks: Ask[] = [];
  theme = $state<Theme>("light");
  dpr = $state(window.devicePixelRatio || 1);
  liveMessage = $state("");
  /** Documents recovered at start. The self-test reports it. */
  recovered = $state(0);
  keymap = $state<Keymap>(buildKeymap(DEFAULT_KEYS, null, {}));
  paletteOpen = $state(false);
  leftOpen = $state(true);
  leftView = $state<"pages" | "outline" | "search">("pages");
  rightOpen = $state(true);
  /** "<mod>/<panel>" of the panel in the right side bar. */
  activePanel = $state<string | null>(null);
  /** Increments after the panel theme file changes, so panels load again. */
  panelTheme = $state(0);
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
    await on("mod-reloaded", (id) => (this.panelReloads[id] = (this.panelReloads[id] ?? 0) + 1));
    await on("close-requested", () => void this.quit());
    await this.loadRegistry();
    this.developer = await api.developerMode().catch(() => false);
    await this.offerRecovery();
    for (const f of this.info.files) await this.open(f);
    for (const p of this.info.packages) await this.install(p);
    if (this.info.selfTest) {
      const { runSelfTest } = await import("./selftest");
      await runSelfTest(this.info.selfTest);
    }
  }

  async loadRegistry() {
    this.mods = await api.registry();
    this.modRows = await api.modList();
    // The Rust side already stopped the tool of a mod that is gone.
    if (this.tool && !this.mods.some((m) => m.id === this.tool!.modId)) {
      this.tool = null;
      this.cursor = "default";
    }
    await this.loadKeymap();
  }

  /** Offers a `.tmod` package for install. Without a path, the user picks one. */
  async install(path: string | null = null) {
    try {
      const offer = await api.installOffer(path);
      if (!offer) return;
      const title = offer.previousVersion ? S.install.updateTitle(offer.name) : S.install.title(offer.name);
      const yes = await new Promise<boolean>((resolve) =>
        this.enqueue({ title, message: offer.description, offer }, (y) => resolve(y)),
      );
      const id = await api.installAnswer(offer.token, yes);
      if (id) this.toast("success", offer.previousVersion ? S.install.updated(offer.name) : S.install.done(offer.name));
    } catch (e) {
      this.fail(e);
    }
  }

  async setModOn(row: ModRow, on: boolean) {
    try {
      await api.modSetOn(row.key, on);
    } catch (e) {
      this.fail(e);
    }
  }

  async removeMod(row: ModRow) {
    if (!row.id || !(await this.confirm(S.manager.removeTitle, S.manager.removeMessage(row.name)))) return;
    try {
      await api.modRemove(row.id);
    } catch (e) {
      this.fail(e);
    }
  }

  async reloadMod(row: ModRow) {
    try {
      await api.modReload(row.key);
    } catch (e) {
      this.fail(e);
    }
  }

  async setDeveloper(on: boolean) {
    try {
      this.developer = await api.developerMode(on);
    } catch (e) {
      this.fail(e);
    }
  }

  async addDevFolder() {
    try {
      if (await api.devFolderAdd()) this.developer = true;
    } catch (e) {
      this.fail(e);
    }
  }

  async removeDevFolder(row: ModRow) {
    if (!row.folder) return;
    try {
      await api.devFolderRemove(row.folder);
    } catch (e) {
      this.fail(e);
    }
  }

  /** Builds the shortcuts from the defaults, keymap.json, and the mod suggestions. */
  async loadKeymap() {
    let text: string | null = null;
    try {
      text = await api.readKeymap();
    } catch (e) {
      this.fail(e);
    }
    const suggestions: Record<string, string> = {};
    for (const m of this.mods) {
      for (const a of m.actions) if (a.shortcut && !m.disabled) suggestions[`${m.id}/${a.id}`] = a.shortcut;
    }
    this.keymap = buildKeymap(DEFAULT_KEYS, text, suggestions);
    if (this.keymap.problems.length) {
      this.toast("warning", `${S.keymap.problems(this.keymap.problems.length)} ${this.keymap.problems.join(" ")}`);
    }
  }

  toast(level: Toast["level"], message: string) {
    const id = this.nextToast++;
    this.toasts.push({ id, level, message });
    if (this.toasts.length > MAX_TOASTS) this.toasts.shift();
    this.liveMessage = message;
    setTimeout(() => this.dismiss(id), level === "error" ? 10000 : 5000);
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  fail(e: unknown) {
    this.toast("error", errorText(e));
  }

  /**
   * Shows a question after the questions before it. A mod `confirm` waits in Rust until
   * it gets its answer, so a question must never replace another one.
   */
  private enqueue(question: Omit<Ask, "resolve">, done: (yes: boolean, value?: string) => void) {
    const item: Ask = {
      ...question,
      resolve: (yes, value) => {
        this.asks.shift();
        this.ask = this.asks[0] ?? null;
        done(yes, value);
      },
    };
    this.asks.push(item);
    if (this.asks.length === 1) this.ask = item;
  }

  confirm(title: string, message: string): Promise<boolean> {
    return new Promise((resolve) => this.enqueue({ title, message }, (yes) => resolve(yes)));
  }

  askPassword(name: string): Promise<string | null> {
    return new Promise((resolve) =>
      this.enqueue(
        { title: S.dialog.passwordTitle, message: S.dialog.passwordMessage(name), password: true },
        (yes, value) => resolve(yes ? (value ?? "") : null),
      ),
    );
  }

  /** Offers to recover the sessions that did not close. The self-test recovers them. */
  async offerRecovery() {
    let list: { id: string; title: string }[] = [];
    try {
      list = await api.unclosed();
    } catch (e) {
      this.fail(e);
      return;
    }
    if (!list.length) return;
    const titles = list.map((u) => u.title);
    if (!this.info.selfTest && !(await this.confirm(S.recovery.title, S.recovery.message(titles)))) {
      if (await this.confirm(S.recovery.discardTitle, S.recovery.discardMessage)) {
        for (const u of list) await api.discardUnclosed(u.id).catch((e) => this.fail(e));
      }
      return;
    }
    for (const u of list) {
      try {
        let info: DocInfo;
        try {
          info = await api.recover(u.id);
        } catch (e) {
          if (errorCode(e) !== "password-required") throw e;
          const password = await this.askPassword(u.title);
          if (password === null) continue;
          info = await api.recover(u.id, password);
        }
        this.addTab(info);
        this.recovered++;
      } catch (e) {
        this.fail(e);
      }
    }
  }

  /** Asks about unsaved changes, then closes all sessions and stops the app. */
  async quit() {
    const dirty = this.tabs.filter((t) => t.doc.dirty).map((t) => t.doc.title);
    if (dirty.length && !(await this.confirm(S.dialog.quitTitle, S.dialog.quitMessage(dirty)))) return;
    try {
      await api.quit();
    } catch (e) {
      this.fail(e);
    }
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
        const name = path?.split(/[\\/]/).pop() ?? "";
        const password = await this.askPassword(name);
        if (password === null) return;
        info = await api.openFile(path, password);
      }
      if (info) this.addTab(info);
    } catch (e) {
      this.fail(e);
    }
  }

  addTab(info: DocInfo) {
    this.tabs.push({
      doc: info,
      zoom: 1,
      activePage: info.pages[0]?.id ?? null,
      selected: [],
      scrollRequest: 0,
      scrollTarget: null,
      anchor: null,
      hits: [],
      activeHit: -1,
    });
    this.activate(info.id);
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

  /** Scrolls the viewer to a page and makes it the active page. */
  goTo(page: PageId) {
    const tab = this.tab;
    if (!tab) return;
    tab.activePage = page;
    tab.scrollTarget = page;
    tab.scrollRequest++;
    this.pushContext();
  }

  /** Page selection with the thumbnail rules: a click, Ctrl to toggle, Shift for a range. */
  selectPage(page: PageId, mode: "only" | "toggle" | "range") {
    const tab = this.tab;
    if (!tab) return;
    const ids = tab.doc.pages.map((p) => p.id);
    if (mode === "toggle") {
      tab.selected = tab.selected.includes(page) ? tab.selected.filter((p) => p !== page) : [...tab.selected, page];
      tab.anchor = page;
    } else if (mode === "range" && tab.anchor && ids.includes(tab.anchor)) {
      const [a, b] = [ids.indexOf(tab.anchor), ids.indexOf(page)].sort((x, y) => x - y);
      tab.selected = ids.slice(a!, b! + 1);
    } else {
      tab.selected = [page];
      tab.anchor = page;
    }
    this.goTo(page);
  }

  async search(query: string, matchCase: boolean, wholeWord: boolean) {
    const tab = this.tab;
    if (!tab) return;
    try {
      tab.hits = query ? await api.search(tab.doc.id, query, matchCase, wholeWord) : [];
      tab.activeHit = tab.hits.length ? 0 : -1;
      this.liveMessage = tab.hits.length ? S.sidebar.hits(tab.hits.length) : S.sidebar.noHits;
      if (tab.hits[0]) this.goTo(tab.hits[0].page);
    } catch (e) {
      this.fail(e);
    }
  }

  /** Sends the current theme variables to the panel theme file. */
  async pushPanelTheme() {
    const style = getComputedStyle(document.documentElement);
    const vars = PANEL_VARS.map((v) => `  ${v}: ${style.getPropertyValue(v).trim()};`).join("\n");
    const css = `:root {\n${vars}\n  color-scheme: ${this.theme === "light" ? "light" : "dark"};\n}\n`;
    try {
      await api.setPanelTheme(css);
      this.panelTheme++;
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
