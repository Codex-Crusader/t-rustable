<script lang="ts">
  // Mod overlays in display points. Text, images, and carets are upright (ADR 0017).
  import { app } from "../lib/store.svelte";
  import { S } from "../lib/strings";
  import type { Shape, Stroke } from "../lib/types";

  let { shapes, width, height, cssScale }: { shapes: Shape[]; width: number; height: number; cssScale: number } =
    $props();

  function dash(s: Stroke | null): string | undefined {
    return s && s.dash.length ? s.dash.join(" ") : undefined;
  }

  function imageUrl(modId: string, image: number): string {
    return `${app.tileBase}/image/${encodeURIComponent(modId)}/${image}`;
  }

  // Shapes carry no mod ID, so look it up for images.
  function owner(shape: Shape): string {
    for (const [id, list] of Object.entries(app.overlays)) if (list.includes(shape)) return id;
    return "";
  }
</script>

{#if shapes.length}
  <svg
    class="overlay"
    viewBox="0 0 {width} {height}"
    width={width * cssScale}
    height={height * cssScale}
    role="img"
    aria-label={S.viewer.overlay}
  >
    {#each shapes as s, i (i)}
      {#if s.type === "rect"}
        {#if s.ellipse}
          <ellipse
            cx={s.bounds.x + s.bounds.w / 2}
            cy={s.bounds.y + s.bounds.h / 2}
            rx={s.bounds.w / 2}
            ry={s.bounds.h / 2}
            fill={s.fill ?? "none"}
            stroke={s.stroke?.color ?? "none"}
            stroke-width={s.stroke?.width ?? 0}
            stroke-dasharray={dash(s.stroke)}
            aria-label={s.label}><title>{s.label}</title></ellipse
          >
        {:else}
          <rect
            x={s.bounds.x}
            y={s.bounds.y}
            width={s.bounds.w}
            height={s.bounds.h}
            fill={s.fill ?? "none"}
            stroke={s.stroke?.color ?? "none"}
            stroke-width={s.stroke?.width ?? 0}
            stroke-dasharray={dash(s.stroke)}
            aria-label={s.label}><title>{s.label}</title></rect
          >
        {/if}
      {:else if s.type === "line"}
        <line
          x1={s.start[0]}
          y1={s.start[1]}
          x2={s.end[0]}
          y2={s.end[1]}
          stroke={s.stroke.color}
          stroke-width={s.stroke.width}
          stroke-dasharray={dash(s.stroke)}
          aria-label={s.label}><title>{s.label}</title></line
        >
      {:else if s.type === "polyline"}
        {@const pts = s.points.map((p) => p.join(",")).join(" ")}
        {#if s.closed}
          <polygon
            points={pts}
            fill={s.fill ?? "none"}
            stroke={s.stroke?.color ?? "none"}
            stroke-width={s.stroke?.width ?? 0}
            aria-label={s.label}><title>{s.label}</title></polygon
          >
        {:else}
          <polyline
            points={pts}
            fill={s.fill ?? "none"}
            stroke={s.stroke?.color ?? "none"}
            stroke-width={s.stroke?.width ?? 0}
            aria-label={s.label}><title>{s.label}</title></polyline
          >
        {/if}
      {:else if s.type === "text"}
        <text
          x={s.at[0]}
          y={s.at[1]}
          font-family="{s.fontFamily}, Noto Sans, sans-serif"
          font-size={s.size}
          font-weight={s.bold ? 700 : 400}
          font-style={s.italic ? "italic" : "normal"}
          fill={s.color}
          xml:space="preserve"
          aria-label={s.label}>{s.text}</text
        >
      {:else if s.type === "image"}
        <image
          href={imageUrl(owner(s), s.image)}
          x={s.bounds.x}
          y={s.bounds.y}
          width={s.bounds.w}
          height={s.bounds.h}
          opacity={s.opacity}
          preserveAspectRatio="none"
          aria-label={s.label}><title>{s.label}</title></image
        >
      {:else if s.type === "caret"}
        <line
          class="caret"
          x1={s.at[0]}
          y1={s.at[1]}
          x2={s.at[0]}
          y2={s.at[1] - s.height}
          stroke={s.color}
          stroke-width={1 / cssScale + 0.5}
          aria-label={s.label}><title>{s.label}</title></line
        >
      {:else if s.type === "handle"}
        <rect
          class="handle"
          x={s.at[0] - 4 / cssScale}
          y={s.at[1] - 4 / cssScale}
          width={8 / cssScale}
          height={8 / cssScale}
          stroke-width={1 / cssScale}
          aria-label={s.label}><title>{s.label}</title></rect
        >
      {/if}
    {/each}
  </svg>
{/if}

<style>
  .overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: visible;
  }
  .caret {
    animation: blink 1s steps(1) infinite;
  }
  .handle {
    fill: var(--tr-surface);
    stroke: var(--tr-accent);
  }
  @keyframes blink {
    50% {
      opacity: 0;
    }
  }
</style>
