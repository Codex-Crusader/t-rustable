//! The example mods in `mods/examples/` work through the host, as an author sees them.

mod common;

use common::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use t_rustable_core::PageId;
use t_rustable_engine::ObjectKind;
use t_rustable_plugin_host::api::tool::{Modifiers, PointerButton, PointerEvent, PointerKind};
use t_rustable_plugin_host::api::types::{OverlayShape, Point};
use t_rustable_plugin_host::bindings::v0_1::api_page;
use t_rustable_plugin_host::{Limits, ModHost, SessionRef};

const STAMP: &str = "example.stamp";
const PAGE_NUMBERS: &str = "example.page-numbers";

fn example_dir(id: &str) -> PathBuf {
    let dir = repo().join("target/examples").join(id);
    assert!(
        dir.join("manifest.json").is_file(),
        "{} is missing. Run: cargo xtask build-mods",
        dir.display()
    );
    dir
}

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn pages(doc: &SessionRef) -> Vec<PageId> {
    doc.lock().unwrap().page_ids()
}

/// Bounds of the text objects of a page: left, bottom, right, top.
fn text_bounds(doc: &SessionRef, page: PageId) -> Vec<[f64; 4]> {
    let mut s = doc.lock().unwrap();
    s.objects(page)
        .unwrap()
        .into_iter()
        .filter(|o| o.info.kind == ObjectKind::Text)
        .map(|o| {
            let b = o.info.bounds;
            [b.left, b.bottom, b.right, b.top]
        })
        .collect()
}

fn batch(host: &mut ModHost, id: &str, doc: &SessionRef, args: &str) -> Result<String, String> {
    host.run_batch(id, doc, args, no_cancel())
        .unwrap()
        .value
        .map_err(|e| e.code)
}

#[test]
fn page_numbers_go_to_the_bottom_as_the_reader_sees_it() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 3);
    let mut host = host(TestUi::new(Duration::ZERO), Limits::default());
    host.load_dir(&mod_dir("t-rustable.page-tools")).unwrap();
    host.load_dir(&example_dir(PAGE_NUMBERS)).unwrap();
    batch(
        &mut host,
        "t-rustable.page-tools",
        &doc,
        r#"{"action": "rotate", "pages": [2], "degrees": 90}"#,
    )
    .unwrap();

    assert_eq!(
        batch(&mut host, PAGE_NUMBERS, &doc, "").unwrap(),
        r#"{"pages":3}"#
    );
    let [a, b, c] = pages(&doc).try_into().unwrap();
    for (i, page) in [a, b, c].into_iter().enumerate() {
        let text = doc.lock().unwrap().page_text(page).unwrap();
        assert!(text.contains(&format!("Page {} of 3", i + 1)), "{text:?}");
    }
    // Page 1: centered near the bottom edge of the 612 by 792 page.
    let [l, bo, r, t] = text_bounds(&doc, a)[0];
    assert!(bo > 15.0 && t < 45.0, "{bo} {t}");
    assert!(((l + r) / 2.0 - 306.0).abs() < 2.0, "{l} {r}");
    // Page 2 turns 90 degrees clockwise, so its bottom is the right edge, and the text runs up.
    let [l, bo, r, t] = text_bounds(&doc, b)[0];
    assert!(l > 560.0 && r < 600.0, "{l} {r}");
    assert!(t - bo > 2.0 * (r - l), "the text must run along the edge");
    assert!(((bo + t) / 2.0 - 396.0).abs() < 2.0, "{bo} {t}");

    // One undo removes all numbers.
    doc.lock().unwrap().undo().unwrap();
    assert!([a, b, c].iter().all(|&p| text_bounds(&doc, p).is_empty()));

    // Options.
    batch(
        &mut host,
        PAGE_NUMBERS,
        &doc,
        r#"{"format": "{n}", "position": "right", "size": 12}"#,
    )
    .unwrap();
    let [_, _, r, _] = text_bounds(&doc, a)[0];
    // The glyph bounds end a little before the advance width that the mod measures.
    assert!((r - (612.0 - 24.0)).abs() < 4.0, "{r}");
    for bad in [
        r#"{"size": 1}"#,
        r#"{"colour": "red"}"#,
        r#"{"position": "top"}"#,
    ] {
        assert_eq!(
            batch(&mut host, PAGE_NUMBERS, &doc, bad).unwrap_err(),
            "invalid-argument",
            "{bad}"
        );
    }
}

fn pointer(kind: PointerKind, page: PageId, x: f64, y: f64) -> PointerEvent {
    PointerEvent {
        kind,
        page: api_page(page),
        at: Point { x, y },
        button: if kind == PointerKind::Down {
            PointerButton::Primary
        } else {
            PointerButton::None
        },
        modifiers: Modifiers::empty(),
        pressure: 0.5,
    }
}

#[test]
fn the_stamp_tool_follows_its_panel() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 1);
    let page = pages(&doc)[0];
    let mut host = host(TestUi::new(Duration::ZERO), Limits::default());
    host.load_dir(&example_dir(STAMP)).unwrap();
    host.activate(STAMP).unwrap().value.unwrap();

    let moved = host
        .on_pointer(STAMP, &doc, pointer(PointerKind::Move, page, 300.0, 400.0))
        .unwrap();
    let labels: Vec<String> = moved
        .value
        .overlay
        .iter()
        .map(|s| match s {
            OverlayShape::Rect(r) => r.label.clone(),
            OverlayShape::Text(t) => t.label.clone(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(
        labels,
        [
            "Frame of the stamp APPROVED",
            "Preview of the stamp APPROVED"
        ]
    );

    let down = host
        .on_pointer(STAMP, &doc, pointer(PointerKind::Down, page, 300.0, 400.0))
        .unwrap();
    assert_eq!(down.changes.len(), 1, "one transaction");
    assert!(
        doc.lock()
            .unwrap()
            .page_text(page)
            .unwrap()
            .contains("APPROVED")
    );
    let [l, bo, r, t] = text_bounds(&doc, page)[0];
    assert!(((l + r) / 2.0 - 300.0).abs() < 3.0 && ((bo + t) / 2.0 - 400.0).abs() < 8.0);

    let set = |host: &mut ModHost, message: &str| {
        host.on_panel_message(STAMP, "options", message)
            .unwrap()
            .value
            .unwrap();
    };
    set(&mut host, r#"{"kind":"set","text":"DRAFT","color":"blue"}"#);
    // Refused choices keep the old one. The panel gets an error message.
    set(&mut host, r#"{"kind":"set","text":"   ","color":"blue"}"#);
    set(&mut host, r#"{"kind":"set","text":"OK","color":"purple"}"#);
    set(&mut host, "not json");
    host.on_pointer(STAMP, &doc, pointer(PointerKind::Down, page, 300.0, 200.0))
        .unwrap();
    let text = doc.lock().unwrap().page_text(page).unwrap();
    assert!(text.contains("DRAFT") && !text.contains("OK"), "{text:?}");
}
