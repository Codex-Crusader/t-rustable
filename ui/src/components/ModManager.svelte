<script lang="ts">
  // The mod manager: each mod with its source, state, and permissions. Each button names
  // its mod for screen readers.
  import { onMount } from "svelte";
  import { app } from "../lib/store.svelte";
  import { S } from "../lib/strings";
  import type { ModRow } from "../lib/types";

  let { onclose }: { onclose: () => void } = $props();

  let dialog: HTMLDivElement | undefined = $state();
  let opener: Element | null = null;

  onMount(() => {
    opener = document.activeElement;
    dialog?.querySelector<HTMLElement>("button, input")?.focus();
    return () => (opener as HTMLElement | null)?.focus?.();
  });

  const order = { builtin: 0, developer: 1, installed: 2 };
  const rows = $derived(
    [...app.modRows].sort((a, b) => order[a.source] - order[b.source] || a.name.localeCompare(b.name)),
  );

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !app.ask) {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "Tab" && dialog) {
      // Keep the focus in the dialog.
      const items = [...dialog.querySelectorAll<HTMLElement>("button:not(:disabled), input")];
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        last?.focus();
        e.preventDefault();
      } else if (!e.shiftKey && document.activeElement === last) {
        first?.focus();
        e.preventDefault();
      }
    }
  }

  const on = (r: ModRow) => r.state === "on";
</script>

<div class="backdrop">
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="manager-title"
    tabindex="-1"
    bind:this={dialog}
    onkeydown={onKey}
  >
    <header>
      <h2 id="manager-title">{S.manager.title}</h2>
      <button onclick={onclose}>{S.manager.close}</button>
    </header>

    <div class="tools">
      <label title={S.manager.developerHelp}>
        <input type="checkbox" checked={app.developer} onchange={(e) => void app.setDeveloper(e.currentTarget.checked)} />
        {S.manager.developer}
      </label>
      <button onclick={() => void app.install()}>{S.menu.installMod}</button>
      {#if app.developer}
        <button onclick={() => void app.addDevFolder()}>{S.manager.addFolder}</button>
      {/if}
    </div>

    {#if rows.length}
      <ul class="rows">
        {#each rows as r (r.key)}
          <li class="row" data-mod={r.key}>
            <div class="head">
              <span class="name">{r.name}</span>
              {#if r.version}<span class="muted">{r.version}</span>{/if}
              <span class="tag {r.source}">{S.manager.source[r.source]}</span>
              <span class="state {r.state}">{S.manager.state[r.state]}</span>
            </div>
            {#if r.author || r.id}
              <div class="muted">{r.author}{r.author && r.id ? ". " : ""}{r.id ?? ""}</div>
            {/if}
            {#if r.folder}<div class="muted path">{r.folder}</div>{/if}
            {#if r.error}<div class="error" role="note">{r.error}</div>{/if}
            <div class="muted">
              {S.manager.permissions}: {r.permissions.length ? r.permissions.join(", ") : S.manager.noPermissions}
            </div>
            <div class="buttons">
              {#if r.id && r.state !== "failed"}
                <button
                  aria-label={on(r) ? S.manager.turnOff(r.name) : S.manager.turnOn(r.name)}
                  onclick={() => void app.setModOn(r, !on(r))}>{on(r) ? S.manager.off : S.manager.on}</button
                >
              {/if}
              {#if r.source === "installed"}
                <button aria-label={S.manager.remove(r.name)} onclick={() => void app.removeMod(r)}
                  >{S.manager.removeShort}</button
                >
              {/if}
              {#if r.source === "developer"}
                <button aria-label={S.manager.reload(r.name)} onclick={() => void app.reloadMod(r)}
                  >{S.manager.reloadShort}</button
                >
                <button aria-label={S.manager.removeFolder(r.name)} onclick={() => void app.removeDevFolder(r)}
                  >{S.manager.removeFolderShort}</button
                >
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <p>{S.manager.empty}</p>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.4);
  }
  .dialog {
    display: grid;
    grid-template-rows: auto auto 1fr;
    width: min(720px, 92vw);
    max-height: 86vh;
    padding: 12px 16px;
    border-radius: var(--tr-radius);
    background: var(--tr-surface);
    border: 1px solid var(--tr-border);
  }
  header,
  .tools {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .tools {
    justify-content: start;
  }
  h2 {
    margin: 0;
    font-size: 1.15em;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: auto;
  }
  .row {
    padding: 8px 4px;
    border-top: 1px solid var(--tr-border);
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: baseline;
  }
  .name {
    font-weight: 700;
  }
  .muted {
    color: var(--tr-text-muted);
  }
  .path {
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
    overflow-wrap: anywhere;
  }
  .tag,
  .state {
    padding: 0 6px;
    border-radius: 3px;
    border: 1px solid var(--tr-border);
    font-size: 0.85em;
  }
  .tag.developer {
    border: 2px solid var(--tr-warning);
    font-weight: 700;
  }
  .state.stopped,
  .state.failed {
    border-color: var(--tr-danger);
    color: var(--tr-danger);
  }
  .state.off {
    color: var(--tr-text-muted);
  }
  .error {
    margin: 4px 0;
    padding-left: 8px;
    border-left: 4px solid var(--tr-danger);
    overflow-wrap: anywhere;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  button {
    padding: 4px 10px;
    border: 1px solid var(--tr-border);
    border-radius: var(--tr-radius);
    background: var(--tr-surface-2);
  }
</style>
