<script lang="ts">
  import { api } from "../lib/api";
  import { app, type Tab } from "../lib/store.svelte";
  import { S } from "../lib/strings";
  import type { OutlineItem } from "../lib/types";
  import OutlineTree from "./OutlineTree.svelte";
  import Thumbnails from "./Thumbnails.svelte";

  let { tab }: { tab: Tab } = $props();

  const VIEWS = [
    { id: "pages", label: S.sidebar.pages },
    { id: "outline", label: S.sidebar.outline },
    { id: "search", label: S.sidebar.search },
  ] as const;

  let outline: OutlineItem[] | null = $state(null);
  let outlineFor = -1;
  $effect(() => {
    const doc = tab.doc.id;
    const structure = tab.doc.structure;
    if (app.leftView !== "outline" || outlineFor === doc * 1e6 + structure) return;
    outlineFor = doc * 1e6 + structure;
    api.outline(doc).then(
      (items) => (outline = items),
      (e) => app.fail(e),
    );
  });

  let query = $state("");
  let matchCase = $state(false);
  let wholeWord = $state(false);

  function onTabKey(e: KeyboardEvent, i: number) {
    const next = e.key === "ArrowRight" ? i + 1 : e.key === "ArrowLeft" ? i - 1 : -99;
    if (next === -99) return;
    const v = VIEWS[(next + VIEWS.length) % VIEWS.length]!;
    app.leftView = v.id;
    document.getElementById(`side-tab-${v.id}`)?.focus();
    e.preventDefault();
  }
</script>

<aside class="side" aria-label={S.sidebar.label}>
  <div class="views" role="tablist">
    {#each VIEWS as v, i (v.id)}
      <button
        id="side-tab-{v.id}"
        role="tab"
        aria-selected={app.leftView === v.id}
        aria-controls="side-view"
        tabindex={app.leftView === v.id ? 0 : -1}
        onclick={() => (app.leftView = v.id)}
        onkeydown={(e) => onTabKey(e, i)}>{v.label}</button
      >
    {/each}
  </div>
  <div id="side-view" class="view" role="tabpanel" aria-labelledby="side-tab-{app.leftView}">
    {#if app.leftView === "pages"}
      <Thumbnails {tab} />
    {:else if app.leftView === "outline"}
      <div class="scroll">
        {#if outline?.length}
          <OutlineTree items={outline} />
        {:else if outline}
          <p class="muted">{S.sidebar.noOutline}</p>
        {/if}
      </div>
    {:else}
      <div class="scroll">
        <form
          class="search"
          onsubmit={(e) => {
            e.preventDefault();
            void app.search(query, matchCase, wholeWord);
          }}
        >
          <input type="search" aria-label={S.sidebar.searchLabel} bind:value={query} />
          <button type="submit">{S.sidebar.searchButton}</button>
          <label><input type="checkbox" bind:checked={matchCase} /> {S.sidebar.matchCase}</label>
          <label><input type="checkbox" bind:checked={wholeWord} /> {S.sidebar.wholeWord}</label>
        </form>
        {#if tab.hits.length}
          <p class="muted">{S.sidebar.hits(tab.hits.length)}</p>
          <ol class="hits">
            {#each tab.hits as hit, i (i)}
              {@const label = tab.doc.pages.find((p) => p.id === hit.page)?.label ?? "?"}
              <li>
                <button
                  aria-current={i === tab.activeHit}
                  onclick={() => {
                    tab.activeHit = i;
                    app.goTo(hit.page);
                  }}
                >
                  <span class="muted">{S.sidebar.hitPage(label)}</span>
                  <span>{hit.text}</span>
                </button>
              </li>
            {/each}
          </ol>
        {/if}
      </div>
    {/if}
  </div>
</aside>

<style>
  .side {
    display: grid;
    grid-template-rows: auto 1fr;
    min-height: 0;
    width: 220px;
    background: var(--tr-surface);
    border-right: 1px solid var(--tr-border);
  }
  .views {
    display: flex;
    border-bottom: 1px solid var(--tr-border);
  }
  .views button {
    flex: 1;
    padding: 6px 4px;
    background: none;
    border: 0;
    border-bottom: 3px solid transparent;
  }
  .views button[aria-selected="true"] {
    border-bottom-color: var(--tr-accent);
    font-weight: 700;
  }
  .view {
    min-height: 0;
  }
  .scroll {
    height: 100%;
    overflow-y: auto;
    padding: 8px;
    box-sizing: border-box;
  }
  .search {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px;
  }
  .search label {
    grid-column: 1 / -1;
  }
  .muted {
    color: var(--tr-text-muted);
  }
  .hits {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .hits button {
    display: grid;
    width: 100%;
    gap: 2px;
    padding: 5px 6px;
    background: none;
    border: 0;
    border-radius: 4px;
    text-align: left;
  }
  .hits button[aria-current="true"] {
    background: var(--tr-surface-2);
    outline: 2px solid var(--tr-accent);
  }
</style>
