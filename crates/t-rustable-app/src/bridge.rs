//! The GUI side of the plugin host services: `UiBridge`, `FilePicker`, and `SettingsStore`.

use crate::state::Shared;
use crate::wire::{self, ShapeDto};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use t_rustable_plugin_host::api::types::{OverlayShape, PageId};
use t_rustable_plugin_host::api::ui::{
    Action, NotifyLevel, Panel, Shortcut, ToolbarItem, UiContext,
};
use t_rustable_plugin_host::bindings::v0_1::{api_page, page_id};
use t_rustable_plugin_host::services::{
    FileFilter, FilePicker, SettingsStore, UiBridge, mod_target,
};
use tauri::Emitter;
use tauri_plugin_dialog::DialogExt;

/// Overlay images of one mod: at most this many bytes.
const IMAGE_BYTES_PER_MOD: usize = 64 << 20;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionDto {
    pub id: String,
    pub title: String,
    pub description: String,
    pub tool: bool,
    pub shortcut: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolbarDto {
    pub action_id: String,
    pub icon: Option<String>,
    pub tooltip: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelDto {
    pub id: String,
    pub title: String,
    pub entry: String,
}

/// What one mod shows in the UI: from its manifest and from run-time registrations.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUi {
    pub id: String,
    pub name: String,
    pub disabled: bool,
    pub actions: Vec<ActionDto>,
    pub toolbar: Vec<ToolbarDto>,
    pub panels: Vec<PanelDto>,
    /// The folder of the mod. The `trpanel` scheme serves panel files from it.
    #[serde(skip)]
    pub dir: PathBuf,
    /// The mod comes from a developer folder.
    pub developer: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayEvent {
    mod_id: String,
    shapes: Vec<ShapeDto>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotifyEvent {
    mod_id: String,
    level: &'static str,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmEvent {
    id: u64,
    mod_id: String,
    title: String,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    mod_id: String,
    fraction: f32,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelMessageEvent {
    pub mod_id: String,
    pub panel_id: String,
    pub message: String,
}

/// The media type and the bytes of an overlay image.
pub type Image = Arc<(String, Vec<u8>)>;

pub struct GuiBridge {
    shared: Arc<Shared>,
    pub mods: Mutex<BTreeMap<String, ModUi>>,
    confirms: Mutex<HashMap<u64, mpsc::Sender<bool>>>,
    next_confirm: AtomicU64,
    images: Mutex<HashMap<String, Vec<Image>>>,
    /// The panel theme file: `--tr-*` variables that the UI sends.
    pub theme_css: Mutex<String>,
}

impl GuiBridge {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            mods: Mutex::new(BTreeMap::new()),
            confirms: Mutex::new(HashMap::new()),
            next_confirm: AtomicU64::new(1),
            images: Mutex::new(HashMap::new()),
            theme_css: Mutex::new(String::new()),
        }
    }

    /// The folder of a loaded mod that has panels and is not disabled.
    pub fn panel_dir(&self, mod_id: &str) -> Option<PathBuf> {
        let mods = self.mods.lock().unwrap();
        let m = mods.get(mod_id).filter(|m| !m.disabled)?;
        (!m.panels.is_empty()).then(|| m.dir.clone())
    }

    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(app) = self.shared.app.get() {
            let _ = app.emit(event, payload);
        }
    }

    pub fn registry_changed(&self) {
        self.emit("registry-changed", ());
    }

    /// A developer folder loaded again. Its open panels load again too.
    pub fn mod_reloaded(&self, mod_id: &str) {
        self.emit("mod-reloaded", mod_id.to_string());
    }

    /// A notice from the app, not from a mod.
    pub fn app_notice(&self, level: &'static str, message: &str) {
        self.emit(
            "notify",
            NotifyEvent {
                mod_id: String::new(),
                level,
                message: message.into(),
            },
        );
    }

    /// Answers a `confirm-request`. Unknown IDs do nothing.
    pub fn answer_confirm(&self, id: u64, yes: bool) {
        if let Some(tx) = self.confirms.lock().unwrap().remove(&id) {
            let _ = tx.send(yes);
        }
    }

    /// Sends the shapes of a mod to the UI, in display points.
    pub fn show_overlay(&self, mod_id: &str, shapes: &[OverlayShape]) {
        let shapes = self.convert(shapes);
        self.emit(
            "overlay",
            OverlayEvent {
                mod_id: mod_id.into(),
                shapes,
            },
        );
    }

    pub fn convert(&self, shapes: &[OverlayShape]) -> Vec<ShapeDto> {
        shapes
            .iter()
            .filter_map(|s| {
                let page = wire::shape_page(s);
                let doc = self.shared.doc_of(page)?;
                let view = self.shared.view(doc, page)?;
                Some(wire::shape(&view, s))
            })
            .collect()
    }

    /// The type and the bytes of an overlay image, for the `trtile` scheme.
    pub fn image(&self, mod_id: &str, id: u32) -> Option<Image> {
        self.images
            .lock()
            .unwrap()
            .get(mod_id)?
            .get(id as usize)
            .cloned()
    }

    pub fn clear_overlay_of(&self, mod_id: &str) {
        self.show_overlay(mod_id, &[]);
    }

    pub fn emit_panel_message(&self, event: PanelMessageEvent) {
        self.emit("panel-message", event);
    }

    fn with_mod(&self, mod_id: &str, f: impl FnOnce(&mut ModUi)) {
        let mut mods = self.mods.lock().unwrap();
        let entry = mods.entry(mod_id.to_string()).or_insert_with(|| ModUi {
            id: mod_id.into(),
            name: mod_id.into(),
            ..ModUi::default()
        });
        f(entry);
    }
}

impl UiBridge for GuiBridge {
    fn register_action(&self, mod_id: &str, action: Action) -> Result<(), String> {
        let mut dup = false;
        self.with_mod(mod_id, |m| {
            dup = m.actions.iter().any(|a| a.id == action.id);
            if !dup {
                m.actions.push(ActionDto {
                    id: action.id,
                    title: action.title,
                    description: action.description,
                    tool: false,
                    shortcut: None,
                });
            }
        });
        if dup {
            return Err("An action with this ID exists.".into());
        }
        self.registry_changed();
        Ok(())
    }

    fn register_toolbar_item(&self, mod_id: &str, item: ToolbarItem) -> Result<(), String> {
        let mut known = false;
        self.with_mod(mod_id, |m| {
            known = m.actions.iter().any(|a| a.id == item.action_id);
            if known {
                m.toolbar.retain(|t| t.action_id != item.action_id);
                m.toolbar.push(ToolbarDto {
                    action_id: item.action_id,
                    icon: item.icon,
                    tooltip: item.tooltip,
                });
            }
        });
        if !known {
            return Err("The toolbar item names an unknown action.".into());
        }
        self.registry_changed();
        Ok(())
    }

    fn register_panel(&self, mod_id: &str, panel: Panel) -> Result<(), String> {
        if !panel.entry.ends_with(".html") {
            return Err("The panel entry must be an .html file.".into());
        }
        let mut dup = false;
        self.with_mod(mod_id, |m| {
            dup = m.panels.iter().any(|p| p.id == panel.id);
            if !dup {
                m.panels.push(PanelDto {
                    id: panel.id,
                    title: panel.title,
                    entry: panel.entry,
                });
            }
        });
        if dup {
            return Err("A panel with this ID exists.".into());
        }
        self.registry_changed();
        Ok(())
    }

    fn suggest_shortcut(&self, mod_id: &str, shortcut: Shortcut) -> Result<(), String> {
        let mut known = false;
        self.with_mod(mod_id, |m| {
            if let Some(a) = m.actions.iter_mut().find(|a| a.id == shortcut.action_id) {
                a.shortcut = Some(shortcut.keys);
                known = true;
            }
        });
        if !known {
            return Err("The shortcut names an unknown action.".into());
        }
        self.registry_changed();
        Ok(())
    }

    fn set_overlay(&self, mod_id: &str, shapes: Vec<OverlayShape>) {
        self.show_overlay(mod_id, &shapes);
    }

    fn clear_overlay(&self, mod_id: &str) {
        self.show_overlay(mod_id, &[]);
    }

    fn register_overlay_image(&self, mod_id: &str, data: Vec<u8>) -> Result<u32, String> {
        let mime = if data.starts_with(b"\x89PNG") {
            "image/png"
        } else {
            "image/jpeg"
        };
        let mut images = self.images.lock().unwrap();
        let list = images.entry(mod_id.to_string()).or_default();
        let used: usize = list.iter().map(|i| i.1.len()).sum();
        if used + data.len() > IMAGE_BYTES_PER_MOD {
            return Err("The overlay images of this mod use too much memory.".into());
        }
        list.push(Arc::new((mime.into(), data)));
        Ok((list.len() - 1) as u32)
    }

    fn post_to_panel(&self, mod_id: &str, panel_id: &str, message: &str) -> Result<(), String> {
        let known = self
            .mods
            .lock()
            .unwrap()
            .get(mod_id)
            .is_some_and(|m| m.panels.iter().any(|p| p.id == panel_id));
        if !known {
            return Err(format!("This mod has no panel {panel_id:?}."));
        }
        self.emit_panel_message(PanelMessageEvent {
            mod_id: mod_id.into(),
            panel_id: panel_id.into(),
            message: message.into(),
        });
        Ok(())
    }

    fn notify(&self, mod_id: &str, level: NotifyLevel, message: &str) {
        let level = match level {
            NotifyLevel::Info => "info",
            NotifyLevel::Success => "success",
            NotifyLevel::Warning => "warning",
            NotifyLevel::Error => "error",
        };
        self.emit(
            "notify",
            NotifyEvent {
                mod_id: mod_id.into(),
                level,
                message: message.into(),
            },
        );
    }

    fn confirm(&self, mod_id: &str, title: &str, message: &str) -> bool {
        let id = self.next_confirm.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.confirms.lock().unwrap().insert(id, tx);
        self.emit(
            "confirm-request",
            ConfirmEvent {
                id,
                mod_id: mod_id.into(),
                title: title.into(),
                message: message.into(),
            },
        );
        // A closed window drops the sender, which counts as "no".
        rx.recv().unwrap_or(false)
    }

    fn context(&self, _mod_id: &str) -> UiContext {
        let c = self.shared.context.lock().unwrap().clone();
        UiContext {
            document_open: c.doc.is_some(),
            active_page: c.active_page.map(api_page),
            selected_pages: c.selected_pages.into_iter().map(api_page).collect(),
            selected_objects: Vec::new(),
            zoom: c.zoom,
        }
    }

    fn display_scale(&self, page: PageId) -> Option<f64> {
        let c = self.shared.context.lock().unwrap().clone();
        let doc = c.doc?;
        self.shared.view(doc, page_id(page))?;
        Some(c.zoom * 96.0 / 72.0)
    }

    fn progress(&self, mod_id: &str, fraction: f32, message: &str) {
        self.emit(
            "progress",
            ProgressEvent {
                mod_id: mod_id.into(),
                fraction,
                message: message.into(),
            },
        );
    }
}

/// Native open dialogs. They run on the main thread through the dialog plugin.
pub struct DialogPicker {
    pub shared: Arc<Shared>,
    /// The self-test picks this file without a dialog.
    pub fixed: Option<PathBuf>,
}

impl FilePicker for DialogPicker {
    fn pick(&self, _mod_id: &str, title: &str, filters: &[FileFilter]) -> Option<PathBuf> {
        if let Some(p) = &self.fixed {
            return Some(p.clone());
        }
        let app = self.shared.app.get()?;
        let mut dialog = app.dialog().file().set_title(title);
        for f in filters {
            let ext: Vec<&str> = f.extensions.iter().map(String::as_str).collect();
            dialog = dialog.add_filter(&f.name, &ext);
        }
        dialog.blocking_pick_file()?.into_path().ok()
    }
}

/// Mod settings: one JSON file per mod in `<config>/mod-settings/`.
pub struct FileSettings {
    dir: PathBuf,
    cache: Mutex<HashMap<String, BTreeMap<String, String>>>,
}

impl FileSettings {
    pub fn new(config: &Path) -> Self {
        Self {
            dir: config.join("mod-settings"),
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn file(&self, mod_id: &str) -> PathBuf {
        // Mod IDs are checked on load. Replace other characters anyway.
        let name: String = mod_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "._-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{name}.json"))
    }

    fn with<R>(&self, mod_id: &str, f: impl FnOnce(&mut BTreeMap<String, String>) -> R) -> R {
        let mut cache = self.cache.lock().unwrap();
        let map = cache
            .entry(mod_id.to_string())
            .or_insert_with(|| t_rustable_core::save::read_kept(&self.file(mod_id)));
        f(map)
    }

    fn save(&self, mod_id: &str, map: &BTreeMap<String, String>) {
        let saved = std::fs::create_dir_all(&self.dir)
            .map_err(|e| e.to_string())
            .and_then(|()| serde_json::to_vec_pretty(map).map_err(|e| e.to_string()))
            .and_then(|json| {
                t_rustable_core::save::write_atomic(&self.file(mod_id), &json)
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = saved {
            log::error!(target: &mod_target(mod_id), "The settings were not saved: {e}");
        }
    }
}

impl SettingsStore for FileSettings {
    fn get(&self, mod_id: &str, key: &str) -> Option<String> {
        self.with(mod_id, |m| m.get(key).cloned())
    }

    fn set(&self, mod_id: &str, key: &str, value: &str) {
        self.with(mod_id, |m| {
            m.insert(key.into(), value.into());
            self.save(mod_id, m);
        });
    }

    fn remove(&self, mod_id: &str, key: &str) {
        self.with(mod_id, |m| {
            if m.remove(key).is_some() {
                self.save(mod_id, m);
            }
        });
    }

    fn size(&self, mod_id: &str) -> usize {
        self.with(mod_id, |m| m.iter().map(|(k, v)| k.len() + v.len()).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let s = FileSettings::new(dir.path());
        s.set("a.b", "k", "v");
        s.set("a.b", "k2", "v2");
        s.remove("a.b", "k2");
        let again = FileSettings::new(dir.path());
        assert_eq!(again.get("a.b", "k").as_deref(), Some("v"));
        assert_eq!(again.get("a.b", "k2"), None);
        assert_eq!(again.size("a.b"), 2);
        assert_eq!(again.size("other"), 0);
    }

    #[test]
    fn odd_mod_ids_stay_in_the_folder() {
        let s = FileSettings::new(Path::new("cfg"));
        let f = s.file("../../x");
        assert_eq!(f.parent().unwrap(), Path::new("cfg").join("mod-settings"));
    }
}
