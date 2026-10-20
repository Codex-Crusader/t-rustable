<script lang="ts">
  import { onMount, untrack } from "svelte";
  import MenuBar, { type Item } from "./components/MenuBar.svelte";
  import Palette, { type Command } from "./components/Palette.svelte";
  import PanelHost from "./components/PanelHost.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Viewer from "./components/Viewer.svelte";
  import { key, registerTextInput, text } from "./lib/input";
  import { fromEvent } from "./lib/keymap";
  import { app, type Theme } from "./lib/store.svelte";
  import { S } from "./lib/strings";

  let viewer: Viewer | undefined = $state();
  let textArea: HTMLTextAreaElement | undefined = $state();

  onMount(() => {
    void app.start();
  });

  $effect(() => {
    registerTextInput(textArea ?? null);
  });

  $effect(() => {
    document.documentElement.dataset.theme = app.theme;
    // Panels get the new variables after the page has the new theme.
    untrack(() => void app.pushPanelTheme());
  });

  const tools = $derived(
    app.mods.flatMap((m) =>
      m.disabled ? [] : m.actions.filter((a) => a.tool).map((a) => ({ modId: m.id, action: a })),
    ),
  );

  const THEMES: Theme[] = ["light", "dark", "high-contrast"];

  /** App actions. Their IDs are the keys in keymap.json. */
  const appActions = $derived<Command[]>([
    { id: "app.open", title: S.menu.open, run: () => void app.open() },
    { id: "app.save", title: S.menu.save, disabled: !app.tab, run: () => void app.save(false) },
    { id: "app.save-as", title: S.menu.saveAs, disabled: !app.tab, run: () => void app.save(true) },
    {
      id: "app.close",
      title: S.menu.close,
      disabled: !app.tab,
      run: () => app.activeDoc !== null && void app.close(app.activeDoc),
    },
    { id: "app.undo", title: S.menu.undo, disabled: !app.tab?.doc.canUndo, run: () => void app.undo(false) },
    { id: "app.redo", title: S.menu.redo, disabled: !app.tab?.doc.canRedo, run: () => void app.undo(true) },
    { id: "app.zoom-in", title: S.menu.zoomIn, disabled: !app.tab, run: () => app.zoomStep(1) },
    { id: "app.zoom-out", title: S.menu.zoomOut, disabled: !app.tab, run: () => app.zoomStep(-1) },
    { id: "app.zoom-reset", title: S.menu.zoomReset, disabled: !app.tab, run: () => app.setZoom(1) },
    { id: "app.fit-width", title: S.menu.fitWidth, disabled: !app.tab, run: () => viewer?.fitWidth() },
    { id: "app.sidebar", title: S.menu.sidebar, run: () => (app.leftOpen = !app.leftOpen) },
    { id: "app.panels", title: S.menu.panels, run: () => (app.rightOpen = !app.rightOpen) },
    {
      id: "app.theme",
      title: S.menu.theme,
      run: () => app.setTheme(THEMES[(THEMES.indexOf(app.theme) + 1) % THEMES.length]!),
    },
    { id: "app.palette", title: S.menu.palette, run: () => (app.paletteOpen = true) },
    { id: "app.no-tool", title: S.menu.noTool, run: () => void app.selectTool(null) },
  ].map((a) => ({ ...a, source: S.palette.app })));

  /** Mod actions. A tool action selects the tool, another action runs once. */
  const modActions = $derived<Command[]>(
    app.mods.flatMap((m) =>
      m.disabled
        ? []
        : m.actions.map((a) => ({
            id: `${m.id}/${a.id}`,
            title: a.title,
            source: m.name,
            disabled: !a.tool && !app.tab,
            run: () => void app.runAction(m.id, a.id, a.title, a.tool),
          })),
    ),
  );

  const actions = $derived(new Map([...appActions, ...modActions].map((a) => [a.id, a])));

  function item(id: string): Item {
    const a = actions.get(id)!;
    const shortcut = id === "app.no-tool" ? "Escape" : (app.keymap.byAction.get(id) ?? null);
    return { label: a.title, shortcut, disabled: a.disabled, run: a.run };
  }

  const menus = $derived<{ label: string; items: Item[] }[]>([
    { label: S.menu.file, items: ["app.open", "app.save", "app.save-as", "app.close"].map(item) },
    { label: S.menu.edit, items: ["app.undo", "app.redo", "app.palette"].map(item) },
    {
      label: S.menu.view,
      items: [
        "app.zoom-in",
        "app.zoom-out",
        "app.zoom-reset",
        "app.fit-width",
        "app.sidebar",
        "app.panels",
        "app.theme",
      ].map(item),
    },
    { label: S.menu.tools, items: ["app.no-tool", ...modActions.map((a) => a.id)].map(item) },
  ]);

  /** Shortcuts that a text field keeps for itself. */
  const TEXT_KEYS = new Set(["Ctrl+Z", "Ctrl+Y", "Ctrl+A", "Ctrl+C", "Ctrl+V", "Ctrl+X"]);

  function isTextField(t: EventTarget | null): boolean {
    return t !== textArea && (t instanceof HTMLTextAreaElement || (t instanceof HTMLInputElement && t.type !== "checkbox"));
  }

  async function onKeyDown(e: KeyboardEvent) {
    if (app.ask || app.paletteOpen) return;
    const k = fromEvent(e);
    const id = k ? app.keymap.byKey.get(k) : undefined;
    const action = id ? actions.get(id) : undefined;
    if (action && !(isTextField(e.target) && TEXT_KEYS.has(k!))) {
      e.preventDefault();
      if (!action.disabled) action.run();
      return;
    }
    if (app.tool && (e.target === textArea || e.target === document.body)) {
      const handled = await key(e);
      if (!handled && e.key === "Escape") void app.selectTool(null);
    }
  }

  function onKeyUp(e: KeyboardEvent) {
    if (app.tool && e.target === textArea) void key(e);
  }

  function onInput() {
    if (!textArea) return;
    const value = textArea.value;
    textArea.value = "";
    void text(value);
  }
