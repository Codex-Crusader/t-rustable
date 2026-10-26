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
    // The batch job uses no UI: each UI call would panic and fail the call.
    let mut host = host(Arc::new(PanicUi), Limits::default());
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
    // Without pages, rotate uses all pages. Delete needs pages.
    run(&mut host, r#"{"action": "rotate", "degrees": 90}"#).unwrap();
    assert_eq!([a, b, c].map(|p| quarter_turns(&doc, p)), [3, 1, 3]);
    assert_eq!(
        run(&mut host, r#"{"action": "delete"}"#).unwrap_err().code,
        "invalid-argument"
    );
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

    // The larger side of the drag sets the size, and the image keeps its aspect ratio.
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
        close_to(found[1].1, [200.0, 100.0, 400.0, 200.0], 0.01),
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
    // Text on the rotated page: replay must use the stored quarter turns.
    host.load_dir(&mod_dir(ADD_TEXT)).unwrap();
    host.on_pointer(
        ADD_TEXT,
        &doc,
        pointer(PointerKind::Down, b, 300.0, 100.0, false),
    )
    .unwrap();
    host.on_text_input(ADD_TEXT, &doc, "Replayed").unwrap();
    host.on_key(ADD_TEXT, &doc, key("Enter")).unwrap();
    assert_eq!(objects(&doc, b).len(), 1);

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

const ADD_TEXT: &str = "t-rustable.add-text";

fn key(name: &str) -> KeyEvent {
    KeyEvent {
        key: name.into(),
        code: name.into(),
        ..escape()
    }
}

fn caret_and_text(ui: &TestUi) -> (Point, Option<String>) {
    let overlay = ui.overlay(ADD_TEXT);
    let caret = overlay
        .iter()
        .find_map(|s| match s {
            OverlayShape::Caret(c) => Some(c.at),
            _ => None,
        })
        .expect("a caret");
    let text = overlay.iter().find_map(|s| match s {
        OverlayShape::Text(t) => Some(t.text.clone()),
        _ => None,
    });
    (caret, text)
}

fn width(text: &str) -> f64 {
    let face = fonts().select("Noto Sans", false, false).unwrap();
    t_rustable_core::fonts::text_width(&face, text, 14.0).unwrap()
}

#[test]
fn add_text_types_on_the_page() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 1);
    let page = pages(&doc)[0];
    let ui = TestUi::new(Duration::ZERO);
    let mut host = host(ui.clone(), Limits::default());
    host.load_dir(&mod_dir(ADD_TEXT)).unwrap();

    host.on_pointer(
        ADD_TEXT,
        &doc,
        pointer(PointerKind::Down, page, 100.0, 700.0, false),
    )
    .unwrap();
    let (caret, text) = caret_and_text(&ui);
    assert_eq!((caret.x, text), (100.0, None));
    host.on_text_input(ADD_TEXT, &doc, "Hello\n!").unwrap();
    let (caret, text) = caret_and_text(&ui);
    assert_eq!(text.as_deref(), Some("Hello!"), "the line break is gone");
    assert!((caret.x - (100.0 + width("Hello!"))).abs() < 1e-9);
    assert!(
        host.on_key(ADD_TEXT, &doc, key("Backspace"))
            .unwrap()
            .value
            .handled
    );
    // A character that the font cannot show is refused with a message.
    host.on_text_input(ADD_TEXT, &doc, "\u{65E5}").unwrap();
    assert_eq!(caret_and_text(&ui).1.as_deref(), Some("Hello"));
    assert_eq!(ui.notes.lock().unwrap().len(), 1);
    let enter = host.on_key(ADD_TEXT, &doc, key("Enter")).unwrap();
    assert_eq!(enter.changes.len(), 1);
    assert!(ui.overlay(ADD_TEXT).is_empty());

    // A click at another place adds the current text. Escape cancels.
    host.on_pointer(
        ADD_TEXT,
        &doc,
        pointer(PointerKind::Down, page, 100.0, 600.0, false),
    )
    .unwrap();
    host.on_text_input(ADD_TEXT, &doc, "Second").unwrap();
    host.on_pointer(
        ADD_TEXT,
        &doc,
        pointer(PointerKind::Down, page, 100.0, 500.0, false),
    )
    .unwrap();
    host.on_text_input(ADD_TEXT, &doc, "Cancelled").unwrap();
    assert!(host.on_key(ADD_TEXT, &doc, escape()).unwrap().value.handled);
    assert!(
        !host
            .on_key(ADD_TEXT, &doc, key("Enter"))
            .unwrap()
            .value
            .handled
    );

    // The action adds text without a pointer.
    ui.set_active_page(page);
    let args = r#"{"x": 100, "y": 400, "text": "From the keyboard", "size": 20}"#;
    host.run_action(ADD_TEXT, Some(&doc), "add-text", args, no_cancel())
        .unwrap()
        .value
        .unwrap();

    let text = doc.lock().unwrap().page_text(page).unwrap();
    for want in ["Hello", "Second", "From the keyboard"] {
        assert!(text.contains(want), "{text:?}");
    }
    assert!(
        !text.contains("Cancelled") && !text.contains("Hello!"),
        "{text:?}"
    );
}

