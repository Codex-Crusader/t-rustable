//! The template and the built-in mods change a document through the host, and the
//! journal replays without the mods.

mod common;

use common::*;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use t_rustable_core::{PageId, Session};
use t_rustable_engine::{ObjectKind, Priority};
use t_rustable_plugin_host::api::tool::{
    KeyEvent, KeyKind, Modifiers, PointerButton, PointerEvent, PointerKind,
};
use t_rustable_plugin_host::api::types::{OverlayShape, Point};
use t_rustable_plugin_host::bindings::v0_1::api_page;
use t_rustable_plugin_host::{Limits, ModHost, SessionRef};

const PAGE_TOOLS: &str = "t-rustable.page-tools";
const SHAPES: &str = "t-rustable.shapes";
const ADD_IMAGE: &str = "t-rustable.add-image";

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn action(host: &mut ModHost, id: &str, doc: &SessionRef, action_id: &str) {
    let out = host
        .run_action(id, Some(doc), action_id, "", no_cancel())
        .unwrap();
    out.value.unwrap_or_else(|e| panic!("{action_id}: {e:?}"));
}

fn pointer(kind: PointerKind, page: PageId, x: f64, y: f64, shift: bool) -> PointerEvent {
    PointerEvent {
        kind,
        page: api_page(page),
        at: Point { x, y },
        button: if kind == PointerKind::Move {
            PointerButton::None
        } else {
            PointerButton::Primary
        },
        modifiers: if shift {
            Modifiers::SHIFT
        } else {
            Modifiers::empty()
        },
        pressure: 0.5,
    }
}

fn escape() -> KeyEvent {
    KeyEvent {
        kind: KeyKind::Down,
        key: "Escape".into(),
        code: "Escape".into(),
        modifiers: Modifiers::empty(),
        repeat: false,
    }
}

fn pages(doc: &SessionRef) -> Vec<PageId> {
    doc.lock().unwrap().page_ids()
}

fn objects(doc: &SessionRef, page: PageId) -> Vec<(ObjectKind, [f64; 4])> {
    let mut s = doc.lock().unwrap();
    s.objects(page)
        .unwrap()
        .into_iter()
        .map(|o| {
            let b = o.info.bounds;
            (o.info.kind, [b.left, b.bottom, b.right, b.top])
        })
        .collect()
}

fn close_to(a: [f64; 4], b: [f64; 4], tolerance: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tolerance)
}

/// Pages, objects, text, and pixels of each page.
fn snapshot(s: &mut Session) -> Vec<(PageId, usize, String, Vec<u8>)> {
    s.page_ids()
        .into_iter()
        .map(|p| {
            let objects = s.objects(p).unwrap().len();
            let text = s.page_text(p).unwrap();
            let bitmap = s.render(p, 0.25, None, Priority::Normal).unwrap();
            (p, objects, text, bitmap.bgra)
        })
        .collect()
}

#[test]
fn the_template_rotates_the_active_page() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 2);
    let ui = TestUi::new(Duration::ZERO);
    let mut host = host(ui.clone(), Limits::default());
    let id = host.load_dir(&mod_dir("com.example.rotate-page")).unwrap();
    let second = pages(&doc)[1];
    ui.set_active_page(second);
    action(&mut host, &id, &doc, "rotate-page");
    assert_eq!(quarter_turns(&doc, second), 1);
    assert_eq!(quarter_turns(&doc, pages(&doc)[0]), 0);
}

#[test]
fn page_tools_change_the_selected_pages() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 3);
    let ui = TestUi::new(Duration::ZERO);
    let mut host = host(ui.clone(), Limits::default());
    host.load_dir(&mod_dir(PAGE_TOOLS)).unwrap();
    let [a, b, c] = pages(&doc).try_into().unwrap();

    ui.set_active_page(b);
    action(&mut host, PAGE_TOOLS, &doc, "rotate-left");
    assert_eq!(quarter_turns(&doc, b), 3);
    action(&mut host, PAGE_TOOLS, &doc, "move-up");
    assert_eq!(pages(&doc), [b, a, c]);
    action(&mut host, PAGE_TOOLS, &doc, "move-up");
    assert_eq!(pages(&doc), [b, a, c], "the first page stays");
    action(&mut host, PAGE_TOOLS, &doc, "move-down");
    action(&mut host, PAGE_TOOLS, &doc, "move-down");
    assert_eq!(pages(&doc), [a, c, b]);

    // Two selected pages move together, and rotate in one transaction.
    ui.select_pages(&[c, b]);
    action(&mut host, PAGE_TOOLS, &doc, "move-up");
    assert_eq!(pages(&doc), [c, b, a]);
    let out = host
        .run_action(PAGE_TOOLS, Some(&doc), "rotate-right", "", no_cancel())
        .unwrap();
    assert_eq!(out.changes.len(), 1);
    assert_eq!((quarter_turns(&doc, c), quarter_turns(&doc, b)), (1, 0));

    ui.set_active_page(c);
    action(&mut host, PAGE_TOOLS, &doc, "insert-blank-after");
    let after = pages(&doc);
    assert_eq!(after.len(), 4);
    assert_eq!((after[0], after[2], after[3]), (c, b, a));
    action(&mut host, PAGE_TOOLS, &doc, "delete");
    assert_eq!(pages(&doc).len(), 3);

    // Deleting every page is the mod's own error, not a failure.
    ui.select_pages(&pages(&doc));
    let out = host
        .run_action(PAGE_TOOLS, Some(&doc), "delete", "", no_cancel())
        .unwrap();
    assert_eq!(out.value.unwrap_err().code, "invalid-argument");
    assert_eq!(pages(&doc).len(), 3);
}

