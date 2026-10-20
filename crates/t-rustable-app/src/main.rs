//! Tauri shell for the T-Rustable GUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod commands;
mod log;
mod modthread;
mod panels;
mod paths;
mod selftest;
mod state;
mod tiles;
mod wire;

use bridge::{ActionDto, DialogPicker, FileSettings, GuiBridge, ModUi, PanelDto};
use commands::App;
use modthread::ModThread;
use paths::{Folders, Resources};
use state::Shared;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use t_rustable_core::fonts::Fonts;
use t_rustable_engine::Engine;
use t_rustable_plugin_host::{Limits, ModHost, Services};
use tauri::http::Response;

fn banner() -> String {
    format!("T-Rustable {}", t_rustable_core::VERSION)
}

struct Args {
    /// An empty path means: use a generated sample PDF.
    self_test: Option<PathBuf>,
    /// Open the window without focus and below other windows.
    background: bool,
    files: Vec<PathBuf>,
}

fn args() -> Args {
    let mut out = Args {
        self_test: None,
        background: false,
        files: Vec::new(),
    };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--self-test" => {
                let file = it.next_if(|n| !n.starts_with("--"));
                out.self_test = Some(file.map(PathBuf::from).unwrap_or_default());
            }
            "--background" => out.background = true,
            "--version" => {
                println!("{}", banner());
                std::process::exit(0);
            }
            _ if !a.starts_with("--") => out.files.push(PathBuf::from(a)),
            _ => {}
        }
    }
    out
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

/// Loads the built-in mods. Test mods (`test.*`) load only in the self-test.
fn load_mods(host: &mut ModHost, dir: &std::path::Path, tests: bool, shared: &Shared) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
    dirs.sort();
    for d in dirs {
        let name = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let wanted = name.starts_with("t-rustable.") || (tests && name.starts_with("test."));
        if !wanted || !d.join("manifest.json").is_file() {
            continue;
        }
        if let Err(e) = host.load_dir(&d) {
            shared
                .log
                .write("error", "mods", &format!("{}: {e}", d.display()));
        }
    }
}

fn mod_ui(host: &ModHost) -> Vec<ModUi> {
    host.manifests()
        .map(|m| ModUi {
            id: m.id.clone(),
            name: m.name.clone(),
            disabled: false,
            actions: m
                .actions
                .iter()
                .map(|a| ActionDto {
                    id: a.id.clone(),
                    title: a.title.clone(),
                    description: a.description.clone(),
                    tool: a.tool,
                    shortcut: a.shortcut.clone(),
                })
                .collect(),
            toolbar: Vec::new(),
            panels: m
                .panels
                .iter()
                .map(|p| PanelDto {
                    id: p.id.clone(),
                    title: p.title.clone(),
                    entry: p.entry.clone(),
                })
                .collect(),
            dir: host.mod_dir(&m.id).map(PathBuf::from).unwrap_or_default(),
        })
        .collect()
}

/// A `trpanel` response. Every response has the panel CSP, also errors.
fn panel_file(bridge: &GuiBridge, path: &str) -> Response<Vec<u8>> {
    let found = panels::parse(path).and_then(|(mod_id, file)| {
        let dir = bridge.panel_dir(mod_id)?;
        if file == panels::THEME_FILE {
            let css = bridge.theme_css.lock().unwrap().clone().into_bytes();
            return Some((css, "text/css; charset=utf-8"));
        }
        let bytes = std::fs::read(panels::resolve(&dir, file)?).ok()?;
        let mime = panels::mime(file)?;
        let bytes = if file.ends_with(".html") {
            panels::inject_theme(&bytes, mod_id)
        } else {
            bytes
        };
        Some((bytes, mime))
    });
    let (status, mime, body) = match found {
        Some((body, mime)) => (200, mime, body),
        None => (404, "text/plain", Vec::new()),
    };
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Content-Security-Policy", panels::CSP)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap()
}

fn respond(status: u16, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-store")
        .body(body)
        .expect("response")
}

