//! Data that goes between the Rust side and the UI.
//!
//! All positions for the UI are display points: points from the top left of the crop box,
//! after the page rotation, at zoom 1. The UI multiplies them by its zoom. Only Rust
//! converts between display points and PDF coordinates (`PageView`).

use serde::{Deserialize, Serialize};
use t_rustable_core::PageId;
use t_rustable_core::coords::PageView;
use t_rustable_engine::{Point, Rect};
use t_rustable_plugin_host::api::{tool, types as t};
use t_rustable_plugin_host::bindings::v0_1::{api_page, page_id, point, rect};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Box2 {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn display_point(view: &PageView, p: Point) -> [f64; 2] {
    let d = view.to_display(p);
    [d.x, d.y]
}

pub fn display_box(view: &PageView, r: Rect) -> Box2 {
    let a = view.to_display(Point {
        x: r.left,
        y: r.bottom,
    });
    let b = view.to_display(Point {
        x: r.right,
        y: r.top,
    });
    Box2 {
        x: a.x.min(b.x),
        y: a.y.min(b.y),
        w: (a.x - b.x).abs(),
        h: (a.y - b.y).abs(),
    }
}

/// A CSS color.
pub fn css(c: t::Color) -> String {
    format!(
        "rgba({},{},{},{:.3})",
        c.r,
        c.g,
        c.b,
        f64::from(c.a) / 255.0
    )
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeDto {
    pub color: String,
    pub width: f64,
    pub dash: Vec<f64>,
}

fn stroke(s: &t::Stroke) -> StrokeDto {
    StrokeDto {
        color: css(s.color),
        width: s.width,
        dash: s.dash.clone(),
    }
}

/// One overlay shape in display points. Text, images, and carets are upright (ADR 0017).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ShapeDto {
    #[serde(rename_all = "camelCase")]
    Rect {
        page: PageId,
        bounds: Box2,
        ellipse: bool,
        stroke: Option<StrokeDto>,
        fill: Option<String>,
        label: String,
    },
    #[serde(rename_all = "camelCase")]
    Line {
        page: PageId,
        start: [f64; 2],
        end: [f64; 2],
        stroke: StrokeDto,
        start_head: String,
        end_head: String,
        label: String,
    },
    #[serde(rename_all = "camelCase")]
    Polyline {
        page: PageId,
        points: Vec<[f64; 2]>,
        closed: bool,
        stroke: Option<StrokeDto>,
        fill: Option<String>,
        label: String,
    },
    #[serde(rename_all = "camelCase")]
    Text {
        page: PageId,
        at: [f64; 2],
        text: String,
        font_family: String,
        size: f64,
        color: String,
        bold: bool,
        italic: bool,
        label: String,
    },
    #[serde(rename_all = "camelCase")]
    Image {
        page: PageId,
        bounds: Box2,
        image: u32,
        opacity: f32,
        label: String,
    },
    #[serde(rename_all = "camelCase")]
    Caret {
        page: PageId,
        at: [f64; 2],
        height: f64,
        color: String,
        label: String,
    },
    #[serde(rename_all = "camelCase")]
    Handle {
        page: PageId,
        at: [f64; 2],
        kind: String,
        label: String,
    },
}

fn head(h: t::Arrowhead) -> String {
    match h {
        t::Arrowhead::None => "none",
        t::Arrowhead::Open => "open",
        t::Arrowhead::Closed => "closed",
    }
    .into()
}

/// The page of a shape.
pub fn shape_page(s: &t::OverlayShape) -> PageId {
    use t::OverlayShape as S;
    page_id(match s {
        S::Rect(r) | S::Ellipse(r) => r.page,
        S::Line(l) => l.page,
        S::Polyline(p) => p.page,
        S::Text(x) => x.page,
        S::Image(i) => i.page,
        S::Caret(c) => c.page,
        S::Handle(h) => h.page,
    })
}

/// Converts a shape to display points. `view` is the view of the shape's page at zoom 1.
pub fn shape(view: &PageView, s: &t::OverlayShape) -> ShapeDto {
    use t::OverlayShape as S;
    let page = shape_page(s);
    let at = |p: t::Point| display_point(view, point(p));
    match s {
        S::Rect(r) | S::Ellipse(r) => ShapeDto::Rect {
            page,
            bounds: display_box(view, rect(r.bounds)),
            ellipse: matches!(s, S::Ellipse(_)),
            stroke: r.stroke.as_ref().map(stroke),
            fill: r.fill.map(css),
            label: r.label.clone(),
        },
        S::Line(l) => ShapeDto::Line {
            page,
            start: at(l.start),
            end: at(l.end),
            stroke: stroke(&l.stroke),
            start_head: head(l.start_head),
            end_head: head(l.end_head),
            label: l.label.clone(),
        },
        S::Polyline(p) => ShapeDto::Polyline {
            page,
            points: p.points.iter().map(|&q| at(q)).collect(),
            closed: p.closed,
            stroke: p.stroke.as_ref().map(stroke),
            fill: p.fill.map(css),
            label: p.label.clone(),
        },
        S::Text(x) => ShapeDto::Text {
            page,
            at: at(x.at),
            text: x.text.clone(),
            font_family: x.style.font_family.clone(),
            size: x.style.size,
            color: css(x.style.color),
            bold: x.style.bold,
            italic: x.style.italic,
            label: x.label.clone(),
        },
        S::Image(i) => ShapeDto::Image {
            page,
            bounds: display_box(view, rect(i.bounds)),
            image: i.image,
            opacity: i.opacity,
            label: i.label.clone(),
        },
        S::Caret(c) => ShapeDto::Caret {
            page,
            at: at(c.at),
            height: c.height,
            color: css(c.color),
            label: c.label.clone(),
        },
        S::Handle(h) => ShapeDto::Handle {
            page,
            at: at(h.at),
            kind: format!("{:?}", h.kind).to_lowercase(),
            label: h.label.clone(),
        },
    }
}

/// Keyboard modifiers from the UI.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Mods {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
}

