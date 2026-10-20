// Tile math (ARCHITECTURE section 10). Positions are CSS pixels inside one page.

export const TILE = 512;
/** Zoom buckets. The engine renders at these zooms times the device pixel ratio. */
export const BUCKETS = [0.25, 0.5, 1, 2, 4];
/** CSS pixels per point at zoom 1 (96 dpi). */
export const CSS_PER_POINT = 96 / 72;

/** The smallest bucket at or above the zoom, so tiles are never blurred. */
export function bucketFor(zoom: number): number {
  return BUCKETS.find((b) => b >= zoom - 1e-9) ?? BUCKETS[BUCKETS.length - 1]!;
}

/** Device pixels per point for a bucket. Rounded like the URL, so both sides agree. */
export function renderScale(bucket: number, dpr: number): number {
  return Math.round(bucket * CSS_PER_POINT * dpr * 1000) / 1000;
}

export interface TileRef {
  tx: number;
  ty: number;
  /** CSS position and size inside the page. */
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * The tiles of a page that touch the visible part. `pageW` and `pageH` are display
 * points, `cssScale` is CSS pixels per point, `scale` is device pixels per point.
 */
export function visibleTiles(
  pageW: number,
  pageH: number,
  cssScale: number,
  scale: number,
  visible: { top: number; bottom: number; left: number; right: number },
): TileRef[] {
  const pxW = Math.ceil(pageW * scale);
  const pxH = Math.ceil(pageH * scale);
  const k = cssScale / scale;
  const out: TileRef[] = [];
  const cols = Math.ceil(pxW / TILE);
  const rows = Math.ceil(pxH / TILE);
  for (let ty = 0; ty < rows; ty++) {
    const top = ty * TILE * k;
    const height = Math.min(TILE, pxH - ty * TILE) * k;
    if (top + height < visible.top || top > visible.bottom) continue;
    for (let tx = 0; tx < cols; tx++) {
      const left = tx * TILE * k;
      const width = Math.min(TILE, pxW - tx * TILE) * k;
      if (left + width < visible.left || left > visible.right) continue;
      out.push({ tx, ty, left, top, width, height });
    }
  }
  return out;
}

/** The base of scheme URLs. WebView2 and WebKitGTK need different forms (M1 spike 1). */
export function schemeBase(scheme: string, platform: string): string {
  return platform === "windows" ? `http://${scheme}.localhost` : `${scheme}://localhost`;
}

export function tilePath(doc: number, page: string, revision: number, scale: number, tx: number, ty: number): string {
  return `/${doc}/${page}/${revision}/${Math.round(scale * 1000)}/${tx}_${ty}.png`;
}