#[test]
fn add_text_follows_a_rotated_page() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 1);
    let page = pages(&doc)[0];
    let ui = TestUi::new(Duration::ZERO);
    let mut host = host(ui.clone(), Limits::default());
    host.load_dir(&mod_dir(PAGE_TOOLS)).unwrap();
    host.load_dir(&mod_dir(ADD_TEXT)).unwrap();
    ui.set_active_page(page);
    action(&mut host, PAGE_TOOLS, &doc, "rotate-right");

    host.on_pointer(
        ADD_TEXT,
        &doc,
        pointer(PointerKind::Down, page, 300.0, 100.0, false),
    )
    .unwrap();
    host.on_text_input(ADD_TEXT, &doc, "Upright").unwrap();
    // The baseline runs up the page, so the caret moves in y.
    let (caret, _) = caret_and_text(&ui);
    assert!(
        (caret.y - (100.0 + width("Upright"))).abs() < 1e-9,
        "{caret:?}"
    );
    host.on_key(ADD_TEXT, &doc, key("Enter")).unwrap();
    let found = objects(&doc, page);
    let [left, bottom, right, top] = found[0].1;
    assert!(top - bottom > 2.0 * (right - left), "{:?}", found[0].1);
}

#[test]
fn add_image_drags_in_any_direction() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 2);
    let [flat, turned] = pages(&doc).try_into().unwrap();
    let file = dir.path().join("picture.png");
    std::fs::write(&file, png(40, 20)).unwrap();
    let ui = TestUi::new(Duration::ZERO);
    let mut host = ModHost::new(services(ui.clone(), Some(file)), Limits::default()).unwrap();
    host.load_dir(&mod_dir(ADD_IMAGE)).unwrap();
    host.load_dir(&mod_dir(PAGE_TOOLS)).unwrap();
    ui.set_active_page(turned);
    action(&mut host, PAGE_TOOLS, &doc, "rotate-right");
    let mut drag = |page: PageId, from: (f64, f64), to: (f64, f64)| {
        action(&mut host, ADD_IMAGE, &doc, "add-image");
        host.on_pointer(
            ADD_IMAGE,
            &doc,
            pointer(PointerKind::Down, page, from.0, from.1, false),
        )
        .unwrap();
        let up = host
            .on_pointer(
                ADD_IMAGE,
                &doc,
                pointer(PointerKind::Up, page, to.0, to.1, false),
            )
            .unwrap();
        assert_eq!(up.changes.len(), 1, "{:?}", ui.notes.lock().unwrap());
    };

    // A vertical drag on a page without rotation: the height sets the size.
    drag(flat, (100.0, 100.0), (100.5, 200.0));
    // On a page rotated by 90 degrees, a drag across the screen goes along user y, and the
    // image width runs along the height of its bounds.
    drag(turned, (200.0, 200.0), (200.5, 300.0));

    let found = objects(&doc, flat);
    assert!(
        close_to(found[0].1, [100.0, 100.0, 300.0, 200.0], 0.01),
        "{:?}",
        found[0].1
    );
    let found = objects(&doc, turned);
    assert!(
        close_to(found[0].1, [200.0, 200.0, 250.0, 300.0], 0.01),
        "{:?}",
        found[0].1
    );
}
