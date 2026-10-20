<script lang="ts">
  import { app, type Tab } from "../lib/store.svelte";
  import { S } from "../lib/strings";
  import { CSS_PER_POINT } from "../lib/tiles";
  import Page from "./Page.svelte";

  let { tab }: { tab: Tab } = $props();

  const GAP = 16;
  const MARGIN = 600;

  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let scrollLeft = $state(0);
  let viewW = $state(800);
  let viewH = $state(600);

  const cssScale = $derived(tab.zoom * CSS_PER_POINT);
  const layout = $derived.by(() => {
    let y = GAP;
    let maxW = 0;
    const tops: number[] = [];
    for (const p of tab.doc.pages) {
      tops.push(y);
      y += p.height * cssScale + GAP;
      maxW = Math.max(maxW, p.width * cssScale);
    }
    return { tops, height: y, width: maxW + 2 * GAP };
  });

  const visible = $derived.by(() => {
    const out: number[] = [];
    const pages = tab.doc.pages;
    for (let i = 0; i < pages.length; i++) {
      const top = layout.tops[i]!;
      const bottom = top + pages[i]!.height * cssScale;
      if (bottom >= scrollTop - MARGIN && top <= scrollTop + viewH + MARGIN) out.push(i);
    }
    return out;
  });

  function pageLeft(width: number): number {
    return Math.max(GAP, (Math.max(layout.width, viewW) - width * cssScale) / 2);
  }

  // The page under the middle of the view is the active page.
  $effect(() => {
    const mid = scrollTop + viewH / 2;
    const pages = tab.doc.pages;
    let best = -1;
    for (const i of visible) {
      if (layout.tops[i]! <= mid) best = i;
    }
    const id = pages[best >= 0 ? best : (visible[0] ?? 0)]?.id ?? null;
    if (id && id !== tab.activePage) {
      tab.activePage = id;
      app.pushContext();
    }
  });

  // Keep the same document position when the zoom changes.
  let lastScale = 0;
  $effect(() => {
    const s = cssScale;
    if (scroller && lastScale && s !== lastScale) {
      const k = s / lastScale;
      scroller.scrollTop = scrollTop * k;
      scroller.scrollLeft = scrollLeft * k;
    }
    lastScale = s;
  });

  $effect(() => {
    void tab.scrollRequest;
    const i = tab.doc.pages.findIndex((p) => p.id === tab.scrollTarget);
    if (scroller && i >= 0) scroller.scrollTop = layout.tops[i]! - GAP;
  });

  function onScroll() {
    if (!scroller) return;
    scrollTop = scroller.scrollTop;
    scrollLeft = scroller.scrollLeft;
  }

  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    app.zoomStep(e.deltaY < 0 ? 1 : -1);
  }

  $effect(() => {
    if (!scroller) return;
    const ro = new ResizeObserver(() => {
      viewW = scroller!.clientWidth;
      viewH = scroller!.clientHeight;
      if (tab.zoom === 0) app.setZoom(1);
    });
    ro.observe(scroller);
    return () => ro.disconnect();
  });

  export function fitWidth() {
    const widest = Math.max(1, ...tab.doc.pages.map((p) => p.width));
    app.setZoom((viewW - 2 * GAP - 16) / (widest * CSS_PER_POINT));
  }
</script>

<div
  class="scroller"
  bind:this={scroller}
  onscroll={onScroll}
  onwheel={onWheel}
  role="region"
  aria-label={S.viewer.label}
  tabindex="-1"
>
  <div class="sheet" style:height="{layout.height}px" style:width="{layout.width}px">
    {#each visible as i (tab.doc.pages[i]!.id)}
      {@const page = tab.doc.pages[i]!}
      {@const top = layout.tops[i]!}
      {@const left = pageLeft(page.width)}
      <Page
        doc={tab.doc.id}
        {page}
        {cssScale}
        {top}
        {left}
        selected={tab.selected.includes(page.id)}
        visibleRect={{
          top: scrollTop - top,
          bottom: scrollTop + viewH - top,
          left: scrollLeft - left,
          right: scrollLeft + viewW - left,
        }}
      />
    {/each}
  </div>
</div>

<style>
  .scroller {
    position: relative;
    overflow: auto;
    height: 100%;
    background: var(--tr-canvas);
  }
  .sheet {
    position: relative;
    min-width: 100%;
  }
</style>
