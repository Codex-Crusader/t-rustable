//! Test mod for the limits of the plugin host. Each action misbehaves in one way.

use std::cell::RefCell;
use std::hint::black_box;
use t_rustable_sdk::document::Doc;
use t_rustable_sdk::edit::{self, Transaction};
use t_rustable_sdk::types::ApiError;
use t_rustable_sdk::{Command, Tool, api_error, export_interactive, run_commands, ui};

thread_local! {
    static KEPT: RefCell<Option<Transaction>> = const { RefCell::new(None) };
}

struct Misbehave;

impl Tool for Misbehave {
    fn run_action(document: Option<&Doc>, action_id: String, args: String) -> Result<(), ApiError> {
        match action_id.as_str() {
            "ok" => Ok(()),
            "loop" => {
                let mut n = 0u64;
                loop {
                    n = black_box(n.wrapping_add(1));
                }
            }
            "alloc" => {
                let mb: usize = args
                    .parse()
                    .map_err(|_| api_error("invalid-argument", "Give the size in MB."))?;
                let v = vec![1u8; mb << 20];
                black_box(&v);
                Ok(())
            }
            "trap" => panic!("trap on request"),
            "slow-host" => {
                // The test host waits in each `confirm` call. That time does not count.
                let times: u32 = args.parse().unwrap_or(5);
                for _ in 0..times {
                    black_box(ui::confirm("Wait", "The test host waits here."));
                }
                Ok(())
            }
            "commit-then-trap" => {
                let document =
                    document.ok_or_else(|| api_error("no-document", "Open a document first."))?;
                let page = document.page_ids()[0];
                run_commands(
                    document,
                    "Rotate",
                    vec![Command::RotatePage { page, degrees: 90 }],
                )?;
                panic!("trap after commit");
            }
            "keep-transaction" => {
                let document =
                    document.ok_or_else(|| api_error("no-document", "Open a document first."))?;
                let tx = edit::begin(document, "Kept")?;
                KEPT.with(|k| *k.borrow_mut() = Some(tx));
                Ok(())
            }
            "use-kept-transaction" => {
                let document =
                    document.ok_or_else(|| api_error("no-document", "Open a document first."))?;
                let tx = KEPT
                    .with(|k| k.borrow_mut().take())
                    .ok_or_else(|| api_error("not-found", "No kept transaction."))?;
                tx.rotate_page(document.page_ids()[0], 90)
            }
            _ => Err(api_error(
                "unknown-action",
                format!("Unknown action {action_id:?}."),
            )),
        }
    }
}

export_interactive!(Misbehave);
