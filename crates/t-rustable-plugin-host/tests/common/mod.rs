//! Test services and helpers for the plugin host tests.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use t_rustable_core::fonts::Fonts;
use t_rustable_core::{OpenOptions, PageId, Session};
use t_rustable_engine::Engine;
use t_rustable_plugin_host::api::types::{self, LogLevel, OverlayShape};
use t_rustable_plugin_host::api::ui::{
    Action, NotifyLevel, Panel, Shortcut, ToolbarItem, UiContext,
};
use t_rustable_plugin_host::services::{FileFilter, FilePicker, MemorySettings, UiBridge};
use t_rustable_plugin_host::{Limits, ModHost, Services, SessionRef};

pub use t_rustable_engine::testing::{blank_pdf, engine};

/// A UI that records what mods do. `confirm` waits, then says yes.
pub struct TestUi {
    pub confirm_wait: Duration,
    pub active_page: Mutex<Option<PageId>>,
    pub overlays: Mutex<BTreeMap<String, Vec<OverlayShape>>>,
    pub notes: Mutex<Vec<(NotifyLevel, String)>>,
    pub actions: Mutex<Vec<(String, String)>>,
    pub images: Mutex<Vec<Vec<u8>>>,
}

impl TestUi {
    pub fn new(confirm_wait: Duration) -> Arc<Self> {
        Arc::new(Self {
            confirm_wait,
            active_page: Mutex::new(None),
            overlays: Mutex::default(),
            notes: Mutex::default(),
            actions: Mutex::default(),
            images: Mutex::default(),
        })
    }

    pub fn set_active_page(&self, page: PageId) {
        *self.active_page.lock().unwrap() = Some(page);
    }

    pub fn overlay(&self, mod_id: &str) -> Vec<OverlayShape> {
        self.overlays
            .lock()
            .unwrap()
            .get(mod_id)
            .cloned()
            .unwrap_or_default()
    }
}

fn api_page(p: PageId) -> types::PageId {
    t_rustable_plugin_host::bindings::v0_1::api_page(p)
}

impl UiBridge for TestUi {
    fn register_action(&self, mod_id: &str, action: Action) -> Result<(), String> {
        self.actions
            .lock()
            .unwrap()
            .push((mod_id.into(), action.id));
        Ok(())
    }
    fn register_toolbar_item(&self, _: &str, _: ToolbarItem) -> Result<(), String> {
        Ok(())
    }
    fn register_panel(&self, _: &str, _: Panel) -> Result<(), String> {
        Ok(())
    }
    fn suggest_shortcut(&self, _: &str, _: Shortcut) -> Result<(), String> {
        Ok(())
    }
    fn set_overlay(&self, mod_id: &str, shapes: Vec<OverlayShape>) {
        self.overlays.lock().unwrap().insert(mod_id.into(), shapes);
    }
    fn clear_overlay(&self, mod_id: &str) {
        self.overlays.lock().unwrap().remove(mod_id);
    }
    fn register_overlay_image(&self, _: &str, data: Vec<u8>) -> Result<u32, String> {
        let mut images = self.images.lock().unwrap();
        images.push(data);
        Ok(images.len() as u32)
    }
    fn post_to_panel(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn notify(&self, _: &str, level: NotifyLevel, message: &str) {
        self.notes.lock().unwrap().push((level, message.into()));
    }
    fn confirm(&self, _: &str, _: &str, _: &str) -> bool {
        std::thread::sleep(self.confirm_wait);
        true
    }
    fn context(&self, _: &str) -> UiContext {
        let active = *self.active_page.lock().unwrap();
        UiContext {
            document_open: true,
            active_page: active.map(api_page),
            selected_pages: active.into_iter().map(api_page).collect(),
            selected_objects: Vec::new(),
            zoom: 1.0,
        }
    }
    fn display_scale(&self, _: types::PageId) -> Option<f64> {
        Some(1.0)
    }
    fn log(&self, _: &str, _: LogLevel, _: &str) {}
    fn progress(&self, _: &str, _: f32, _: &str) {}
}

/// A file picker that gives one fixed file.
pub struct OneFile(pub Option<PathBuf>);

impl FilePicker for OneFile {
    fn pick(&self, _: &str, _: &str, _: &[FileFilter]) -> Option<PathBuf> {
        self.0.clone()
    }
}

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fonts() -> Arc<Fonts> {
    Arc::new(Fonts::load(&repo().join("fonts")).unwrap())
}

/// The built mod folder in `target/mods/`. Fails with the command to run if it is missing.
pub fn mod_dir(id: &str) -> PathBuf {
    let dir = repo().join("target/mods").join(id);
    assert!(
        dir.join("manifest.json").is_file(),
        "{} is missing. Run: cargo xtask build-mods",
        dir.display()
    );
    dir
}

pub fn services(ui: Arc<TestUi>, file: Option<PathBuf>) -> Services {
    Services {
        ui,
        files: Arc::new(OneFile(file)),
        settings: Arc::new(MemorySettings::default()),
        fonts: fonts(),
    }
}

pub fn host(ui: Arc<TestUi>, limits: Limits) -> ModHost {
    ModHost::new(services(ui, None), limits).unwrap()
}

pub fn session(e: &Engine, dir: &Path, pages: usize) -> SessionRef {
    let options = OpenOptions {
        sessions_dir: dir,
        password: None,
        source_name: Some("test.pdf".into()),
    };
    let bytes = blank_pdf(&vec![(612.0, 792.0); pages]);
    Arc::new(Mutex::new(Session::open(e, bytes.into(), options).unwrap()))
}

pub fn quarter_turns(s: &SessionRef, page: PageId) -> u8 {
    s.lock().unwrap().page(page).unwrap().info.quarter_turns
}
