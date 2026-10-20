<script lang="ts">
  import { onMount, untrack } from "svelte";
  import MenuBar, { type Item } from "./components/MenuBar.svelte";
  import PanelHost from "./components/PanelHost.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import Viewer from "./components/Viewer.svelte";
  import { key, registerTextInput, text } from "./lib/input";
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
  const commands = $derived(
    app.mods.flatMap((m) =>
      m.disabled ? [] : m.actions.filter((a) => !a.tool).map((a) => ({ modId: m.id, action: a })),
    ),
  );

  const THEMES: Theme[] = ["light", "dark", "high-contrast"];

  /** App actions. Step 8 moves the shortcuts into keymap.json. */
  const appActions = $derived({
    open: { label: S.menu.open, shortcut: "Ctrl+O", run: () => void app.open() },
    save: { label: S.menu.save, shortcut: "Ctrl+S", disabled: !app.tab, run: () => void app.save(false) },
    saveAs: { label: S.menu.saveAs, shortcut: "Ctrl+Shift+S", disabled: !app.tab, run: () => void app.save(true) },
    close: {
      label: S.menu.close,
      shortcut: "Ctrl+W",
      disabled: !app.tab,
      run: () => app.activeDoc !== null && void app.close(app.activeDoc),
    },
    undo: { label: S.menu.undo, shortcut: "Ctrl+Z", disabled: !app.tab?.doc.canUndo, run: () => void app.undo(false) },
    redo: { label: S.menu.redo, shortcut: "Ctrl+Y", disabled: !app.tab?.doc.canRedo, run: () => void app.undo(true) },
    zoomIn: { label: S.menu.zoomIn, shortcut: "Ctrl+=", disabled: !app.tab, run: () => app.zoomStep(1) },
    zoomOut: { label: S.menu.zoomOut, shortcut: "Ctrl+-", disabled: !app.tab, run: () => app.zoomStep(-1) },
    zoomReset: { label: S.menu.zoomReset, shortcut: "Ctrl+0", disabled: !app.tab, run: () => app.setZoom(1) },
    fitWidth: { label: S.menu.fitWidth, shortcut: null, disabled: !app.tab, run: () => viewer?.fitWidth() },
    theme: {
      label: S.menu.theme,
      shortcut: null,
      run: () => app.setTheme(THEMES[(THEMES.indexOf(app.theme) + 1) % THEMES.length]!),
    },
    sidebar: {
      label: S.menu.sidebar,
      shortcut: "F4",
      run: () => (app.leftOpen = !app.leftOpen),
    },
    panels: {
      label: S.menu.panels,
      shortcut: "F6",
      run: () => (app.rightOpen = !app.rightOpen),
    },
    noTool: { label: S.menu.noTool, shortcut: "Escape", run: () => void app.selectTool(null) },
  });

  const menus = $derived<{ label: string; items: Item[] }[]>([
    { label: S.menu.file, items: [appActions.open, appActions.save, appActions.saveAs, appActions.close] },
    { label: S.menu.edit, items: [appActions.undo, appActions.redo] },
    {
      label: S.menu.view,
      items: [
        appActions.zoomIn,
        appActions.zoomOut,
        appActions.zoomReset,
        appActions.fitWidth,
        appActions.sidebar,
        appActions.panels,
        appActions.theme,
      ],
    },
    {
      label: S.menu.tools,
      items: [
        appActions.noTool,
        ...tools.map((t) => ({
          label: t.action.title,
          shortcut: t.action.shortcut,
          run: () => void app.selectTool(t.modId, t.action.id, t.action.title),
        })),
        ...commands.map((c) => ({
          label: c.action.title,
          shortcut: c.action.shortcut,
          disabled: !app.tab,
          run: () => void app.runAction(c.modId, c.action.id, c.action.title, false),
        })),
      ],
    },
  ]);

  function matches(e: KeyboardEvent, shortcut: string): boolean {
    const parts = shortcut.split("+");
    const k = parts.pop()!.toLowerCase();
    const want = new Set(parts.map((p) => p.toLowerCase()));
    return (
      e.ctrlKey === want.has("ctrl") &&
      e.shiftKey === want.has("shift") &&
      e.altKey === want.has("alt") &&
      (e.key.toLowerCase() === k || (k === "=" && e.key === "+"))
    );
  }

  async function onKeyDown(e: KeyboardEvent) {
    if (app.ask) return;
    const all = [...Object.values(appActions)];
    const hit = all.find((a) => a.shortcut && a.shortcut !== "Escape" && matches(e, a.shortcut));
    if (hit) {
      e.preventDefault();
      if (!("disabled" in hit && hit.disabled)) hit.run();
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
