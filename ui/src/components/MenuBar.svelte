<script lang="ts">
  // A menu bar with the WAI-ARIA menu pattern: arrow keys move, Escape closes.
  export interface Item {
    label: string;
    shortcut?: string | null;
    disabled?: boolean;
    run: () => void;
  }

  let { menus }: { menus: { label: string; items: Item[] }[] } = $props();

  let open = $state(-1);
  let bar: HTMLDivElement | undefined = $state();

  function focusItem(menu: number, index: number) {
    queueMicrotask(() => {
      const items = bar?.querySelectorAll<HTMLButtonElement>(`[data-menu="${menu}"] [role="menuitem"]`);
      if (!items?.length) return;
      items[(index + items.length) % items.length]!.focus();
    });
  }

  function toggle(i: number) {
    open = open === i ? -1 : i;
    if (open >= 0) focusItem(i, 0);
  }

  function onKey(e: KeyboardEvent, menu: number) {
    const items = [...(bar?.querySelectorAll<HTMLButtonElement>(`[data-menu="${menu}"] [role="menuitem"]`) ?? [])];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown") focusItem(menu, at + 1);
    else if (e.key === "ArrowUp") focusItem(menu, at - 1);
    else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      const next = (menu + (e.key === "ArrowRight" ? 1 : -1) + menus.length) % menus.length;
      open = next;
      focusItem(next, 0);
    } else if (e.key === "Escape") {
      open = -1;
      bar?.querySelectorAll<HTMLButtonElement>(".top")[menu]?.focus();
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function run(item: Item) {
    open = -1;
    item.run();
  }
</script>

<svelte:window onclick={(e) => !bar?.contains(e.target as Node) && (open = -1)} />

<div class="bar" role="menubar" tabindex="-1" bind:this={bar}>
  {#each menus as menu, i (menu.label)}
    <div class="menu" data-menu={i}>
      <button
        class="top"
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={open === i}
        onclick={() => toggle(i)}
        onkeydown={(e) => {
          if (e.key === "ArrowDown") {
            open = i;
            focusItem(i, 0);
            e.preventDefault();
          }
        }}>{menu.label}</button
      >
      {#if open === i}
        <div class="list" role="menu" tabindex="-1" aria-label={menu.label} onkeydown={(e) => onKey(e, i)}>
          {#each menu.items as item (item.label)}
            <button role="menuitem" disabled={item.disabled} onclick={() => run(item)}>
              <span>{item.label}</span>
              {#if item.shortcut}<kbd>{item.shortcut}</kbd>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .bar {
    display: flex;
    gap: 2px;
    padding: 2px 6px;
    background: var(--tr-surface);
    border-bottom: 1px solid var(--tr-border);
  }
  .menu {
    position: relative;
  }
  .top {
    background: none;
    border: 0;
    padding: 4px 10px;
    border-radius: var(--tr-radius);
  }
  .top:hover,
  .top[aria-expanded="true"] {
    background: var(--tr-surface-2);
  }
  .list {
    position: absolute;
    z-index: 30;
    top: 100%;
    left: 0;
    min-width: 240px;
    padding: 4px;
    background: var(--tr-surface);
    border: 1px solid var(--tr-border);
    border-radius: var(--tr-radius);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }
  .list button {
    display: flex;
    justify-content: space-between;
    gap: 24px;
    width: 100%;
    padding: 5px 10px;
    background: none;
    border: 0;
    border-radius: 4px;
    text-align: left;
  }
  .list button:hover:not(:disabled),
  .list button:focus-visible {
    background: var(--tr-accent);
    color: var(--tr-accent-text);
  }
  .list button:disabled {
    opacity: 0.5;
  }
  kbd {
    font: inherit;
    opacity: 0.75;
  }
</style>
