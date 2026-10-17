//! Test mod: a batch job that adds blank pages at the end of the document.

use serde::{Deserialize, Serialize};
use t_rustable_sdk::document::Doc;
use t_rustable_sdk::types::ApiError;
use t_rustable_sdk::{BatchJob, Command, export_batch, parse_args, run_commands};

#[derive(Deserialize)]
struct Args {
    count: u32,
}

#[derive(Serialize)]
struct Reply {
    pages_before: usize,
    pages_after: usize,
}

struct AddPages;

impl BatchJob for AddPages {
    fn run(document: &Doc, args: String) -> Result<String, ApiError> {
        let args: Args = parse_args(&args)?;
        let pages_before = document.page_ids().len();
        let commands = (0..args.count)
            .map(|_| Command::InsertBlankPage {
                before: None,
                width: 612.0,
                height: 792.0,
            })
            .collect();
        run_commands(document, "Add blank pages", commands)?;
        let reply = Reply {
            pages_before,
            pages_after: document.page_ids().len(),
        };
        Ok(serde_json::to_string(&reply).unwrap())
    }
}

export_batch!(AddPages);
