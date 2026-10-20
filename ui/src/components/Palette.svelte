<script lang="ts" module>
  export interface Command {
    id: string;
    title: string;
    /** The app or the name of the mod. */
    source: string;
    disabled?: boolean;
    run: () => void;
  }
</script>

<script lang="ts">
  // The command palette (Ctrl+K): all app and mod actions, with the WAI-ARIA combobox keys.
  import { app } from "../lib/store.svelte";
  import { filter } from "../lib/palette";
  import { S } from "../lib/strings";

  let { commands, onclose }: { commands: Command[]; onclose: () => void } = $props();

  let query = $state("");
  let active = $state(0);
  let list: HTMLUListElement | undefined = $state();
  const results = $derived(filter(commands, query, (c) => `${c.title} ${c.source}`));

  $effect(() => {
    void query;
    active = 0;
  });

  // Focus goes back to where it was when the palette closes.
  $effect(() => {
    const before = document.activeElement as HTMLElement | null;
    return () => before?.focus?.();
  });

  $effect(() => {
    list?.querySelector(`#palette-item-${active}`)?.scrollIntoView({ block: "nearest" });
  });

  function shortcut(c: Command): string {
    return app.keymap.byAction.get(c.id) ?? (app.keymap.notBound.includes(c.id) ? S.palette.notBound : "");
  }

  function run(c: Command | undefined) {
    if (!c || c.disabled) return;
    onclose();
    c.run();
  }

  function onKey(e: KeyboardEvent) {
    const n = results.length;
    if (e.key === "ArrowDown") active = n ? (active + 1) % n : 0;
    else if (e.key === "ArrowUp") active = n ? (active - 1 + n) % n : 0;
    else if (e.key === "Home") active = 0;
    else if (e.key === "End") active = Math.max(0, n - 1);
    else if (e.key === "Enter") run(results[active]);
    else if (e.key === "Escape") onclose();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="palette" role="dialog" aria-modal="true" aria-label={S.palette.label}>
    <!-- svelte-ignore a11y_autofocus -->
    <input
      autofocus
      role="combobox"
      aria-label={S.palette.input}
      aria-expanded="true"
      aria-controls="palette-list"
      aria-autocomplete="list"
      aria-activedescendant={results.length ? `palette-item-${active}` : undefined}
      placeholder={S.palette.input}
      bind:value={query}
      onkeydown={onKey}
    />
    <ul id="palette-list" role="listbox" aria-label={S.palette.label} bind:this={list}>
      {#each results as c, i (c.id)}
        <li
          id="palette-item-{i}"
          role="option"
          aria-selected={i === active}
          aria-disabled={c.disabled ? "true" : undefined}
          onclick={() => run(c)}
          onkeydown={() => {}}
          onpointermove={() => (active = i)}
        >
          <span class="title">{c.title}</span>
          <span class="source">{c.source}</span>
          <kbd>{shortcut(c)}</kbd>
        </li>
      {:else}
        <li class="empty" role="presentation">{S.palette.empty}</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    justify-items: center;
    align-items: start;
    padding-top: 12vh;
    background: rgba(0, 0, 0, 0.3);
  }
  .palette {
    width: min(560px, 90vw);
    background: var(--tr-surface);
    border: 1px solid var(--tr-border);
    border-radius: var(--tr-radius);
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.35);
    overflow: hidden;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    padding: 10px 12px;
    border: 0;
    border-bottom: 1px solid var(--tr-border);
    background: var(--tr-surface);
    color: var(--tr-text);
    font: inherit;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 4px;
    max-height: 50vh;
    overflow-y: auto;
  }
  li {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 12px;
    padding: 6px 10px;
    border-radius: 4px;
    cursor: pointer;
  }
  li[aria-selected="true"] {
    background: var(--tr-accent);
    color: var(--tr-accent-text);
  }
  li[aria-disabled="true"] {
    opacity: 0.5;
    cursor: default;
  }
  .source,
  kbd {
    font: inherit;
    opacity: 0.75;
  }
  .empty {
    display: block;
    color: var(--tr-text-muted);
    cursor: default;
  }
</style>
