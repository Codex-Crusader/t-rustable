<script lang="ts">
  import { api } from "../lib/api";
  import { focusToolInput, pointer } from "../lib/input";
  import { app } from "../lib/store.svelte";
  import { S } from "../lib/strings";
  import { bucketFor, renderScale, tilePath, visibleTiles } from "../lib/tiles";
  import type { DocId, PageInfo, TextRun } from "../lib/types";
  import Overlay from "./Overlay.svelte";

  let {
    doc,
    page,
    cssScale,
    top,
    left,
    selected,
    visibleRect,
  }: {
    doc: DocId;
    page: PageInfo;
    cssScale: number;
    top: number;
    left: number;
    selected: boolean;
    visibleRect: { top: number; bottom: number; left: number; right: number };
  } = $props();

  const width = $derived(page.width * cssScale);
  const height = $derived(page.height * cssScale);
  const zoom = $derived(cssScale / (96 / 72));
  const scale = $derived(renderScale(bucketFor(zoom * app.dpr), 1));
  // First pass: one low resolution layer for the whole page.
  const lowScale = $derived(Math.min(scale, renderScale(0.25, 1)));
  const all = { top: -1e9, bottom: 1e9, left: -1e9, right: 1e9 };
  const lowTiles = $derived(visibleTiles(page.width, page.height, cssScale, lowScale, all));
  const tiles = $derived(scale === lowScale ? [] : visibleTiles(page.width, page.height, cssScale, scale, visibleRect));

  function url(s: number, tx: number, ty: number): string {
    return app.tileBase + tilePath(doc, page.id, page.revision, s, tx, ty);
  }

  // A tile that leaves the view before it loads is cancelled.
  function tileLife(img: HTMLImageElement, path: string) {
    let done = img.complete;
    const loaded = () => {
      done = true;
      window.dispatchEvent(new CustomEvent("tr-tile"));
    };
    img.addEventListener("load", loaded);
    img.addEventListener("error", loaded);
    return {
      destroy() {
        img.removeEventListener("load", loaded);
        img.removeEventListener("error", loaded);
        if (!done) cancelLater(path);
      },
    };
  }

  let text: TextRun[] = $state([]);
  let textFor = "";
  $effect(() => {
    const key = `${doc}/${page.id}/${page.revision}`;
    if (key === textFor) return;
    textFor = key;
    api.pageText(doc, page.id).then(
      (runs) => {
        if (textFor === key) text = runs;
      },
      () => (text = []),
    );
  });

  // Search hits on this page. The active hit has a stronger color.
  const hits = $derived.by(() => {
    const tab = app.tabs.find((t) => t.doc.id === doc);
    if (!tab) return [];
    return tab.hits.flatMap((h, i) =>
      h.page === page.id ? h.boxes.map((b) => ({ ...b, active: i === tab.activeHit })) : [],
    );
  });

  const shapes = $derived(
    Object.values(app.overlays)
      .flat()
      .filter((s) => s.page === page.id),
  );

  function at(e: PointerEvent | MouseEvent): [number, number] {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return [(e.clientX - r.left) / cssScale, (e.clientY - r.top) / cssScale];
  }

  function onPointer(kind: "down" | "move" | "up" | "enter" | "leave", e: PointerEvent) {
    const [x, y] = at(e);
    if (kind === "down") {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      focusToolInput();
    }
    pointer(doc, page.id, kind, x, y, e);
  }

  function onDouble(e: MouseEvent) {
    const [x, y] = at(e);
    pointer(doc, page.id, "double-click", x, y, e);
  }
</script>

<script module lang="ts">
  import { api as tileApi } from "../lib/api";

  let toCancel: string[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  function cancelLater(path: string) {
    toCancel.push(path);
    timer ??= setTimeout(() => {
      const paths = toCancel;
      toCancel = [];
      timer = null;
      void tileApi.cancelTiles(paths);
    }, 50);
  }
</script>

<div
  class="page"
  class:selected
  style:top="{top}px"
  style:left="{left}px"
  style:width="{width}px"
  style:height="{height}px"
  role="group"
  aria-label={S.viewer.page(page.label)}
>
  {#each lowTiles as t (`${lowScale}/${t.tx}_${t.ty}/${page.revision}`)}
    <img
      class="tile"
      alt=""
      draggable="false"
      src={url(lowScale, t.tx, t.ty)}
      style:left="{t.left}px"
      style:top="{t.top}px"
      style:width="{t.width}px"
      style:height="{t.height}px"
    />
  {/each}
  {#each tiles as t (`${scale}/${t.tx}_${t.ty}/${page.revision}`)}
    {@const path = tilePath(doc, page.id, page.revision, scale, t.tx, t.ty)}
    <img
      class="tile"
      alt=""
      draggable="false"
      src={app.tileBase + path}
      use:tileLife={path}
      style:left="{t.left}px"
      style:top="{t.top}px"
      style:width="{t.width}px"
      style:height="{t.height}px"
    />
  {/each}

  <svg class="text-layer" viewBox="0 0 {page.width} {page.height}" width={width} height={height}>
    {#each text as run, i (i)}
      {#if run.vertical}
        <text
          x={run.box.x + run.box.w * 0.8}
          y={run.box.y}
          font-size={run.box.w}
          textLength={run.box.h}
          lengthAdjust="spacingAndGlyphs"
          transform="rotate(90 {run.box.x + run.box.w * 0.8} {run.box.y})">{run.text}</text
        >
      {:else}
        <text
          x={run.box.x}
          y={run.box.y + run.box.h * 0.8}
          font-size={run.box.h}
          textLength={run.box.w}
          lengthAdjust="spacingAndGlyphs">{run.text}</text
        >
      {/if}
    {/each}
  </svg>

  {#if hits.length}
    <svg class="hits" viewBox="0 0 {page.width} {page.height}" width={width} height={height} aria-hidden="true">
      {#each hits as b, i (i)}
        <rect x={b.x} y={b.y} width={b.w} height={b.h} class:active={b.active} />
      {/each}
    </svg>
  {/if}

  <Overlay {shapes} width={page.width} height={page.height} {cssScale} />

  {#if app.tool}
    <div
      class="capture"
      style:cursor={app.cursor}
      role="presentation"
      onpointerdown={(e) => onPointer("down", e)}
      onpointermove={(e) => onPointer("move", e)}
      onpointerup={(e) => onPointer("up", e)}
      onpointerenter={(e) => onPointer("enter", e)}
      onpointerleave={(e) => onPointer("leave", e)}
      ondblclick={onDouble}
    ></div>
  {/if}
</div>

<style>
  .page {
    position: absolute;
    background: #ffffff;
    box-shadow: var(--tr-page-shadow);
    overflow: hidden;
  }
  .page.selected {
    outline: 3px solid var(--tr-accent);
  }
  .tile {
    position: absolute;
    user-select: none;
  }
  .text-layer {
    position: absolute;
    inset: 0;
    fill: transparent;
    font-family: var(--tr-font);
  }
  .text-layer text::selection {
    fill: transparent;
    background: var(--tr-selection);
  }
  .hits {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .hits rect {
    fill: var(--tr-hit);
    opacity: 0.35;
  }
  .hits rect.active {
    opacity: 0.7;
  }
  .capture {
    position: absolute;
    inset: 0;
    touch-action: none;
  }
</style>
