<script lang="ts">
  import { app, type Tab } from "../lib/store.svelte";
  import { S } from "../lib/strings";
  import { bucketFor, CSS_PER_POINT, renderScale, tilePath, visibleTiles } from "../lib/tiles";

  let { tab }: { tab: Tab } = $props();

  const WIDTH = 120;
  const ALL = { top: -1e9, bottom: 1e9, left: -1e9, right: 1e9 };

  let list: HTMLDivElement | undefined = $state();

  function thumb(width: number, height: number) {
    const cssScale = WIDTH / Math.max(width, height);
    const scale = renderScale(bucketFor((cssScale / CSS_PER_POINT) * app.dpr), 1);
    return { cssScale, scale, tiles: visibleTiles(width, height, cssScale, scale, ALL) };
  }

  function select(e: MouseEvent | KeyboardEvent, id: string) {
    app.selectPage(id, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "only");
  }

  function onKey(e: KeyboardEvent) {
    const pages = tab.doc.pages;
    const at = pages.findIndex((p) => p.id === tab.activePage);
    let next = at;
    if (e.key === "ArrowDown" || e.key === "ArrowRight") next = at + 1;
    else if (e.key === "ArrowUp" || e.key === "ArrowLeft") next = at - 1;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = pages.length - 1;
    else if (e.key === " " || e.key === "Enter") {
      if (tab.activePage) select(e, tab.activePage);
      e.preventDefault();
      return;
    } else return;
    e.preventDefault();
    const page = pages[Math.max(0, Math.min(pages.length - 1, next))];
    if (!page) return;
    if (e.shiftKey) select(e, page.id);
    else app.goTo(page.id);
  }

  // Keep the active thumbnail in view.
  $effect(() => {
    const id = tab.activePage;
    list?.querySelector(`[data-page="${CSS.escape(id ?? "")}"]`)?.scrollIntoView({ block: "nearest" });
  });
</script>

<div
  class="thumbs"
  role="listbox"
  aria-label={S.sidebar.thumbnails}
  aria-multiselectable="true"
  aria-activedescendant={tab.activePage ? `thumb-${tab.activePage}` : undefined}
  tabindex="0"
  bind:this={list}
  onkeydown={onKey}
>
  {#each tab.doc.pages as page (page.id)}
    {@const t = thumb(page.width, page.height)}
    <div
      id="thumb-{page.id}"
      class="item"
      class:active={page.id === tab.activePage}
      role="option"
      aria-selected={tab.selected.includes(page.id)}
      data-page={page.id}
      tabindex="-1"
      onclick={(e) => select(e, page.id)}
      onkeydown={() => {}}
    >
      <div class="frame" style:width="{page.width * t.cssScale}px" style:height="{page.height * t.cssScale}px">
        {#each t.tiles as tile (`${tile.tx}_${tile.ty}`)}
          <img
            alt=""
            loading="lazy"
            draggable="false"
            src={app.tileBase + tilePath(tab.doc.id, page.id, page.revision, t.scale, tile.tx, tile.ty)}
            style:left="{tile.left}px"
            style:top="{tile.top}px"
            style:width="{tile.width}px"
            style:height="{tile.height}px"
          />
        {/each}
      </div>
      <span class="label">{page.label}</span>
    </div>
  {/each}
</div>

<style>
  .thumbs {
    display: grid;
    justify-items: center;
    gap: 10px;
    padding: 10px 6px;
    overflow-y: auto;
    height: 100%;
    box-sizing: border-box;
  }
  .item {
    display: grid;
    justify-items: center;
    gap: 4px;
    padding: 6px;
    border-radius: var(--tr-radius);
    border: 2px solid transparent;
    cursor: pointer;
  }
  .item[aria-selected="true"] {
    background: var(--tr-surface-2);
    border-color: var(--tr-accent);
  }
  .item.active .label {
    font-weight: 700;
  }
  .thumbs:focus-visible .item.active {
    outline: 2px solid var(--tr-focus);
  }
  .frame {
    position: relative;
    background: #ffffff;
    box-shadow: var(--tr-page-shadow);
    overflow: hidden;
  }
  .frame img {
    position: absolute;
  }
  .label {
    color: var(--tr-text-muted);
  }
</style>
