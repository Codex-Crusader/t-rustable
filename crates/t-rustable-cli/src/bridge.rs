//! The UI and the file picker of a batch run. The command line has no UI, so it cannot
//! ask the user. The notes go into the result of the file.

use std::path::PathBuf;
use std::sync::Mutex;

use t_rustable_plugin_host::api::types::{OverlayShape, PageId};
use t_rustable_plugin_host::api::ui::{
    Action, NotifyLevel, Panel, Shortcut, ToolbarItem, UiContext,
};
use t_rustable_plugin_host::services::{FileFilter, FilePicker, UiBridge};

const NO_UI: &str = "The command line has no UI.";
/// A mod can send messages in a loop. The rest are only counted.
const MAX_NOTES: usize = 100;

/// Collects the messages of the mods for the current file.
#[derive(Default)]
pub struct Notes {
    notes: Mutex<(Vec<String>, usize)>,
}

impl Notes {
    fn add(&self, note: String) {
        let mut n = self.notes.lock().unwrap();
        if n.0.len() < MAX_NOTES {
            n.0.push(note);
        } else {
            n.1 += 1;
        }
    }

    pub fn take(&self) -> Vec<String> {
        let (mut notes, more) = std::mem::take(&mut *self.notes.lock().unwrap());
        if more > 0 {
            notes.push(format!("The mods sent {more} more messages."));
        }
        notes
    }
}

pub struct CliUi(pub std::sync::Arc<Notes>);

impl UiBridge for CliUi {
    fn register_action(&self, _: &str, _: Action) -> Result<(), String> {
        Err(NO_UI.into())
    }
    fn register_toolbar_item(&self, _: &str, _: ToolbarItem) -> Result<(), String> {
        Err(NO_UI.into())
    }
    fn register_panel(&self, _: &str, _: Panel) -> Result<(), String> {
        Err(NO_UI.into())
    }
    fn suggest_shortcut(&self, _: &str, _: Shortcut) -> Result<(), String> {
        Err(NO_UI.into())
    }
    fn set_overlay(&self, _: &str, _: Vec<OverlayShape>) {}
    fn clear_overlay(&self, _: &str) {}
    fn register_overlay_image(&self, _: &str, _: Vec<u8>) -> Result<u32, String> {
        Err(NO_UI.into())
    }
    fn post_to_panel(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
        Err(NO_UI.into())
    }
    fn notify(&self, mod_id: &str, level: NotifyLevel, message: &str) {
        let level = match level {
            NotifyLevel::Info => "info",
            NotifyLevel::Success => "success",
            NotifyLevel::Warning => "warning",
            NotifyLevel::Error => "error",
        };
        self.0.add(format!("{mod_id} ({level}): {message}"));
    }
    fn confirm(&self, mod_id: &str, title: &str, _: &str) -> bool {
        self.0.add(format!(
            "{mod_id} asked the question \"{title}\". The command line cannot ask, so the answer was no. Use the app to answer it."
        ));
        false
    }
    fn context(&self, _: &str) -> UiContext {
        UiContext {
            document_open: false,
            active_page: None,
            selected_pages: Vec::new(),
            selected_objects: Vec::new(),
            zoom: 1.0,
        }
    }
    fn display_scale(&self, _: PageId) -> Option<f64> {
        None
    }
    fn progress(&self, mod_id: &str, fraction: f32, message: &str) {
        let target = t_rustable_plugin_host::services::mod_target(mod_id);
        log::info!(target: &target, "{:.0}% {message}", fraction * 100.0);
    }
}

pub struct CliFiles(pub std::sync::Arc<Notes>);

impl FilePicker for CliFiles {
    fn pick(&self, mod_id: &str, title: &str, _: &[FileFilter]) -> Option<PathBuf> {
        self.0.add(format!(
            "{mod_id} asked for a file (\"{title}\"). The command line cannot ask, so no file was picked. Use the app."
        ));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn questions_get_no_and_a_note() {
        let notes = Arc::new(Notes::default());
        let (ui, files) = (CliUi(notes.clone()), CliFiles(notes.clone()));
        assert!(!ui.confirm("m.x", "Delete all?", "Sure?"));
        assert_eq!(files.pick("m.x", "Pick an image", &[]), None);
        ui.notify("m.x", NotifyLevel::Warning, "Careful");
        assert!(
            ui.register_panel(
                "m.x",
                Panel {
                    id: "p".into(),
                    title: "P".into(),
                    entry: "p.html".into()
                }
            )
            .is_err()
        );
        let n = notes.take();
        assert_eq!(n.len(), 3);
        assert!(n[0].contains("Delete all?") && n[0].contains("answer was no"));
        assert!(n[1].contains("Pick an image"));
        assert_eq!(n[2], "m.x (warning): Careful");
        assert!(notes.take().is_empty());
    }

    #[test]
    fn a_mod_in_a_loop_cannot_fill_the_notes() {
        let notes = Arc::new(Notes::default());
        let ui = CliUi(notes.clone());
        for _ in 0..MAX_NOTES + 5 {
            ui.notify("m.x", NotifyLevel::Info, "Again");
        }
        let n = notes.take();
        assert_eq!(n.len(), MAX_NOTES + 1);
        assert_eq!(n[MAX_NOTES], "The mods sent 5 more messages.");
    }
}
