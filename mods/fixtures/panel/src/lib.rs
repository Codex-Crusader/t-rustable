//! Test mod for panels. It sends each panel message back with the prefix "echo:".

use t_rustable_sdk::types::ApiError;
use t_rustable_sdk::{Tool, export_interactive, ui};

struct Panel;

impl Tool for Panel {
    fn on_panel_message(panel_id: String, message: String) -> Result<(), ApiError> {
        ui::post_to_panel(&panel_id, &format!("echo:{message}"))
    }
}

export_interactive!(Panel);
