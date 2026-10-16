// Spike 1 page: virtualized pages, 512 px tiles over the trtile scheme, scroll timing.
const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
const status = (t) => (document.getElementById('status').textContent = t);
const GAP = 16;
const result = { userAgent: navigator.userAgent, dpr: devicePixelRatio, urlForms: {}, tileLatencyMs: [] };

function loadImage(url) {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve(true);
    img.onerror = () => resolve(false);
    img.src = url;
  });
}

function summary(v) {
  const s = [...v].sort((a, b) => a - b);
  const pick = (q) => s[Math.round((s.length - 1) * q)];
  return { n: s.length, mean: s.reduce((a, b) => a + b, 0) / s.length, p50: pick(0.5), p95: pick(0.95), max: pick(1) };
}

async function main() {
  const info = await invoke('doc_info');
  const scale = info.pxPerPt * devicePixelRatio;
  const scaleKey = Math.round(scale * 1000);
  const tileCss = info.tile / devicePixelRatio;

  // Which URL form loads on this platform?
  const forms = { scheme: 'trtile://localhost', http: 'http://trtile.localhost' };
  for (const [name, base] of Object.entries(forms)) {
    result.urlForms[name] = await loadImage(`${base}/0/${scaleKey}/0_0.png`);
  }
  const base = result.urlForms.http ? forms.http : forms.scheme;
  result.base = base;

  // Lay out all pages; only visible ones get tiles.
  const container = document.getElementById('pages');
  const width = Math.max(...info.pages.map((p) => p[0])) * info.pxPerPt;
  container.style.width = `${width}px`;
  let y = GAP;
  const pages = info.pages.map(([w, h], i) => {
    const div = document.createElement('div');
    div.className = 'page';
    const cssW = w * info.pxPerPt, cssH = h * info.pxPerPt;
    Object.assign(div.style, { top: `${y}px`, left: `${(width - cssW) / 2}px`, width: `${cssW}px`, height: `${cssH}px` });
    container.appendChild(div);
    const page = { i, div, top: y, height: cssH, w, h, loaded: false };
    y += cssH + GAP;
    return page;
  });
  container.style.height = `${y}px`;

  let firstPageDone = null;
  const scroller = document.getElementById('scroller');
  function update() {
    const top = scroller.scrollTop - 400, bottom = scroller.scrollTop + scroller.clientHeight + 400;
    for (const p of pages) {
      const visible = p.top + p.height > top && p.top < bottom;
      if (visible && !p.loaded) {
        p.loaded = true;
        const cols = Math.ceil((p.w * scale) / info.tile), rows = Math.ceil((p.h * scale) / info.tile);
        let pending = cols * rows;
        for (let ty = 0; ty < rows; ty++) {
          for (let tx = 0; tx < cols; tx++) {
            const img = new Image();
            const start = performance.now();
            img.onload = () => {
              result.tileLatencyMs.push(performance.now() - start);
              if (result.firstTileMs === undefined) result.firstTileMs = performance.now();
              if (--pending === 0 && p.i === 0) firstPageDone = performance.now();
            };
            img.style.left = `${tx * tileCss}px`;
            img.style.top = `${ty * tileCss}px`;
            img.style.width = `${Math.min(info.tile, p.w * scale - tx * info.tile) / devicePixelRatio}px`;
            img.style.height = `${Math.min(info.tile, p.h * scale - ty * info.tile) / devicePixelRatio}px`;
            img.src = `${base}/${p.i}/${scaleKey}/${tx}_${ty}.png`;
            p.div.appendChild(img);
          }
        }
      } else if (!visible && p.loaded) {
        p.loaded = false;
        p.div.replaceChildren();
      }
    }
  }
  scroller.addEventListener('scroll', update, { passive: true });
  update();

  while (firstPageDone === null) await new Promise((r) => setTimeout(r, 10));
  result.firstPageMs = firstPageDone;
  status(`First tile ${result.firstTileMs.toFixed(0)} ms, first page ${firstPageDone.toFixed(0)} ms after start. Scrolling.`);
  await new Promise((r) => setTimeout(r, 500));

  // Auto-scroll at 3000 CSS px per second for 6 seconds and record frame times.
  const frames = [];
  await new Promise((done) => {
    const t0 = performance.now();
    let last = t0;
    function step(now) {
      frames.push(now - last);
      last = now;
      scroller.scrollTop = ((now - t0) / 1000) * 3000;
      if (now - t0 < 6000 && scroller.scrollTop < scroller.scrollHeight - scroller.clientHeight) requestAnimationFrame(step);
      else done();
    }
    requestAnimationFrame(step);
  });
  frames.shift();
  result.frameMs = summary(frames);
  result.framesOver20ms = frames.filter((f) => f > 20).length;
  result.framesOver33ms = frames.filter((f) => f > 33.4).length;
  await new Promise((r) => setTimeout(r, 1500));
  result.tileLatency = summary(result.tileLatencyMs);
  delete result.tileLatencyMs;
  status('Done');
  await invoke('report', { result });
}

main().catch((e) => {
  status(`Error: ${e}`);
  invoke('report', { result: { error: String(e), ...result } });
});