#[test]
fn page_tools_run_as_a_batch_job() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 3);
    // The batch job uses no UI: a UI that panics would show it.
    let mut host = host(TestUi::new(Duration::ZERO), Limits::default());
    host.load_dir(&mod_dir(PAGE_TOOLS)).unwrap();
    let [a, b, c] = pages(&doc).try_into().unwrap();
    let run = |host: &mut ModHost, args: &str| {
        host.run_batch(PAGE_TOOLS, &doc, args, no_cancel())
            .unwrap()
            .value
    };

    let reply = run(
        &mut host,
        r#"{"action": "rotate", "pages": [1, 3], "degrees": 180}"#,
    )
    .unwrap();
    assert_eq!(reply, r#"{"pages":3}"#);
    assert_eq!([a, b, c].map(|p| quarter_turns(&doc, p)), [2, 0, 2]);
    run(&mut host, r#"{"action": "move-down", "pages": [1]}"#).unwrap();
    assert_eq!(pages(&doc), [b, a, c]);
    let reply = run(&mut host, r#"{"action": "insert-blank-after", "page": 3}"#).unwrap();
    assert_eq!(reply, r#"{"pages":4}"#);
    assert_eq!(&pages(&doc)[..3], [b, a, c]);
    run(&mut host, r#"{"action": "delete", "pages": [4]}"#).unwrap();
    assert_eq!(pages(&doc), [b, a, c]);

    assert_eq!(
        run(&mut host, r#"{"action": "delete", "pages": [9]}"#)
            .unwrap_err()
            .code,
        "not-found"
    );
    assert_eq!(
        run(&mut host, r#"{"action": "spin"}"#).unwrap_err().code,
        "invalid-argument"
    );
}

#[test]
fn shapes_draw_with_a_labelled_preview() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 1);
    let page = pages(&doc)[0];
    let mut host = host(TestUi::new(Duration::ZERO), Limits::default());
    host.load_dir(&mod_dir(SHAPES)).unwrap();
    host.activate(SHAPES).unwrap().value.unwrap();
    let drag = |host: &mut ModHost, to: (f64, f64), shift: bool| {
        host.on_pointer(
            SHAPES,
            &doc,
            pointer(PointerKind::Down, page, 100.0, 100.0, false),
        )
        .unwrap();
        let moved = host
            .on_pointer(
                SHAPES,
                &doc,
                pointer(PointerKind::Move, page, to.0, to.1, shift),
            )
            .unwrap();
        host.on_pointer(
            SHAPES,
            &doc,
            pointer(PointerKind::Up, page, to.0, to.1, shift),
        )
        .unwrap();
        moved.value.overlay
    };

    let overlay = drag(&mut host, (200.0, 150.0), false);
    let [OverlayShape::Rect(r)] = overlay.as_slice() else {
        panic!("one rectangle preview, not {overlay:?}");
    };
    assert!(!r.label.is_empty());
    assert_eq!((r.bounds.right, r.bounds.top), (200.0, 150.0));
    // Shift keeps the ellipse square.
    action(&mut host, SHAPES, &doc, "ellipse");
    let overlay = drag(&mut host, (160.0, 300.0), true);
    let [OverlayShape::Ellipse(r)] = overlay.as_slice() else {
        panic!("one ellipse preview, not {overlay:?}");
    };
    assert_eq!((r.bounds.right, r.bounds.top), (300.0, 300.0));
    // A click adds nothing.
    drag(&mut host, (100.5, 100.5), false);
    action(&mut host, SHAPES, &doc, "line");
    drag(&mut host, (100.0, 400.0), false);

    let found = objects(&doc, page);
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(found.iter().all(|(kind, _)| *kind == ObjectKind::Path));
    // PDFium adds the stroke width (1.5 points) to the bounds on each side.
    assert!(
        close_to(found[0].1, [98.5, 98.5, 201.5, 151.5], 0.01),
        "{:?}",
        found[0].1
    );
    assert!(
        close_to(found[1].1, [98.5, 98.5, 301.5, 301.5], 0.01),
        "{:?}",
        found[1].1
    );

    // Escape cancels a drag.
    host.on_pointer(
        SHAPES,
        &doc,
        pointer(PointerKind::Down, page, 10.0, 10.0, false),
    )
    .unwrap();
    assert!(host.on_key(SHAPES, &doc, escape()).unwrap().value.handled);
    host.on_pointer(
        SHAPES,
        &doc,
        pointer(PointerKind::Up, page, 50.0, 50.0, false),
    )
    .unwrap();
    assert_eq!(objects(&doc, page).len(), 3);
}

#[test]
fn add_image_places_the_picked_file() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 1);
    let page = pages(&doc)[0];
    let file = dir.path().join("picture.png");
    std::fs::write(&file, png(40, 20)).unwrap();
    let ui = TestUi::new(Duration::ZERO);
    let mut host = ModHost::new(services(ui.clone(), Some(file)), Limits::default()).unwrap();
    host.load_dir(&mod_dir(ADD_IMAGE)).unwrap();

    // Before the action, the tool does nothing.
    let idle = host
        .on_pointer(
            ADD_IMAGE,
            &doc,
            pointer(PointerKind::Move, page, 50.0, 50.0, false),
        )
        .unwrap();
    assert!(!idle.value.handled);

    action(&mut host, ADD_IMAGE, &doc, "add-image");
    assert_eq!(ui.images.lock().unwrap().len(), 1);
    let hover = host
        .on_pointer(
            ADD_IMAGE,
            &doc,
            pointer(PointerKind::Move, page, 100.0, 100.0, false),
        )
        .unwrap();
    let [OverlayShape::Image(preview)] = hover.value.overlay.as_slice() else {
        panic!("one image preview, not {:?}", hover.value.overlay);
    };
    assert!(!preview.label.is_empty());
    // 40 by 20 pixels at 96 pixels per inch is 30 by 15 points.
    let b = preview.bounds;
    assert_eq!(
        [b.left, b.bottom, b.right, b.top],
        [85.0, 92.5, 115.0, 107.5]
    );

    host.on_pointer(
        ADD_IMAGE,
        &doc,
        pointer(PointerKind::Down, page, 100.0, 100.0, false),
    )
    .unwrap();
    let up = host
        .on_pointer(
            ADD_IMAGE,
            &doc,
            pointer(PointerKind::Up, page, 100.0, 100.0, false),
        )
        .unwrap();
    assert_eq!(up.changes.len(), 1);
    let found = objects(&doc, page);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, ObjectKind::Image);
    assert!(
        close_to(found[0].1, [85.0, 92.5, 115.0, 107.5], 0.01),
        "{:?}",
        found[0].1
    );

    // One image per pick: the next click does nothing.
    let again = host
        .on_pointer(
            ADD_IMAGE,
            &doc,
            pointer(PointerKind::Down, page, 10.0, 10.0, false),
        )
        .unwrap();
    assert!(!again.value.handled);

    // A drag sets the width, and the image keeps its aspect ratio.
    action(&mut host, ADD_IMAGE, &doc, "add-image");
    host.on_pointer(
        ADD_IMAGE,
        &doc,
        pointer(PointerKind::Down, page, 200.0, 200.0, false),
    )
    .unwrap();
    host.on_pointer(
        ADD_IMAGE,
        &doc,
        pointer(PointerKind::Up, page, 300.0, 100.0, false),
    )
    .unwrap();
    let found = objects(&doc, page);
    assert!(
        close_to(found[1].1, [200.0, 150.0, 300.0, 200.0], 0.01),
        "{:?}",
        found[1].1
    );
}

#[test]
fn the_journal_replays_without_the_mods() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 3);
    let ui = TestUi::new(Duration::ZERO);
    let mut host = host(ui.clone(), Limits::default());
    host.load_dir(&mod_dir(PAGE_TOOLS)).unwrap();
    host.load_dir(&mod_dir(SHAPES)).unwrap();
    let [a, b, _] = pages(&doc).try_into().unwrap();
    ui.set_active_page(b);
    action(&mut host, PAGE_TOOLS, &doc, "rotate-right");
    action(&mut host, PAGE_TOOLS, &doc, "move-up");
    host.on_pointer(
        SHAPES,
        &doc,
        pointer(PointerKind::Down, a, 50.0, 50.0, false),
    )
    .unwrap();
    host.on_pointer(
        SHAPES,
        &doc,
        pointer(PointerKind::Up, a, 250.0, 120.0, false),
    )
    .unwrap();

    let want = snapshot(&mut doc.lock().unwrap());
    let journal = doc.lock().unwrap().journal_dir().to_path_buf();
    // The app stops without a close. Recovery does not load any mod.
    drop(host);
    drop(Arc::try_unwrap(doc).ok().unwrap());
    let mut recovered = Session::recover(&e, &journal, None).unwrap();
    assert_eq!(snapshot(&mut recovered), want);
    assert_eq!(recovered.page_ids()[0], b);
    recovered.close().unwrap();
}
