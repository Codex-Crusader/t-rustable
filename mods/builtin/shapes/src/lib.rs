//! Built-in mod: draws rectangles, ellipses, and lines.
//!
//! Drag on a page to draw. A preview follows the pointer, and the shape is added when
//! the button comes up. Shift keeps a rectangle or an ellipse square, and a line at
//! a multiple of 45 degrees. Escape cancels.

use std::cell::RefCell;
use t_rustable_sdk::document::Doc;
use t_rustable_sdk::edit::Shape;
use t_rustable_sdk::exports::tool::{
    Cursor, DocumentChange, KeyEvent, KeyKind, KeyResult, Modifiers, PointerButton, PointerEvent,
    PointerKind, PointerResult,
};
use t_rustable_sdk::types::{
    ApiError, Arrowhead, Color, OverlayLine, OverlayRect, OverlayShape, PageId, Point, Rect, Stroke,
};
use t_rustable_sdk::ui::{self, NotifyLevel};
use t_rustable_sdk::{Command, Tool, api_error, export_interactive, run_commands};

/// Drags shorter than this, in points, add nothing.
const MIN_SIZE: f64 = 2.0;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Rectangle,
    Ellipse,
    Line,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Rectangle => "rectangle",
            Kind::Ellipse => "ellipse",
            Kind::Line => "line",
        }
    }
}

struct Drag {
    page: PageId,
    start: Point,
    end: Point,
}

struct State {
    kind: Kind,
    drag: Option<Drag>,
}

thread_local! {
    static STATE: RefCell<State> = const { RefCell::new(State { kind: Kind::Rectangle, drag: None }) };
}

fn stroke() -> Stroke {
    Stroke {
        color: Color::rgb(0, 0, 0),
        width: 1.5,
        dash: Vec::new(),
    }
}

/// The end point after the Shift rule.
fn constrained(kind: Kind, start: Point, at: Point, shift: bool) -> Point {
    if !shift {
        return at;
    }
    let (dx, dy) = (at.x - start.x, at.y - start.y);
    match kind {
        Kind::Rectangle | Kind::Ellipse => {
            let side = dx.abs().max(dy.abs());
            Point::new(start.x + side.copysign(dx), start.y + side.copysign(dy))
        }
        Kind::Line => {
            let step = std::f64::consts::FRAC_PI_4;
            let angle = (dy.atan2(dx) / step).round() * step;
            let length = dx.hypot(dy);
            Point::new(
                start.x + length * angle.cos(),
                start.y + length * angle.sin(),
            )
        }
    }
}

fn too_small(kind: Kind, d: &Drag) -> bool {
    let (dx, dy) = ((d.end.x - d.start.x).abs(), (d.end.y - d.start.y).abs());
    match kind {
        Kind::Line => dx.hypot(dy) < MIN_SIZE,
        _ => dx < MIN_SIZE || dy < MIN_SIZE,
    }
}

fn shape(kind: Kind, d: &Drag) -> Shape {
    match kind {
        Kind::Rectangle => Shape::Rect(Rect::from_corners(d.start, d.end)),
        Kind::Ellipse => Shape::Ellipse(Rect::from_corners(d.start, d.end)),
        Kind::Line => Shape::Line((d.start, d.end)),
    }
}

fn preview(kind: Kind, d: &Drag) -> OverlayShape {
    let label = format!("Preview of the new {}", kind.name());
    match kind {
        Kind::Line => OverlayShape::Line(OverlayLine {
            page: d.page,
            start: d.start,
            end: d.end,
            stroke: stroke(),
            start_head: Arrowhead::None,
            end_head: Arrowhead::None,
            label,
        }),
        _ => {
            let r = OverlayRect {
                page: d.page,
                bounds: Rect::from_corners(d.start, d.end),
                stroke: Some(stroke()),
                fill: None,
                label,
            };
            if kind == Kind::Ellipse {
                OverlayShape::Ellipse(r)
            } else {
                OverlayShape::Rect(r)
            }
        }
    }
}

fn result(overlay: Vec<OverlayShape>, handled: bool) -> PointerResult {
    PointerResult {
        overlay,
        cursor: Cursor::Crosshair,
        handled,
    }
}

struct Shapes;

impl Tool for Shapes {
    fn deactivate() {
        STATE.with(|s| s.borrow_mut().drag = None);
    }

    fn run_action(
        _document: Option<&Doc>,
        action_id: String,
        _args: String,
    ) -> Result<(), ApiError> {
        let kind = match action_id.as_str() {
            "rectangle" => Kind::Rectangle,
            "ellipse" => Kind::Ellipse,
            "line" => Kind::Line,
            _ => {
                return Err(api_error(
                    "unknown-action",
                    format!("Unknown action {action_id:?}."),
                ));
            }
        };
        STATE.with(|s| *s.borrow_mut() = State { kind, drag: None });
        Ok(())
    }

    fn on_pointer(document: &Doc, event: PointerEvent) -> PointerResult {
        STATE.with(|s| {
            let state = &mut *s.borrow_mut();
            let kind = state.kind;
            let shift = event.modifiers.contains(Modifiers::SHIFT);
            match (event.kind, state.drag.as_mut()) {
                (PointerKind::Down, _) if event.button == PointerButton::Primary => {
                    state.drag = Some(Drag {
                        page: event.page,
                        start: event.at,
                        end: event.at,
                    });
                    result(Vec::new(), true)
                }
                (PointerKind::Move, Some(d)) => {
                    // The shape stays on the page where the drag started.
                    if d.page == event.page {
                        d.end = constrained(kind, d.start, event.at, shift);
                    }
                    result(vec![preview(kind, d)], true)
                }
                (PointerKind::Up, Some(d)) => {
                    if d.page == event.page {
                        d.end = constrained(kind, d.start, event.at, shift);
                    }
                    let d = state.drag.take().unwrap();
                    if !too_small(kind, &d) {
                        let command = Command::AddShape {
                            page: d.page,
                            shape: shape(kind, &d),
                            stroke: Some(stroke()),
                            fill: None,
                        };
                        let label = format!("Add a {}", kind.name());
                        if let Err(e) = run_commands(document, &label, vec![command]) {
                            ui::notify(NotifyLevel::Error, &e.message);
                        }
                    }
                    result(Vec::new(), true)
                }
                _ => result(Vec::new(), false),
            }
        })
    }

    fn on_key(_document: &Doc, event: KeyEvent) -> KeyResult {
        let handled = event.kind == KeyKind::Down
            && event.key == "Escape"
            && STATE.with(|s| s.borrow_mut().drag.take().is_some());
        KeyResult { handled }
    }

    fn on_document_changed(document: &Doc, change: DocumentChange) {
        if change.structure_changed {
            let pages = document.page_ids();
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                if s.drag.as_ref().is_some_and(|d| !pages.contains(&d.page)) {
                    s.drag = None;
                }
            });
        }
    }
}

export_interactive!(Shapes);
