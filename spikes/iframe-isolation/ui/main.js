// Spike 2 host page: runs each panel variant, collects what leaked, and checks invoke under the CSP.
const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const status = (t) => (document.getElementById('status').textContent = t);

const isWindows = navigator.userAgent.includes('Windows');
const panelBase = isWindows ? 'http://trpanel.localhost' : 'trpanel://localhost';
const srcdoc = `<!doctype html><html><body><p>Panel</p><script src="${location.origin}/panel.js"></script></body></html>`;

const VARIANTS = [
  { variant: 'srcdoc + sandbox=allow-scripts', sandbox: 'allow-scripts', srcdoc },
  { variant: 'scheme URL + sandbox=allow-scripts + panel CSP', sandbox: 'allow-scripts', src: `${panelBase}/strict/panel.html` },
  { variant: 'scheme URL + sandbox=allow-scripts, no panel CSP', sandbox: 'allow-scripts', src: `${panelBase}/nocsp/panel.html` },
  { variant: 'CONTROL: scheme URL, no sandbox attribute', sandbox: null, src: `${panelBase}/strict/panel.html` },
  { variant: 'CONTROL: srcdoc + sandbox=allow-scripts allow-same-origin', sandbox: 'allow-scripts allow-same-origin', srcdoc },
];

const report = { userAgent: navigator.userAgent, hostOrigin: location.origin, cspViolations: [], variants: [] };
document.addEventListener('securitypolicyviolation', (e) => {
  report.cspViolations.push(`${e.violatedDirective} blocked ${e.blockedURI}`);
});

let navHits = [];
window.addEventListener('hashchange', () => {
  navHits.push(location.hash);
  history.replaceState(null, '', location.pathname);
});

async function runVariant(v) {
  const slot = document.getElementById('slot');
  const frame = document.createElement('iframe');
  if (v.sandbox !== null) frame.setAttribute('sandbox', v.sandbox);
  const messages = [];
  let results = null;
  const onMessage = (event) => {
    const fromFrame = event.source === frame.contentWindow;
    if (event.data?.type === 'ready' && fromFrame) {
      frame.contentWindow.postMessage({ type: 'init', variant: v.variant, topHref: location.href.split('#')[0] }, '*');
    } else if (event.data?.type === 'results' && fromFrame) {
      results = event.data.results;
    } else if (event.data?.type === 'host-command') {
      messages.push({ origin: event.origin, sourceIsThisFrame: fromFrame });
    }
  };
  window.addEventListener('message', onMessage);
  navHits = [];
  const before = await invoke('secret_calls');
  if (v.srcdoc) frame.srcdoc = v.srcdoc;
  else frame.src = v.src;
  slot.appendChild(frame);
  for (let i = 0; i < 300 && !results; i++) await sleep(50);
  await sleep(1500); // let late IPC messages land
  const after = await invoke('secret_calls');
  window.removeEventListener('message', onMessage);
  frame.remove();
  return {
    variant: v.variant,
    panelResults: results ?? 'no results (panel script did not run or could not post)',
    secretCallsFromPanel: after - before,
    topNavigation: navHits.length ? `LEAK: ${navHits.join(', ')}` : 'blocked',
    forgedHostMessages: messages,
  };
}

async function main() {
  // Does invoke work under this CSP, and how fast is it?
  const t0 = performance.now();
  report.firstInvoke = await invoke('ping');
  report.firstInvokeMs = performance.now() - t0;
  const t1 = performance.now();
  for (let i = 0; i < 200; i++) await invoke('ping');
  report.invokeMsMean = (performance.now() - t1) / 200;

  for (const v of VARIANTS) {
    status(`Running: ${v.variant}`);
    report.variants.push(await runVariant(v));
  }
  status('Done');
  await invoke('report', { result: report });
}

main().catch((e) => {
  status(`Error: ${e}`);
  invoke('report', { result: { error: String(e), ...report } });
});
