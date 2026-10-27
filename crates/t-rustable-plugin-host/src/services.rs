//! The outside world of the host. The GUI and the CLI implement these traits.

use crate::api::types::{OverlayShape, PageId};
use crate::api::ui::{Action, NotifyLevel, Panel, Shortcut, ToolbarItem, UiContext};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// The longest mod text, in bytes, for the log, a progress message, or a notice.
pub const MAX_TEXT: usize = 2048;

/// Mod text that is safe for a log file or a terminal. Control characters and
/// direction overrides become spaces, so the text cannot start a new line, send a
/// terminal escape, or show in a false order. The text is cut to `max` bytes.
pub fn printable(text: &str, max: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max));
    for c in text.chars() {
        if out.len() + c.len_utf8() > max {
            break;
        }
        let unsafe_char =
            c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
        out.push(if unsafe_char { ' ' } else { c });
    }
    out
}

/// Registrations, overlays, messages, and the page view. All methods get the mod ID.
pub trait UiBridge: Send + Sync {
    fn register_action(&self, mod_id: &str, action: Action) -> Result<(), String>;
    fn register_toolbar_item(&self, mod_id: &str, item: ToolbarItem) -> Result<(), String>;
    fn register_panel(&self, mod_id: &str, panel: Panel) -> Result<(), String>;
    fn suggest_shortcut(&self, mod_id: &str, shortcut: Shortcut) -> Result<(), String>;
    fn set_overlay(&self, mod_id: &str, shapes: Vec<OverlayShape>);
    fn clear_overlay(&self, mod_id: &str);
    /// The data is a PNG or JPEG file. The host checks the format first.
    fn register_overlay_image(&self, mod_id: &str, data: Vec<u8>) -> Result<u32, String>;
    fn post_to_panel(&self, mod_id: &str, panel_id: &str, message: &str) -> Result<(), String>;
    fn notify(&self, mod_id: &str, level: NotifyLevel, message: &str);
    /// The time limit of the call does not count while this waits for the user.
    fn confirm(&self, mod_id: &str, title: &str, message: &str) -> bool;
    fn context(&self, mod_id: &str) -> UiContext;
    /// CSS pixels per PDF point of the page view, or None if the page is not shown.
    fn display_scale(&self, page: PageId) -> Option<f64>;
    fn progress(&self, mod_id: &str, fraction: f32, message: &str);
}

pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

pub trait FilePicker: Send + Sync {
    /// Shows an open dialog. None when the user cancels.
    fn pick(&self, mod_id: &str, title: &str, filters: &[FileFilter]) -> Option<PathBuf>;
}

/// Settings per mod. The host enforces the size limit.
pub trait SettingsStore: Send + Sync {
    fn get(&self, mod_id: &str, key: &str) -> Option<String>;
    fn set(&self, mod_id: &str, key: &str, value: &str);
    fn remove(&self, mod_id: &str, key: &str);
    /// Bytes of all keys and values of the mod.
    fn size(&self, mod_id: &str) -> usize;
}

/// Settings in memory, for tests and the CLI.
#[derive(Default)]
pub struct MemorySettings {
    map: Mutex<BTreeMap<(String, String), String>>,
}

impl SettingsStore for MemorySettings {
    fn get(&self, mod_id: &str, key: &str) -> Option<String> {
        self.map
            .lock()
            .unwrap()
            .get(&(mod_id.into(), key.into()))
            .cloned()
    }

    fn set(&self, mod_id: &str, key: &str, value: &str) {
        self.map
            .lock()
            .unwrap()
            .insert((mod_id.into(), key.into()), value.into());
    }

    fn remove(&self, mod_id: &str, key: &str) {
        self.map
            .lock()
            .unwrap()
            .remove(&(mod_id.into(), key.into()));
    }

    fn size(&self, mod_id: &str) -> usize {
        self.map
            .lock()
            .unwrap()
            .iter()
            .filter(|((m, _), _)| m == mod_id)
            .map(|((_, k), v)| k.len() + v.len())
            .sum()
    }
}

/// A file picker that never picks a file, for the CLI.
pub struct NoFiles;

impl FilePicker for NoFiles {
    fn pick(&self, _: &str, _: &str, _: &[FileFilter]) -> Option<PathBuf> {
        None
    }
}
