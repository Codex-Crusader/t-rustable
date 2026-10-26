//! Built-in mod: rotates, deletes, moves, and inserts pages.
//!
//! In the GUI, the actions use the selected pages, else the active page.
//! In the CLI, the batch job takes JSON with 1-based page numbers, for example
//! `{"action": "rotate", "pages": [1, 3], "degrees": 90}`. Only `rotate` uses all pages
//! when `pages` is missing.

use serde::{Deserialize, Serialize};
use t_rustable_sdk::document::Doc;
use t_rustable_sdk::types::{ApiError, PageId};
use t_rustable_sdk::{
    BatchJob, Command, Tool, api_error, export_full, parse_args, run_commands, ui,
};

/// One page operation, on pages in document order.
enum Op {
    Rotate(i32),
    Delete,
    MoveUp,
    MoveDown,
    InsertBlankAfter,
}

fn label(op: &Op, count: usize) -> String {
    let pages = if count == 1 {
        "page".to_string()
    } else {
        format!("{count} pages")
    };
    match op {
        Op::Rotate(d) if *d < 0 => format!("Rotate {pages} left"),
        Op::Rotate(_) => format!("Rotate {pages} right"),
        Op::Delete => format!("Delete {pages}"),
        Op::MoveUp => format!("Move {pages} up"),
        Op::MoveDown => format!("Move {pages} down"),
        Op::InsertBlankAfter => "Insert a blank page".to_string(),
    }
}

/// The commands for `op` on `pages`. `order` is the page order of the document.
fn commands(
    document: &Doc,
    order: &[PageId],
    op: &Op,
    pages: &[PageId],
) -> Result<Vec<Command>, ApiError> {
    let selected = |p: &PageId| pages.contains(p);
    let in_order: Vec<PageId> = order.iter().copied().filter(selected).collect();
    Ok(match op {
        Op::Rotate(degrees) => in_order
            .iter()
            .map(|&page| Command::RotatePage {
                page,
                degrees: *degrees,
            })
            .collect(),
        Op::Delete => {
            if in_order.len() == order.len() {
                let mut e = api_error(
                    "invalid-argument",
                    "A document must keep at least one page.",
                );
                e.hint = Some("Select fewer pages.".into());
                return Err(e);
            }
            in_order
                .iter()
                .map(|&page| Command::DeletePage(page))
                .collect()
        }
        Op::MoveUp => {
            // Each selected page moves before the page above it. Pages at the start stay.
            let mut order = order.to_vec();
            let mut out = Vec::new();
            for i in 1..order.len() {
                if selected(&order[i]) && !selected(&order[i - 1]) {
                    out.push(Command::MovePage {
                        page: order[i],
                        before: Some(order[i - 1]),
                    });
                    order.swap(i - 1, i);
                }
            }
            out
        }
        Op::MoveDown => {
            let mut order = order.to_vec();
            let mut out = Vec::new();
            for i in (0..order.len().saturating_sub(1)).rev() {
                if selected(&order[i]) && !selected(&order[i + 1]) {
                    let before = order.get(i + 2).copied();
                    out.push(Command::MovePage {
                        page: order[i],
                        before,
                    });
                    order.swap(i, i + 1);
                }
            }
            out
        }
        Op::InsertBlankAfter => {
            let last = *in_order.last().ok_or_else(no_pages)?;
            let info = document.page_info(last)?;
            let at = order.iter().position(|p| *p == last).unwrap();
            vec![Command::InsertBlankPage {
                before: order.get(at + 1).copied(),
                width: info.width,
                height: info.height,
            }]
        }
    })
}

fn no_pages() -> ApiError {
    api_error("no-page", "Select a page first.")
}

fn run(document: &Doc, op: Op, pages: &[PageId]) -> Result<(), ApiError> {
    if pages.is_empty() {
        return Err(no_pages());
    }
    let order = document.page_ids();
    let commands = commands(document, &order, &op, pages)?;
    if !commands.is_empty() {
        run_commands(document, &label(&op, pages.len()), commands)?;
    }
    Ok(())
}

struct PageTools;

impl Tool for PageTools {
    fn run_action(
        document: Option<&Doc>,
        action_id: String,
        _args: String,
    ) -> Result<(), ApiError> {
        let op = match action_id.as_str() {
            "rotate-left" => Op::Rotate(-90),
            "rotate-right" => Op::Rotate(90),
            "delete" => Op::Delete,
            "move-up" => Op::MoveUp,
            "move-down" => Op::MoveDown,
            "insert-blank-after" => Op::InsertBlankAfter,
            _ => {
                return Err(api_error(
                    "unknown-action",
                    format!("Unknown action {action_id:?}."),
                ));
            }
        };
        let document =
            document.ok_or_else(|| api_error("no-document", "Open a document first."))?;
        let context = ui::context();
        let pages = if context.selected_pages.is_empty() {
            context.active_page.into_iter().collect()
        } else {
            context.selected_pages
        };
        run(document, op, &pages)
    }
}

/// Batch arguments. Page numbers start at 1.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
enum BatchArgs {
    Rotate {
        #[serde(default)]
        pages: Option<Vec<u32>>,
        degrees: i32,
    },
    Delete {
        pages: Vec<u32>,
    },
    MoveUp {
        pages: Vec<u32>,
    },
    MoveDown {
        pages: Vec<u32>,
    },
    InsertBlankAfter {
        page: u32,
    },
}

#[derive(Serialize)]
struct BatchReply {
    pages: usize,
}

fn page_numbers(order: &[PageId], numbers: &[u32]) -> Result<Vec<PageId>, ApiError> {
    numbers
        .iter()
        .map(|&n| {
            order
                .get((n as usize).wrapping_sub(1))
                .copied()
                .ok_or_else(|| {
                    api_error(
                        "not-found",
                        format!(
                            "Page {n} does not exist. The document has {} pages.",
                            order.len()
                        ),
                    )
                })
        })
        .collect()
}

impl BatchJob for PageTools {
    fn run(document: &Doc, args: String) -> Result<String, ApiError> {
        let args: BatchArgs = parse_args(&args)?;
        let order = document.page_ids();
        let (op, numbers) = match args {
            BatchArgs::Rotate { pages, degrees } => {
                let all = || (1..=order.len() as u32).collect();
                (Op::Rotate(degrees), pages.unwrap_or_else(all))
            }
            BatchArgs::Delete { pages } => (Op::Delete, pages),
            BatchArgs::MoveUp { pages } => (Op::MoveUp, pages),
            BatchArgs::MoveDown { pages } => (Op::MoveDown, pages),
            BatchArgs::InsertBlankAfter { page } => (Op::InsertBlankAfter, vec![page]),
        };
        run(document, op, &page_numbers(&order, &numbers)?)?;
        let reply = BatchReply {
            pages: document.page_ids().len(),
        };
        Ok(serde_json::to_string(&reply).unwrap())
    }
}

export_full!(PageTools);
