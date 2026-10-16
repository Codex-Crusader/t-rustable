//! Spike 2: mod panel isolation.
//!
//!   spike-iframe-isolation [locked|ipc-allowed]
//!
//! `locked` uses the CSP from tauri.conf.json (connect-src 'none').
//! `ipc-allowed` adds the Tauri IPC origins to connect-src, to compare.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tauri::http::Response;
use tauri::utils::config::Csp;

static SECRET_CALLS: AtomicUsize = AtomicUsize::new(0);

const PANEL_HTML: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head>
<body><p>Panel</p><script src="/panel.js"></script></body></html>"#;
const PANEL_JS: &str = include_str!("../ui/panel.js");
// 'self' does not match the scheme in an opaque-origin panel on WebKitGTK, so list the scheme.
const PANEL_CSP: &str = "default-src 'none'; script-src trpanel: http://trpanel.localhost; style-src trpanel: http://trpanel.localhost; img-src trpanel: http://trpanel.localhost data:; connect-src 'none'; form-action 'none'";

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

/// A command that only the host page may call. Any call from a panel is a leak.
#[tauri::command]
fn secret() -> &'static str {
    SECRET_CALLS.fetch_add(1, Ordering::SeqCst);
    "SECRET"
}

#[tauri::command]
fn secret_calls() -> usize {
    SECRET_CALLS.load(Ordering::SeqCst)
}

#[tauri::command]
fn report(handle: tauri::AppHandle, result: serde_json::Value) {
    let mode = std::env::args().nth(1).unwrap_or("locked".into());
    let text = serde_json::to_string_pretty(&result).unwrap();
    println!("{text}");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join(format!("iframe-{}-{mode}.json", std::env::consts::OS)),
        text,
    )
    .unwrap();
    handle.exit(0);
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or("locked".into());
    let mut context = tauri::generate_context!();
    if mode == "ipc-allowed" {
        let csp = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src ipc: http://ipc.localhost; frame-src trpanel: http://trpanel.localhost; object-src 'none'; base-uri 'none'; form-action 'none'";
        context.config_mut().app.security.csp = Some(Csp::Policy(csp.into()));
    }
    println!("mode {mode}, CSP: {:?}", context.config().app.security.csp);

    // If the page cannot call `report`, invoke is blocked. Exit with a note.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(60));
        println!(
            "NO REPORT after 60 s: the page could not call invoke. secret calls: {}",
            SECRET_CALLS.load(Ordering::SeqCst)
        );
        std::process::exit(2);
    });

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ping, secret, secret_calls, report])
        // Panels: /<variant>/panel.html and /panel.js. Variant "nocsp" gets no CSP header.
        .register_uri_scheme_protocol("trpanel", |_ctx, request| {
            let path = request.uri().path().to_string();
            let (body, kind) = if path.ends_with("panel.js") {
                (PANEL_JS.as_bytes().to_vec(), "text/javascript")
            } else {
                (PANEL_HTML.as_bytes().to_vec(), "text/html")
            };
            let mut response = Response::builder().header("Content-Type", kind);
            if !path.contains("/nocsp/") {
                response = response.header("Content-Security-Policy", PANEL_CSP);
            }
            response.body(body).unwrap()
        })
        .run(context)
        .expect("run Tauri");
}
