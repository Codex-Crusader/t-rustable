//! Template mod: one action that rotates the current page by 90 degrees.

use t_rustable_sdk::document::Doc;
use t_rustable_sdk::types::ApiError;
use t_rustable_sdk::{Command, Tool, api_error, export_interactive, run_commands, ui};

struct RotatePage;

impl Tool for RotatePage {
    fn run_action(
        document: Option<&Doc>,
        action_id: String,
        _args: String,
    ) -> Result<(), ApiError> {
        if action_id != "rotate-page" {
            return Err(api_error(
                "unknown-action",
                format!("Unknown action {action_id:?}."),
            ));
        }
        let document =
            document.ok_or_else(|| api_error("no-document", "Open a document first."))?;
        let page = ui::context()
            .active_page
            .ok_or_else(|| api_error("no-page", "Select a page first."))?;
        run_commands(
            document,
            "Rotate page",
            vec![Command::RotatePage { page, degrees: 90 }],
        )?;
        Ok(())
    }
}

export_interactive!(RotatePage);