fn modifiers(m: Mods) -> tool::Modifiers {
    let mut out = tool::Modifiers::empty();
    for (on, flag) in [
        (m.shift, tool::Modifiers::SHIFT),
        (m.ctrl, tool::Modifiers::CTRL),
        (m.alt, tool::Modifiers::ALT),
        (m.meta, tool::Modifiers::META),
    ] {
        if on {
            out |= flag;
        }
    }
    out
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerIn {
    pub kind: String,
    pub page: PageId,
    /// Display points at zoom 1.
    pub x: f64,
    pub y: f64,
    pub button: String,
    #[serde(flatten)]
    pub mods: Mods,
    #[serde(default)]
    pub pressure: f32,
}

pub fn pointer(view: &PageView, e: &PointerIn) -> Result<tool::PointerEvent, String> {
    use tool::{PointerButton as B, PointerKind as K};
    let kind = match e.kind.as_str() {
        "down" => K::Down,
        "move" => K::Move,
        "up" => K::Up,
        "enter" => K::Enter,
        "leave" => K::Leave,
        "double-click" => K::DoubleClick,
        k => return Err(format!("unknown pointer kind {k:?}")),
    };
    let button = match e.button.as_str() {
        "primary" => B::Primary,
        "secondary" => B::Secondary,
        "middle" => B::Middle,
        _ => B::None,
    };
    let p = view.from_display(Point { x: e.x, y: e.y });
    Ok(tool::PointerEvent {
        kind,
        page: api_page(e.page),
        at: t::Point { x: p.x, y: p.y },
        button,
        modifiers: modifiers(e.mods),
        pressure: e.pressure,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyIn {
    pub kind: String,
    pub key: String,
    pub code: String,
    #[serde(flatten)]
    pub mods: Mods,
    #[serde(default)]
    pub repeat: bool,
}

pub fn key(e: &KeyIn) -> tool::KeyEvent {
    tool::KeyEvent {
        kind: if e.kind == "up" {
            tool::KeyKind::Up
        } else {
            tool::KeyKind::Down
        },
        key: e.key.clone(),
        code: e.code.clone(),
        modifiers: modifiers(e.mods),
        repeat: e.repeat,
    }
}

/// The CSS cursor name of a tool cursor.
pub fn cursor(c: tool::Cursor) -> &'static str {
    use tool::Cursor as C;
    match c {
        C::Default => "default",
        C::Crosshair => "crosshair",
        C::Text => "text",
        C::Move => "move",
        C::Pointer => "pointer",
        C::Grab => "grab",
        C::Grabbing => "grabbing",
        C::NotAllowed => "not-allowed",
        C::ResizeNs => "ns-resize",
        C::ResizeEw => "ew-resize",
        C::ResizeNesw => "nesw-resize",
        C::ResizeNwse => "nwse-resize",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(quarter_turns: u8) -> PageView {
        PageView {
            crop_box: Rect {
                left: 0.0,
                bottom: 0.0,
                right: 600.0,
                top: 800.0,
            },
            quarter_turns,
            scale: 1.0,
        }
    }

    #[test]
    fn boxes_follow_the_page_rotation() {
        let r = Rect {
            left: 10.0,
            bottom: 20.0,
            right: 110.0,
            top: 70.0,
        };
        let b = display_box(&view(0), r);
        assert_eq!(
            b,
            Box2 {
                x: 10.0,
                y: 730.0,
                w: 100.0,
                h: 50.0
            }
        );
        // On a page turned by 90 degrees, width and height change places.
        let b = display_box(&view(1), r);
        assert_eq!((b.w, b.h), (50.0, 100.0));
    }

    #[test]
    fn pointer_positions_go_back_to_pdf_space() {
        for turns in 0..4 {
            let v = view(turns);
            let pdf = Point { x: 123.0, y: 456.0 };
            let [x, y] = display_point(&v, pdf);
            let e = PointerIn {
                kind: "down".into(),
                page: PageId(1),
                x,
                y,
                button: "primary".into(),
                mods: Mods {
                    shift: true,
                    ..Mods::default()
                },
                pressure: 0.5,
            };
            let out = pointer(&v, &e).unwrap();
            assert!((out.at.x - 123.0).abs() < 1e-9 && (out.at.y - 456.0).abs() < 1e-9);
            assert!(out.modifiers.contains(tool::Modifiers::SHIFT));
        }
    }

    #[test]
    fn colors_are_css() {
        let c = t::Color {
            r: 255,
            g: 0,
            b: 10,
            a: 255,
        };
        assert_eq!(css(c), "rgba(255,0,10,1.000)");
    }
}
