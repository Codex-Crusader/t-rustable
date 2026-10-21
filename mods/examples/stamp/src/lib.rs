//! Example mod: a stamp tool with a panel.
//!
//! Select the Stamp tool, then click on a page. The stamp text in a frame goes where you
//! click, and it reads upright on a rotated page. A preview follows the pointer. The
//! Stamp panel sets the text and the color. The mod keeps them in its settings.
//!
//! Panel messages are JSON strings:
//! - from the panel: `{"kind":"hello"}` or `{"kind":"set","text":"DRAFT","color":"blue"}`
//! - to the panel: `{"kind":"state","text":"DRAFT","color":"blue","colors":[...]}` or
//!   `{"kind":"error","message":"..."}`

use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use t_rustable_sdk::document::Doc;
use t_rustable_sdk::edit::Shape;
use t_rustable_sdk::exports::tool::{
    Cursor, DocumentChange, PointerButton, PointerEvent, PointerKind, PointerResult,
};
use t_rustable_sdk::types::{
    ApiError, Color, OverlayRect, OverlayShape, OverlayText, PageId, Point, Rect, Stroke, TextStyle,
};
use t_rustable_sdk::ui::{self, NotifyLevel};
use t_rustable_sdk::{Command, Tool, api_error, document, export_interactive, host, run_commands};

const SIZE: f64 = 24.0;
/// Space between the text and the frame, in points.
const PAD: f64 = 6.0;
const MAX_TEXT: usize = 40;
const COLORS: [(&str, u8, u8, u8); 4] = [
    ("red", 200, 30, 30),
    ("blue", 30, 70, 200),
    ("green", 20, 130, 60),
    ("black", 0, 0, 0),
];

struct State {
    text: String,
    color: String,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// The current choice, read from the settings the first time.
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let state = s.get_or_insert_with(|| State {
            text: host::setting_get("text").unwrap_or_else(|| "APPROVED".into()),
            color: host::setting_get("color")
                .filter(|c| color(c).is_some())
                .unwrap_or_else(|| "red".into()),
        });
        f(state)
    })
}

fn color(name: &str) -> Option<Color> {
    COLORS
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|&(_, r, g, b)| Color::rgb(r, g, b))
}

fn style(c: Color) -> TextStyle {
    TextStyle {
        font_family: "Noto Sans".into(),
        size: SIZE,
        color: c,
        bold: true,
        italic: false,
    }
}

/// The baseline direction and the up direction in page coordinates. New text reads
/// upright, so on a rotated page both turn with the page.
fn axes(document: &Doc, page: PageId) -> ((f64, f64), (f64, f64)) {
    let d = match document.page_info(page).map(|i| i.rotation % 360) {
        Ok(90) => (0.0, 1.0),
        Ok(180) => (-1.0, 0.0),
        Ok(270) => (0.0, -1.0),
        _ => (1.0, 0.0),
    };
    (d, (-d.1, d.0))
}

/// Where the text starts and the frame, for a stamp centered at `at`.
struct Layout {
    start: Point,
    frame: Rect,
}

fn layout(document: &Doc, page: PageId, at: Point, text: &str, c: Color) -> Layout {
    let width = document::measure_text(text, &style(c)).unwrap_or(SIZE * text.len() as f64 * 0.6);
    let ((dx, dy), (ux, uy)) = axes(document, page);
    // The baseline sits a third of the size below the center, so the capitals look centered.
    let start = Point::new(
        at.x - dx * width / 2.0 - ux * SIZE * 0.35,
        at.y - dy * width / 2.0 - uy * SIZE * 0.35,
    );
    let (half_w, half_h) = (width / 2.0 + PAD, SIZE * 0.5 + PAD);
    let a = Point::new(
        at.x - dx * half_w - ux * half_h,
        at.y - dy * half_w - uy * half_h,
    );
    let b = Point::new(
        at.x + dx * half_w + ux * half_h,
        at.y + dy * half_w + uy * half_h,
    );
    Layout {
        start,
        frame: Rect::from_corners(a, b),
    }
}

fn stroke(c: Color) -> Stroke {
    Stroke {
        color: c,
        width: 2.0,
        dash: Vec::new(),
    }
}