fn main() {
    let mut args = args();
    let folders = Folders::standard();
    if let Err(e) = folders.create() {
        fail(&format!("Cannot create the app folders: {e}"));
    }
    if args
        .self_test
        .as_ref()
        .is_some_and(|p| p.as_os_str().is_empty())
    {
        args.self_test = Some(selftest::sample(&folders.log));
    }
    if args.self_test.is_some() {
        selftest::watchdog();
    }
    let resources = Resources::find().unwrap_or_else(|e| fail(&e));
    let log = log::Log::open(&folders.log);
    log.write("info", "app", &banner());
    let engine = Engine::start(&resources.pdfium)
        .unwrap_or_else(|e| fail(&format!("PDFium did not start: {e}")));
    let fonts = Arc::new(Fonts::load(&resources.fonts).unwrap_or_else(|e| fail(&e.to_string())));
    let settings = Arc::new(FileSettings::new(&folders.config));
    let shared = Arc::new(Shared::new(engine, folders, log));
    let bridge = Arc::new(GuiBridge::new(shared.clone()));

    let services = Services {
        ui: bridge.clone(),
        files: Arc::new(DialogPicker {
            shared: shared.clone(),
            fixed: args
                .self_test
                .as_ref()
                .map(|_| selftest::picture(&shared.folders.config)),
        }),
        settings,
        fonts,
    };
    let (mods_dir, tests, load_shared, load_bridge) = (
        resources.mods.clone(),
        args.self_test.is_some(),
        shared.clone(),
        bridge.clone(),
    );
    let mods = ModThread::start(move || {
        let mut host = ModHost::new(services, Limits::default())
            .unwrap_or_else(|e| fail(&format!("The mod host did not start: {e}")));
        load_mods(&mut host, &mods_dir, tests, &load_shared);
        let mut registry = load_bridge.mods.lock().unwrap();
        for m in mod_ui(&host) {
            registry.insert(m.id.clone(), m);
        }
        drop(registry);
        host
    });

    let background = args.background || args.self_test.is_some();
    let tile_shared = shared.clone();
    let tile_bridge = bridge.clone();
    let setup_shared = shared.clone();
    let panel_bridge = bridge.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(App {
            shared: shared.clone(),
            bridge,
            mods,
            action_cancel: Mutex::new(None),
            self_test: args.self_test,
            files: args.files,
        })
        .setup(move |app| {
            let _ = setup_shared.app.set(app.handle().clone());
            // A self-test or background window stays behind other windows and takes no
            // focus, so a test run does not disturb the user.
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                .title("T-Rustable")
                .inner_size(1280.0, 860.0)
                .min_inner_size(640.0, 480.0)
                .disable_drag_drop_handler()
                .focused(!background)
                .always_on_bottom(background)
                .build()?;
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("trtile", move |_ctx, request, responder| {
            let path = request.uri().path().to_string();
            let shared = tile_shared.clone();
            let bridge = tile_bridge.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Some(rest) = path.strip_prefix("/image/") {
                    let found = rest
                        .split_once('/')
                        .and_then(|(m, id)| bridge.image(m, id.parse().ok()?));
                    responder.respond(match found {
                        Some(img) => respond(200, &img.0, img.1.clone()),
                        None => respond(404, "text/plain", Vec::new()),
                    });
                    return;
                }
                let r = match tiles::tile(&shared, &path) {
                    Ok(png) => respond(200, "image/png", png.to_vec()),
                    Err(tiles::TileError::BadPath) => respond(400, "text/plain", Vec::new()),
                    Err(tiles::TileError::Gone) => respond(410, "text/plain", Vec::new()),
                    Err(tiles::TileError::Cancelled) => respond(409, "text/plain", Vec::new()),
                    Err(tiles::TileError::Failed(m)) => {
                        shared.log.write("error", "tiles", &m);
                        respond(500, "text/plain", m.into_bytes())
                    }
                };
                responder.respond(r);
            });
        })
        .register_asynchronous_uri_scheme_protocol("trpanel", move |_ctx, request, responder| {
            let path = request.uri().path().to_string();
            let bridge = panel_bridge.clone();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(panel_file(&bridge, &path));
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::open_file,
            commands::document,
            commands::close_doc,
            commands::set_context,
            commands::page_text,
            commands::search,
            commands::registry,
            commands::select_tool,
            commands::run_action,
            commands::cancel_action,
            commands::pointer,
            commands::key,
            commands::text_input,
            commands::answer_confirm,
            commands::undo,
            commands::save,
            commands::cancel_tiles,
            commands::to_pdf,
            commands::outline,
            commands::panel_open,
            commands::panel_message,
            commands::set_panel_theme,
            commands::read_keymap,
            selftest::self_test_report,
            selftest::self_test_secret,
            selftest::self_test_secret_calls,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| fail(&format!("The window did not open: {e}")));
}

#[cfg(test)]
mod tests {
    #[test]
    fn banner_names_the_program() {
        assert!(super::banner().starts_with("T-Rustable "));
    }
}