</script>

<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} />

<div class="shell">
  <MenuBar {menus} />

  <div class="toolbar" role="toolbar" aria-label={S.menu.tools}>
    <button aria-pressed={app.tool === null} onclick={() => void app.selectTool(null)}>{S.menu.noTool}</button>
    {#each tools as t (`${t.modId}/${t.action.id}`)}
      <button
        aria-pressed={app.tool?.modId === t.modId && app.tool.actionId === t.action.id}
        title={t.action.description}
        onclick={() => void app.selectTool(t.modId, t.action.id, t.action.title)}>{t.action.title}</button
      >
    {/each}
  </div>

  <div class="tabs" role="tablist" aria-label={S.tabs.label}>
    {#each app.tabs as t (t.doc.id)}
      <div class="tab" class:active={t.doc.id === app.activeDoc}>
        <button
          role="tab"
          aria-selected={t.doc.id === app.activeDoc}
          onclick={() => app.activate(t.doc.id)}
          title={t.doc.path ?? t.doc.title}
          >{t.doc.title}{#if t.doc.dirty}<span aria-label={S.tabs.unsaved}> *</span>{/if}</button
        >
        <button class="close" aria-label={S.tabs.close(t.doc.title)} onclick={() => void app.close(t.doc.id)}
          >×</button
        >
      </div>
    {/each}
  </div>

  <main class="main">
    {#if app.tab}
      {#if app.leftOpen}
        <Sidebar tab={app.tab} />
      {/if}
      <div class="center">
        {#key app.tab.doc.id}
          <Viewer tab={app.tab} bind:this={viewer} />
        {/key}
      </div>
    {:else}
      <div class="empty">
        <p>{S.status.noDocument}</p>
        <button onclick={() => void app.open()}>{S.menu.open}</button>
      </div>
    {/if}
    {#if app.rightOpen}
      <PanelHost />
    {/if}
  </main>

  <footer class="status" aria-live="off">
    {#if app.tab}
      {@const t = app.tab}
      {@const index = t.doc.pages.findIndex((p) => p.id === t.activePage)}
      <span>{S.status.page(index + 1, t.doc.pages.length)}</span>
      <span>{S.status.zoom(Math.round(t.zoom * 100))}</span>
    {/if}
    <span>{app.tool ? S.status.tool(app.tool.title) : S.status.noTool}</span>
  </footer>
</div>

<textarea
  class="tool-input"
  bind:this={textArea}
  oninput={(e) => !(e instanceof InputEvent && e.isComposing) && onInput()}
  oncompositionend={onInput}
  aria-label={app.tool ? S.status.tool(app.tool.title) : S.status.noTool}
  tabindex="-1"
></textarea>

<div class="toasts" role="status" aria-live="polite">
  {#each app.toasts as toast (toast.id)}
    <div class="toast {toast.level}">
      <span>{toast.message}</span>
      <button aria-label={S.dialog.ok} onclick={() => app.dismiss(toast.id)}>×</button>
    </div>
  {/each}
</div>

<div class="visually-hidden" aria-live="polite">{app.liveMessage}</div>

{#if app.paletteOpen}
  <Palette commands={[...actions.values()]} onclose={() => (app.paletteOpen = false)} />
{/if}

{#if app.ask}
  {@const ask = app.ask}
  <div class="backdrop">
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="ask-title" aria-describedby="ask-text">
      <h2 id="ask-title">{ask.title}</h2>
      <p id="ask-text">{ask.message}</p>
      <div class="buttons">
        <!-- svelte-ignore a11y_autofocus -->
        <button autofocus onclick={() => ask.resolve(false)}>{S.dialog.no}</button>
        <button class="primary" onclick={() => ask.resolve(true)}>{S.dialog.yes}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .shell {
    display: grid;
    grid-template-rows: auto auto auto 1fr auto;
    height: 100%;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px 6px;
    background: var(--tr-surface);
    border-bottom: 1px solid var(--tr-border);
  }
  .toolbar button,
  .empty button,
  .buttons button {
    padding: 4px 10px;
    border: 1px solid var(--tr-border);
    border-radius: var(--tr-radius);
    background: var(--tr-surface-2);
  }
  .toolbar button[aria-pressed="true"],
  .buttons .primary {
    background: var(--tr-accent);
    color: var(--tr-accent-text);
    border-color: var(--tr-accent);
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 4px 6px 0;
    background: var(--tr-bg);
    border-bottom: 1px solid var(--tr-border);
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    border: 1px solid var(--tr-border);
    border-bottom: 0;
    border-radius: var(--tr-radius) var(--tr-radius) 0 0;
    background: var(--tr-surface-2);
  }
  .tab.active {
    background: var(--tr-surface);
    font-weight: 700;
  }
  .tab button {
    background: none;
    border: 0;
    padding: 5px 8px;
    white-space: nowrap;
  }
  .main {
    display: flex;
    min-height: 0;
    position: relative;
  }
  .center {
    flex: 1;
    min-width: 0;
  }
  .empty {
    flex: 1;
    display: grid;
    place-content: center;
    gap: 12px;
    height: 100%;
    text-align: center;
    color: var(--tr-text-muted);
  }
  .status {
    display: flex;
    gap: 16px;
    padding: 3px 10px;
    background: var(--tr-surface);
    border-top: 1px solid var(--tr-border);
    color: var(--tr-text-muted);
  }
  .tool-input {
    position: fixed;
    left: -1000px;
    top: 0;
    width: 10px;
    height: 10px;
    opacity: 0;
  }
  .toasts {
    position: fixed;
    right: 12px;
    bottom: 36px;
    display: grid;
    gap: 6px;
    max-width: 420px;
    z-index: 40;
  }
  .toast {
    display: flex;
    gap: 10px;
    align-items: start;
    padding: 8px 10px;
    border-radius: var(--tr-radius);
    background: var(--tr-surface);
    border: 1px solid var(--tr-border);
    border-left: 4px solid var(--tr-accent);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.25);
  }
  .toast.error {
    border-left-color: var(--tr-danger);
  }
  .toast.warning {
    border-left-color: var(--tr-warning);
  }
  .toast.success {
    border-left-color: var(--tr-success);
  }
  .toast button {
    background: none;
    border: 0;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.4);
  }
  .dialog {
    max-width: 480px;
    padding: 16px 20px;
    border-radius: var(--tr-radius);
    background: var(--tr-surface);
    border: 1px solid var(--tr-border);
  }
  .dialog h2 {
    margin: 0 0 8px;
    font-size: 1.15em;
  }
  .buttons {
    display: flex;
    justify-content: end;
    gap: 8px;
  }
</style>
