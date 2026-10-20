//! Built-in mod: adds a line of text to a page.
//!
//! Click on a page to put a caret there, then type. Backspace removes the last
//! character, Enter adds the text, and Escape cancels. A click at another place adds
//! the current text and starts a new one. The action `add-text` takes JSON for use
//! without a pointer: `{"x": 72, "y": 700, "text": "Hello", "size": 14}`, on the
//! active page.

use serde::Deserialize;
use std::cell::RefCell;
use t_rustable_sdk::document::{self, Doc};
use t_rustable_sdk::exports::tool::{
    Cursor, DocumentChange, KeyEvent, KeyKind, KeyResult, PointerButton, PointerEvent, PointerKind,
    PointerResult,
};
use t_rustable_sdk::types::{
    ApiError, Color, OverlayCaret, OverlayShape, OverlayText, PageId, Point, TextStyle,
};
use t_rustable_sdk::ui::{self, NotifyLevel};
use t_rustable_sdk::{Command, Tool, api_error, export_interactive, parse_args, run_commands};

const DEFAULT_SIZE: f64 = 14.0;

fn style(size: f64) -> TextStyle {
    TextStyle {
        font_family: "Noto Sans".into(),
        size,
        color: Color::rgb(0, 0, 0),
        bold: false,
        italic: false,
    }
}

/// The text that the user types. Plain data, because handles expire after each call.
struct Editing {
    page: PageId,
    at: Point,
    text: String,
    /// Direction of the baseline in page coordinates. New text reads upright, so on a
    /// rotated page the baseline turns with the page.
    direction: (f64, f64),
}

thread_local! {
    static EDITING: RefCell<Option<Editing>> = const { RefCell::new(None) };
}

fn direction(document: &Doc, page: PageId) -> (f64, f64) {
    match document.page_info(page).map(|i| i.rotation % 360) {
        Ok(90) => (0.0, 1.0),
        Ok(180) => (-1.0, 0.0),
        Ok(270) => (0.0, -1.0),
        _ => (1.0, 0.0),
    }
}

/// The text preview and a caret after its last character.
fn show(e: &Editing) {
    let style = style(DEFAULT_SIZE);
    let width = document::measure_text(&e.text, &style).unwrap_or(0.0);
    let (dx, dy) = e.direction;
    // The caret starts below the baseline and goes up, as the reader sees it.
    let (ux, uy) = (-dy, dx);
    let descent = 0.25 * style.size;
    let caret = Point::new(
        e.at.x + dx * width - ux * descent,
        e.at.y + dy * width - uy * descent,
    );
    let mut shapes = Vec::new();
    if !e.text.is_empty() {
        shapes.push(OverlayShape::Text(OverlayText {
            page: e.page,
            at: e.at,
            text: e.text.clone(),
            style: style.clone(),
            label: format!("New text: {}", e.text),
        }));
    }
    shapes.push(OverlayShape::Caret(OverlayCaret {
        page: e.page,
        at: caret,
        height: 1.2 * style.size,
        color: style.color,
        label: "Text caret".into(),
    }));
    ui::set_overlay(&shapes);
}

/// Adds the text of `e` to the document. Empty text adds nothing.
fn finish(document: &Doc, e: Editing) {
    ui::clear_overlay();
    if e.text.is_empty() {
        return;
    }
    let command = Command::AddText {
        page: e.page,
        at: e.at,
        text: e.text,
        style: style(DEFAULT_SIZE),
    };
    if let Err(err) = run_commands(document, "Add text", vec![command]) {
        ui::notify(NotifyLevel::Error, &err.message);
    }
}

fn take() -> Option<Editing> {
    EDITING.with(|e| e.borrow_mut().take())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    x: f64,
    y: f64,
    text: String,
    #[serde(default)]
    size: Option<f64>,
}

struct AddText;

impl Tool for AddText {
    fn deactivate() {
        // There is no document in this call, so typed text that is not added is lost.
        if take().is_some() {
            ui::clear_overlay();
        }
    }

    fn run_action(document: Option<&Doc>, action_id: String, args: String) -> Result<(), ApiError> {
        // The tool action needs no work. The host makes this mod the active tool.
        if action_id == "text" {
            return Ok(());
        }
        if action_id != "add-text" {
            return Err(api_error(
                "unknown-action",
                format!("Unknown action {action_id:?}."),
            ));
        }
        let document =
            document.ok_or_else(|| api_error("no-document", "Open a document first."))?;
        let args: Args = parse_args(&args)?;
        let page = ui::context()
            .active_page
            .ok_or_else(|| api_error("no-page", "Select a page first."))?;
        let command = Command::AddText {
            page,
            at: Point::new(args.x, args.y),
            text: args.text,
            style: style(args.size.unwrap_or(DEFAULT_SIZE)),
        };
        run_commands(document, "Add text", vec![command])?;
        Ok(())
    }

    fn on_pointer(document: &Doc, event: PointerEvent) -> PointerResult {
        let handled = event.kind == PointerKind::Down && event.button == PointerButton::Primary;
        if handled {
            if let Some(previous) = take() {
                finish(document, previous);
            }
            let e = Editing {
                page: event.page,
                at: event.at,
                text: String::new(),
                direction: direction(document, event.page),
            };
            show(&e);
            EDITING.with(|cell| *cell.borrow_mut() = Some(e));
        }
        PointerResult {
            overlay: Vec::new(),
            cursor: Cursor::Text,
            handled,
        }
    }

    fn on_text_input(_document: &Doc, text: String) {
        EDITING.with(|cell| {
            let mut cell = cell.borrow_mut();
            let Some(e) = cell.as_mut() else {
                return;
            };
            // One line: line breaks and other control characters are not text.
            let typed: String = text.chars().filter(|c| !c.is_control()).collect();
            let next = format!("{}{typed}", e.text);
            match document::measure_text(&next, &style(DEFAULT_SIZE)) {
                Ok(_) => e.text = next,
                Err(err) => ui::notify(NotifyLevel::Warning, &err.message),
            }
            show(e);
        });
    }

    fn on_key(document: &Doc, event: KeyEvent) -> KeyResult {
        if event.kind != KeyKind::Down {
            return KeyResult { handled: false };
        }
        let handled = match event.key.as_str() {
            "Enter" => take().map(|e| finish(document, e)).is_some(),
            "Escape" => take().map(|_| ui::clear_overlay()).is_some(),
            "Backspace" => EDITING.with(|cell| {
                let mut cell = cell.borrow_mut();
                cell.as_mut()
                    .map(|e| {
                        e.text.pop();
                        show(e);
                    })
                    .is_some()
            }),
            _ => false,
        };
        KeyResult { handled }
    }

    fn on_document_changed(document: &Doc, change: DocumentChange) {
        if !change.structure_changed {
            return;
        }
        let pages = document.page_ids();
        let gone = EDITING.with(|e| {
            e.borrow()
                .as_ref()
                .is_some_and(|e| !pages.contains(&e.page))
        });
        if gone {
            take();
            ui::clear_overlay();
        }
    }
}

export_interactive!(AddText);