fn preview(document: &Doc, page: PageId, at: Point) -> Vec<OverlayShape> {
    with_state(|s| {
        let c = color(&s.color).unwrap_or(Color::BLACK);
        let l = layout(document, page, at, &s.text, c);
        vec![
            OverlayShape::Rect(OverlayRect {
                page,
                bounds: l.frame,
                stroke: Some(stroke(c)),
                fill: None,
                label: format!("Frame of the stamp {}", s.text),
            }),
            OverlayShape::Text(OverlayText {
                page,
                at: l.start,
                text: s.text.clone(),
                style: style(c),
                label: format!("Preview of the stamp {}", s.text),
            }),
        ]
    })
}

fn add(document: &Doc, page: PageId, at: Point) -> Result<(), ApiError> {
    let (text, c) = with_state(|s| (s.text.clone(), color(&s.color).unwrap_or(Color::BLACK)));
    let l = layout(document, page, at, &text, c);
    let commands = vec![
        Command::AddShape {
            page,
            shape: Shape::Rect(l.frame),
            stroke: Some(stroke(c)),
            fill: None,
        },
        Command::AddText {
            page,
            at: l.start,
            text: text.clone(),
            style: style(c),
        },
    ];
    run_commands(document, &format!("Stamp {text}"), commands).map(drop)
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum FromPanel {
    Hello,
    Set { text: String, color: String },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum ToPanel<'a> {
    State {
        text: &'a str,
        color: &'a str,
        colors: Vec<&'static str>,
    },
    Error {
        message: String,
    },
}

fn send(panel_id: &str, message: &ToPanel) -> Result<(), ApiError> {
    ui::post_to_panel(
        panel_id,
        &serde_json::to_string(message).unwrap_or_default(),
    )
}

fn send_state(panel_id: &str) -> Result<(), ApiError> {
    with_state(|s| {
        send(
            panel_id,
            &ToPanel::State {
                text: &s.text,
                color: &s.color,
                colors: COLORS.iter().map(|c| c.0).collect(),
            },
        )
    })
}

/// Checks and stores a new choice. Returns the problem in words.
fn set(text: &str, color_name: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        return Err(format!("Use 1 to {MAX_TEXT} characters."));
    }
    if color(color_name).is_none() {
        return Err(format!("{color_name:?} is not a stamp color."));
    }
    if let Err(e) = document::measure_text(text, &style(Color::BLACK)) {
        return Err(e.message);
    }
    host::setting_set("text", text).map_err(|e| e.message)?;
    host::setting_set("color", color_name).map_err(|e| e.message)?;
    with_state(|s| {
        s.text = text.to_string();
        s.color = color_name.to_string();
    });
    Ok(())
}

struct Stamp;

impl Tool for Stamp {
    fn deactivate() {
        ui::clear_overlay();
    }

    fn run_action(
        _document: Option<&Doc>,
        action_id: String,
        _args: String,
    ) -> Result<(), ApiError> {
        match action_id.as_str() {
            "stamp" => Ok(()),
            _ => Err(api_error(
                "unknown-action",
                format!("Unknown action {action_id:?}."),
            )),
        }
    }

    fn on_pointer(document: &Doc, event: PointerEvent) -> PointerResult {
        let (overlay, handled) = match event.kind {
            PointerKind::Move => (preview(document, event.page, event.at), true),
            PointerKind::Down if event.button == PointerButton::Primary => {
                if let Err(e) = add(document, event.page, event.at) {
                    ui::notify(NotifyLevel::Error, &e.message);
                }
                (preview(document, event.page, event.at), true)
            }
            _ => (Vec::new(), false),
        };
        PointerResult {
            overlay,
            cursor: Cursor::Crosshair,
            handled,
        }
    }

    fn on_panel_message(panel_id: String, message: String) -> Result<(), ApiError> {
        match serde_json::from_str::<FromPanel>(&message) {
            Ok(FromPanel::Hello) => send_state(&panel_id),
            Ok(FromPanel::Set { text, color }) => match set(&text, &color) {
                Ok(()) => send_state(&panel_id),
                Err(message) => send(&panel_id, &ToPanel::Error { message }),
            },
            Err(e) => send(
                &panel_id,
                &ToPanel::Error {
                    message: format!("The message is not valid: {e}."),
                },
            ),
        }
    }

    fn on_document_changed(_document: &Doc, _change: DocumentChange) {}
}

export_interactive!(Stamp);
