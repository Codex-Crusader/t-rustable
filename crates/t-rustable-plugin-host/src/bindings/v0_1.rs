//! Bindings for `t-rustable:api@0.1.x`, and conversions to core types.

// Code from the wasmtime `bindgen!` macro. It contains `unsafe` blocks.
#[allow(unsafe_code, clippy::all)]
pub mod generated {
    wasmtime::component::bindgen!({
        world: "full",
        path: "../../wit",
        with: {
            "t-rustable:api/document.doc": crate::state::DocRes,
            "t-rustable:api/edit.transaction": crate::state::TxRes,
            "t-rustable:api/files.picked-file": crate::state::PickedRes,
        },
    });
}

use generated::t_rustable::api::{document as d, edit as e, types as t};
use t_rustable_core::command::Shape;
use t_rustable_core::{CoreError, ObjectId, PageId};
use t_rustable_engine as engine;

pub fn page_id(id: t::PageId) -> PageId {
    PageId(((id.hi as u128) << 64) | id.lo as u128)
}

pub fn object_id(id: t::ObjectId) -> ObjectId {
    ObjectId(((id.hi as u128) << 64) | id.lo as u128)
}

pub fn api_page(id: PageId) -> t::PageId {
    t::PageId {
        hi: (id.0 >> 64) as u64,
        lo: id.0 as u64,
    }
}

pub fn api_object(id: ObjectId) -> t::ObjectId {
    t::ObjectId {
        hi: (id.0 >> 64) as u64,
        lo: id.0 as u64,
    }
}

pub fn point(p: t::Point) -> engine::Point {
    engine::Point { x: p.x, y: p.y }
}

pub fn api_point(p: engine::Point) -> t::Point {
    t::Point { x: p.x, y: p.y }
}

pub fn rect(r: t::Rect) -> engine::Rect {
    engine::Rect {
        left: r.left,
        bottom: r.bottom,
        right: r.right,
        top: r.top,
    }
}

pub fn api_rect(r: engine::Rect) -> t::Rect {
    t::Rect {
        left: r.left,
        bottom: r.bottom,
        right: r.right,
        top: r.top,
    }
}

pub fn color(c: t::Color) -> engine::Rgba {
    engine::Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

pub fn stroke(s: t::Stroke) -> engine::Stroke {
    engine::Stroke {
        color: color(s.color),
        width: s.width,
        dash: s.dash,
    }
}

pub fn shape(s: e::Shape) -> Shape {
    match s {
        e::Shape::Rect(r) => Shape::Rect(rect(r)),
        e::Shape::Ellipse(r) => Shape::Ellipse(rect(r)),
        e::Shape::Line((a, b)) => Shape::Line(point(a), point(b)),
        e::Shape::Polyline(points) => Shape::Polyline(points.into_iter().map(point).collect()),
    }
}

pub fn error(e: CoreError) -> t::ApiError {
    t::ApiError {
        code: e.code.to_string(),
        message: e.message,
        hint: e.hint,
        details: e.details,
        command_index: e.command_index.map(|i| i as u32),
    }
}

pub fn api_error(code: &str, message: impl Into<String>) -> t::ApiError {
    t::ApiError {
        code: code.into(),
        message: message.into(),
        hint: None,
        details: None,
        command_index: None,
    }
}

pub fn object_kind(k: engine::ObjectKind) -> d::ObjectKind {
    match k {
        engine::ObjectKind::Text => d::ObjectKind::Text,
        engine::ObjectKind::Image => d::ObjectKind::Image,
        engine::ObjectKind::Path => d::ObjectKind::Path,
        engine::ObjectKind::Shading => d::ObjectKind::Shading,
        engine::ObjectKind::Form => d::ObjectKind::FormXobject,
        engine::ObjectKind::Unknown => d::ObjectKind::Unknown,
    }
}

pub fn field_kind(k: engine::FieldKind) -> d::FormFieldKind {
    match k {
        engine::FieldKind::Text => d::FormFieldKind::Text,
        engine::FieldKind::Checkbox => d::FormFieldKind::Checkbox,
        engine::FieldKind::Radio => d::FormFieldKind::Radio,
        engine::FieldKind::ComboBox => d::FormFieldKind::ComboBox,
        engine::FieldKind::ListBox => d::FormFieldKind::ListBox,
        engine::FieldKind::PushButton => d::FormFieldKind::PushButton,
        engine::FieldKind::Signature => d::FormFieldKind::Signature,
        engine::FieldKind::Unknown => d::FormFieldKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        let id = PageId(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210);
        let api = api_page(id);
        assert_eq!(
            (api.hi, api.lo),
            (0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210)
        );
        assert_eq!(page_id(api_page(id)), id);
        let o = ObjectId(u128::MAX - 5);
        assert_eq!(object_id(api_object(o)), o);
    }
}
