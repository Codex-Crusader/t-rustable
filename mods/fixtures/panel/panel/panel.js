// Test panel: tries to escape the iframe. Each check gives "blocked" or "LEAK: ...".
// The host must block all of them. Then the panel sends the results to its mod, and
// it answers the echo with "ack". This proves the message path both ways.
const results = {};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function check(name, attempt) {
  try {
    const value = await Promise.race([attempt(), sleep(3000).then(() => 'timeout')]);
    results[name] = value === undefined || value === null || value === false || value === 'timeout' ? `blocked (${value})` : `LEAK: ${String(value).slice(0, 120)}`;
  } catch (e) {
    results[name] = `blocked (${e.name}: ${String(e.message).slice(0, 100)})`;
  }
}

// Builds an IPC request like the Tauri 2 JS does, without the invoke key.
function ipcFetch(base) {
  return fetch(`${base}/self_test_secret`, {
    method: 'POST',
    body: JSON.stringify({}),
    headers: { 'Content-Type': 'application/json', 'Tauri-Callback': '1', 'Tauri-Error': '2' },
  }).then(async (r) => (r.ok ? `status ${r.status}: ${await r.text()}` : false));
}

function send(message) {
  parent.postMessage({ type: "t-rustable", message }, "*");
}

async function run() {
  results.origin = String(self.origin);

  await check('own __TAURI_INTERNALS__', () => typeof window.__TAURI_INTERNALS__ !== 'undefined' && 'present');
  await check('own __TAURI_INTERNALS__.invoke(self_test_secret)', () => window.__TAURI_INTERNALS__.invoke('self_test_secret'));
  await check('own __TAURI_INTERNALS__.invoke(app_info)', () => window.__TAURI_INTERNALS__.invoke('app_info'));
  await check('own __TAURI__', () => typeof window.__TAURI__ !== 'undefined' && 'present');
  await check('parent.__TAURI_INTERNALS__', () => typeof parent.__TAURI_INTERNALS__ !== 'undefined' && 'present');
  await check('parent.__TAURI_INTERNALS__.invoke(self_test_secret)', () => parent.__TAURI_INTERNALS__.invoke('self_test_secret'));
  await check('top.__TAURI_INTERNALS__.invoke(self_test_secret)', () => top.__TAURI_INTERNALS__.invoke('self_test_secret'));
  await check('parent.document read', () => parent.document.body.innerHTML.length > 0 && 'read parent DOM');
  await check('fetch ipc://localhost/secret', () => ipcFetch('ipc://localhost'));
  await check('fetch http://ipc.localhost/secret', () => ipcFetch('http://ipc.localhost'));
  await check('window.ipc.postMessage', () => {
    window.ipc.postMessage(JSON.stringify({ cmd: 'self_test_secret', callback: 1, error: 2, payload: {} }));
    return 'sent';
  });
  await check('chrome.webview.postMessage', () => {
    window.chrome.webview.postMessage(JSON.stringify({ cmd: 'self_test_secret', callback: 1, error: 2, payload: {} }));
    return 'sent';
  });
  await check('localStorage', () => {
    localStorage.setItem('x', '1');
    return 'writable';
  });
  await check('document.cookie', () => {
    document.cookie = 'x=1';
    return document.cookie.includes('x=1') && 'cookie set';
  });
  await check('fetch app files', async () => {
    for (const base of ['http://tauri.localhost', 'tauri://localhost']) {
      try {
        const r = await fetch(`${base}/index.html`);
        if (r.ok) return `read ${base}/index.html (${(await r.text()).length} bytes)`;
      } catch (_) {}
    }
    return false;
  });
  await check('fetch external site', () => fetch('https://example.com/').then((r) => `status ${r.status}`));
  await check('window.open', () => {
    const w = window.open('about:blank');
    return w && 'popup opened';
  });
  await check('navigate top', async () => {
    top.location.hash = 'escaped';
    await sleep(300);
    return false; // the self-test reads the hash of the host page
  });
  await check('form submit', async () => {
    const sink = document.createElement('iframe');
    sink.name = 'sink';
    document.body.appendChild(sink);
    let loaded = false;
    sink.onload = () => (loaded = true);
    await sleep(100);
    loaded = false;
    const form = document.createElement('form');
    form.target = 'sink';
    form.action = 'about:blank';
    document.body.appendChild(form);
    form.submit();
    await sleep(500);
    return loaded && 'form submitted';
  });

  document.getElementById("status").textContent = "Done.";
  send(JSON.stringify({ kind: "results", results }));
}

window.addEventListener("message", (event) => {
  if (event.source !== parent || event.data?.type !== "t-rustable") return;
  if (String(event.data.message).startsWith('echo:{')) send("ack");
});
run();
