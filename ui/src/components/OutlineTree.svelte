<script lang="ts">
  import { app } from "../lib/store.svelte";
  import type { OutlineItem } from "../lib/types";
  import OutlineTree from "./OutlineTree.svelte";

  let { items }: { items: OutlineItem[] } = $props();
</script>

<ul>
  {#each items as item, i (i)}
    <li>
      {#if item.children.length}
        <details open>
          <summary>
            <button disabled={!item.page} onclick={() => item.page && app.goTo(item.page)}>{item.title}</button>
          </summary>
          <OutlineTree items={item.children} />
        </details>
      {:else}
        <button disabled={!item.page} onclick={() => item.page && app.goTo(item.page)}>{item.title}</button>
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding-left: 14px;
  }
  button {
    background: none;
    border: 0;
    padding: 3px 4px;
    text-align: left;
    border-radius: 4px;
  }
  button:hover:not(:disabled) {
    background: var(--tr-surface-2);
  }
  button:disabled {
    color: var(--tr-text-muted);
  }
  summary {
    cursor: pointer;
  }
</style>
