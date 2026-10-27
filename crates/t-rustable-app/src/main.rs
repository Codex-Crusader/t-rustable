//! Tauri shell for the T-Rustable GUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod commands;
mod logfile;
mod modcmds;
mod modreg;
mod modthread;
mod panels;
mod recovery;
mod selftest;
mod state;
mod tiles;
mod wire;

use bridge::{DialogPicker, FileSettings, GuiBridge};
use commands::App;
use modreg::Mods;
use modthread::ModThread;
use state::Shared;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use t_rustable_core::fonts::Fonts;
use t_rustable_core::paths::{Folders, Resources};
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
    /// `.tmod` packages to offer for install.
    packages: Vec<PathBuf>,
}

fn args() -> Args {
    let mut out = Args {
        self_test: None,
        background: false,
        files: Vec::new(),
        packages: Vec::new(),
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
            _ if a.to_ascii_lowercase().ends_with(".tmod") => out.packages.push(PathBuf::from(a)),
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
    logfile::Log::install(&folders.log);
    log::info!("{}", banner());
    let resources = Resources::find().unwrap_or_else(|e| fail(&e));
    let engine = Engine::start(&resources.pdfium)
        .unwrap_or_else(|e| fail(&format!("PDFium did not start: {e}")));
    let fonts = Arc::new(Fonts::load(&resources.fonts).unwrap_or_else(|e| fail(&e.to_string())));
    let settings = Arc::new(FileSettings::new(&folders.config));
    let shared = Arc::new(Shared::new(engine, folders));
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
        resources
            .builtin_mods()
            .unwrap_or_else(|e| fail(&e))
            .to_path_buf(),
        args.self_test.is_some(),
        shared.clone(),
        bridge.clone(),
    );
    let mods = ModThread::start(move || {
        let host = ModHost::new(services, Limits::default())
            .unwrap_or_else(|e| fail(&format!("The mod host did not start: {e}")));
        Mods::start(host, load_shared, load_bridge, mods_dir, tests)
    });
    // The developer folder watch. A tick waits until the last tick is done.
    let poll = mods.clone();
    std::thread::Builder::new()
        .name("mod-watch".into())
        .spawn(move || {
            let busy = Arc::new(std::sync::atomic::AtomicBool::new(false));
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if busy.swap(true, std::sync::atomic::Ordering::AcqRel) {
                    continue;
                }
                let done = busy.clone();
                if !poll.send(move |m| {
                    m.poll();
                    done.store(false, std::sync::atomic::Ordering::Release);
                }) {
                    break;
                }
            }
        })
        .expect("start the mod watch thread");

    let background = args.background || args.self_test.is_some();
    let folders_sessions = shared.folders.sessions.clone();
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
            packages: args.packages,
            instance_lock: recovery::instance_lock(&folders_sessions),
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
                        log::error!("{path}: {m}");
                        respond(500, "text/plain", m.into_bytes())
                    }
                };
                responder.respond(r);
            });
        })
        // The UI asks about unsaved changes, then calls `quit`.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = tauri::Emitter::emit(window, "close-requested", ());
            }
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
            modcmds::mod_list,
            modcmds::install_offer,
            modcmds::install_answer,
            modcmds::mod_set_on,
            modcmds::mod_remove,
            modcmds::mod_reload,
            modcmds::developer_mode,
            modcmds::dev_folder_add,
            modcmds::dev_folder_remove,
            selftest::self_test_packages,
            selftest::self_test_rename_dev,
            recovery::unclosed,
            recovery::recover,
            recovery::discard_unclosed,
            recovery::quit,
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
