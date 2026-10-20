use std::path::Path;
use std::sync::Arc;
use t_rustable_core::command::{Command, Shape};
use t_rustable_core::error::code;
use t_rustable_core::fonts::Fonts;
use t_rustable_core::{AssetId, ObjectId, OpenOptions, PageId, SaveOptions, Session};
use t_rustable_engine::testing::{
    blank_objects, blank_pdf, build_pdf, engine, outline_pdf, png, stream,
};
use t_rustable_engine::{Engine, Point, Priority, Rect, Request, Rgba, Stroke};

const RED: Rgba = Rgba {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};
const BLACK: Rgba = Rgba {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts")).unwrap()
}

fn open(e: &Engine, dir: &Path, bytes: Vec<u8>) -> Session {
    let options = OpenOptions {
        sessions_dir: dir,
        password: None,
        source_name: Some("test.pdf".into()),
    };
    Session::open(e, bytes.into(), options).unwrap()
}

fn rect(l: f64, b: f64, r: f64, t: f64) -> Rect {
    Rect {
        left: l,
        bottom: b,
        right: r,
        top: t,
    }
}

fn add_rect(page: PageId, r: Rect) -> (ObjectId, Command) {
    let id = ObjectId::random();
    (
        id,
        Command::AddShape {
            id,
            page,
            shape: Shape::Rect(r),
            stroke: None,
            fill: Some(RED),
        },
    )
}

fn add_text(page: PageId, font: AssetId, at: (f64, f64), text: &str) -> (ObjectId, Command) {
    let id = ObjectId::random();
    let c = Command::AddText {
        id,
        page,
        at: Point { x: at.0, y: at.1 },
        text: text.into(),
        font,
        size: 14.0,
        color: BLACK,
        turns: None,
    };
    (id, c)
}

fn commit(
    s: &mut Session,
    commands: Vec<Command>,
) -> t_rustable_core::Result<t_rustable_core::Change> {
    let mut tx = s.begin("test", "user");
    for c in commands {
        s.add(&mut tx, c)?;
    }
    s.commit(tx)
}

/// Pages, object IDs, text, and a small render of each page.
fn snapshot(s: &mut Session) -> Vec<(PageId, Vec<ObjectId>, String, Vec<u8>)> {
    s.page_ids()
        .into_iter()
        .map(|p| {
            let ids = s.objects(p).unwrap().into_iter().map(|o| o.id).collect();
            let text = s.page_text(p).unwrap();
            let bitmap = s.render(p, 0.25, None, Priority::Normal).unwrap();
            (p, ids, text, bitmap.bgra)
        })
        .collect()
}

#[test]
fn edits_keep_stable_ids() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), blank_pdf(&[(300.0, 300.0), (300.0, 300.0)]));
    let font = s.add_asset(fonts().select("Noto Sans", false, false).unwrap());
    let image = s.add_asset(Arc::from(png(8, 8)));
    let [p0, p1] = [s.page_ids()[0], s.page_ids()[1]];
    let new_page = PageId::random();
    let (text_id, text) = add_text(new_page, font, (20.0, 200.0), "On a new page");
    let (rect_id, rect_cmd) = add_rect(p0, rect(10.0, 10.0, 60.0, 60.0));
    let image_id = ObjectId::random();
    let change = commit(
        &mut s,
        vec![
            Command::InsertBlankPage {
                id: new_page,
                before: Some(p1),
                width: 200.0,
                height: 300.0,
            },
            text,
            rect_cmd,
            Command::AddImage {
                id: image_id,
                page: p0,
                bounds: rect(100.0, 100.0, 150.0, 150.0),
                image,
                turns: None,
            },
        ],
    )
    .unwrap();
    assert!(change.structure_changed);
    assert_eq!(s.page_ids(), [p0, new_page, p1]);
    let ids: Vec<ObjectId> = s.objects(p0).unwrap().into_iter().map(|o| o.id).collect();
    assert_eq!(ids, [rect_id, image_id]);
    let runs = s.text_runs(new_page).unwrap();
    assert_eq!(runs[0].0, Some(text_id));
    assert_eq!(runs[0].1.text, "On a new page");

    // Remove by ID. The transaction finds the page of the object.
    let mut tx = s.begin("Remove", "user");
    let page = tx.locate(rect_id).unwrap();
    s.add(
        &mut tx,
        Command::RemoveObject {
            page,
            object: rect_id,
        },
    )
    .unwrap();
    s.commit(tx).unwrap();
    let ids: Vec<ObjectId> = s.objects(p0).unwrap().into_iter().map(|o| o.id).collect();
    assert_eq!(ids, [image_id]);
    assert_eq!(s.page(new_page).unwrap().label, "2");
    s.close().unwrap();
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "close deletes the journal"
    );
}

#[test]
fn add_checks_against_the_state_before() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), blank_pdf(&[(300.0, 300.0), (300.0, 300.0)]));
    let [p0, p1] = [s.page_ids()[0], s.page_ids()[1]];
    let mut tx = s.begin("two deletes", "user");
    s.add(&mut tx, Command::DeletePage { page: p0 }).unwrap();
    let err = s
        .add(&mut tx, Command::DeletePage { page: p1 })
        .unwrap_err();
    assert_eq!(
        (err.code, err.command_index),
        (code::INVALID_ARGUMENT, Some(1))
    );
    assert_eq!(tx.commands().len(), 1);
    let err = s
        .add(
            &mut tx,
            Command::RotatePage {
                page: p0,
                degrees: 90,
            },
        )
        .unwrap_err();
    assert_eq!(
        err.code,
        code::NOT_FOUND,
        "p0 is deleted in this transaction"
    );
    s.commit(tx).unwrap();
    assert_eq!(s.page_ids(), [p1]);
}

/// A blank page and a page with Type3 text.
fn blank_and_type3() -> Vec<u8> {
    let t3 = "BT /T3 12 Tf 72 700 Td (aaa) Tj ET\nBT /T3 12 Tf 72 680 Td (aa) Tj ET";
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>".into(),
        stream(""),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /T3 7 0 R >> >> >>".into(),
        stream(t3),
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] \
         /CharProcs << /a 8 0 R >> /Encoding << /Type /Encoding /Differences [97 /a] >> \
         /FirstChar 97 /LastChar 97 /Widths [1000] /Resources << >> >>"
            .into(),
        stream("1000 0 0 0 1000 1000 d1 0 0 1000 1000 re f"),
    ])
}

#[test]
fn failed_apply_rolls_back_from_bytes() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), blank_and_type3());
    let [p0, p1] = [s.page_ids()[0], s.page_ids()[1]];
    commit(&mut s, vec![add_rect(p0, rect(0.0, 0.0, 50.0, 50.0)).1]).unwrap();
    let before = snapshot(&mut s);
    let type3_object = s.objects(p1).unwrap()[0].id;

    // The first command changes page 0. The second fails in the engine.
    let err = commit(
        &mut s,
        vec![
            add_rect(p0, rect(100.0, 100.0, 200.0, 200.0)).1,
            Command::RotatePage {
                page: p0,
                degrees: 90,
            },
            Command::RemoveObject {
                page: p1,
                object: type3_object,
            },
        ],
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.command_index),
        (code::UNSUPPORTED_CONTENT, Some(2)),
        "{err:?}"
    );
    assert_eq!(snapshot(&mut s), before);
    assert_eq!(s.page(p0).unwrap().info.quarter_turns, 0);
    assert_eq!(s.history().len(), 1);
}

#[test]
fn undo_and_redo_across_checkpoints() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), blank_pdf(&[(300.0, 300.0), (300.0, 300.0)]));
    let pages = s.page_ids();
    let mut at = Vec::new();
    for i in 0..45 {
        let page = pages[i % 2];
        let c = if i % 7 == 3 {
            Command::RotatePage { page, degrees: 90 }
        } else {
            let x = (i * 6) as f64;
            add_rect(page, rect(x, x, x + 5.0, x + 5.0)).1
        };
        commit(&mut s, vec![c]).unwrap();
        if i + 1 == 18 || i + 1 == 45 {
            at.push(snapshot(&mut s));
        }
    }
    let checkpoints = std::fs::read_dir(s.journal_dir())
        .unwrap()
        .filter(|f| {
            f.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("checkpoint-")
        })
        .count();
    assert_eq!(checkpoints, 2, "checkpoints after 20 and 40");

    for _ in 0..27 {
        s.undo().unwrap();
    }
    assert_eq!(snapshot(&mut s), at[0], "state after 18 transactions");
    for _ in 0..27 {
        s.redo().unwrap();
    }
    assert_eq!(snapshot(&mut s), at[1], "state after 45 transactions");
    assert_eq!(s.redo().unwrap_err().code, code::NOTHING_TO_REDO);

    // A new commit after an undo removes the redo branch.
    s.undo().unwrap();
    commit(
        &mut s,
        vec![Command::RotatePage {
            page: pages[0],
            degrees: 180,
        }],
    )
    .unwrap();
    assert!(!s.can_redo());
    assert_eq!(s.history().len(), 45);
}

#[test]
fn recovery_from_the_journal_gives_the_same_state() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), blank_pdf(&[(300.0, 300.0), (300.0, 300.0)]));
    let font = s.add_asset(fonts().select("Noto Sans", true, false).unwrap());
    let image = s.add_asset(Arc::from(png(4, 4)));
    let pages = s.page_ids();
    commit(
        &mut s,
        vec![add_text(pages[0], font, (10.0, 150.0), "Recovered text").1],
    )
    .unwrap();
    commit(
        &mut s,
        vec![Command::MovePage {
            page: pages[0],
            before: None,
        }],
    )
    .unwrap();
    commit(
        &mut s,
        vec![Command::RotatePage {
            page: pages[1],
            degrees: 270,
        }],
    )
    .unwrap();
    s.undo().unwrap();
    let image_cmd = Command::AddImage {
        id: ObjectId::random(),
        page: pages[1],
        bounds: rect(0.0, 0.0, 40.0, 40.0),
        image,
        turns: None,
    };
    commit(&mut s, vec![image_cmd]).unwrap();
    let want = snapshot(&mut s);
    let history = s.history().to_vec();
    let journal = s.journal_dir().to_path_buf();
    drop(s); // A crash: the journal stays.

    let list = t_rustable_core::journal::list_unclosed(dir.path()).unwrap();
    assert_eq!(list.len(), 1);
    let mut r = Session::recover(&e, &list[0].dir, None).unwrap();
    assert_eq!(list[0].dir, journal);
    assert_eq!(r.history(), history);
    assert_eq!(snapshot(&mut r), want);
    r.undo().unwrap();
    r.close().unwrap();
}

#[test]
fn an_engine_restart_reloads_every_session() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut a = open(&e, dir.path(), blank_pdf(&[(200.0, 200.0)]));
    let mut b = open(&e, dir.path(), blank_pdf(&[(200.0, 200.0), (100.0, 100.0)]));
    let pa = a.page_ids()[0];
    let [pb0, pb] = [b.page_ids()[0], b.page_ids()[1]];
    commit(&mut a, vec![add_rect(pa, rect(0.0, 0.0, 10.0, 10.0)).1]).unwrap();
    commit(&mut b, vec![Command::DeletePage { page: pb0 }]).unwrap();
    let (want_a, want_b) = (snapshot(&mut a), snapshot(&mut b));

    assert!(e.call(Priority::Normal, Request::TestPanic).is_err());
    assert_eq!(snapshot(&mut a), want_a);
    assert_eq!(snapshot(&mut b), want_b);
    commit(&mut b, vec![add_rect(pb, rect(0.0, 0.0, 10.0, 10.0)).1]).unwrap();
    assert_eq!(b.objects(pb).unwrap().len(), 1);
}

#[test]
fn save_writes_the_file_and_guards_signatures() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), blank_pdf(&[(300.0, 300.0)]));
    let font = s.add_asset(fonts().select("Noto Sans", false, true).unwrap());
    let page = s.page_ids()[0];
    let stroke = Stroke {
        color: RED,
        width: 2.0,
        dash: vec![4.0, 2.0],
    };
    let line = Command::AddShape {
        id: ObjectId::random(),
        page,
        shape: Shape::Line(Point { x: 0.0, y: 0.0 }, Point { x: 300.0, y: 300.0 }),
        stroke: Some(stroke),
        fill: None,
    };
    commit(
        &mut s,
        vec![add_text(page, font, (20.0, 20.0), "Saved").1, line],
    )
    .unwrap();
    let out = dir.path().join("out.pdf");
    s.save(&out, SaveOptions::default()).unwrap();
    let mut again = open(&e, dir.path(), std::fs::read(&out).unwrap());
    let p = again.page_ids()[0];
    assert!(again.page_text(p).unwrap().contains("Saved"));
    assert_eq!(again.objects(p).unwrap().len(), 2);

    let signed = build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] /SigFlags 3 >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 4 0 R /Annots [5 0 R] >>".into(),
        stream(""),
        "<< /Type /Annot /Subtype /Widget /FT /Sig /T (sig) /V 6 0 R /Rect [0 0 0 0] /P 3 0 R >>".into(),
        "<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange [0 0 0 0] /Contents <00> >>".into(),
    ]);
    let mut s = open(&e, dir.path(), signed);
    assert_eq!(s.signature_count(), 1);
    let target = dir.path().join("signed.pdf");
    let err = s.save(&target, SaveOptions::default()).unwrap_err();
    assert_eq!(err.code, code::SIGNED_DOCUMENT);
    assert!(!target.exists());
    s.save(
        &target,
        SaveOptions {
            allow_invalidate_signatures: true,
        },
    )
    .unwrap();
    assert!(target.exists());
    // A form field widget cannot be removed.
    let page = s.page_ids()[0];
    let widget = s.annotations(page).unwrap()[0].id;
    let fields = s.form_fields().unwrap();
    assert_eq!(fields[0].id, widget);
    let err = commit(
        &mut s,
        vec![Command::RemoveObject {
            page,
            object: widget,
        }],
    )
    .unwrap_err();
    assert_eq!(err.code, code::INVALID_ARGUMENT);
}

/// Box of the dark pixels of a render: width and height in pixels.
fn ink_size(b: &t_rustable_engine::Bitmap) -> (u32, u32) {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..b.height {
        for x in 0..b.width {
            let i = (y * b.stride + x * 4) as usize;
            if b.bgra[i] < 128 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    assert!(x1 >= x0, "no dark pixels");
    (x1 - x0 + 1, y1 - y0 + 1)
}

#[test]
fn new_text_reads_upright_on_a_rotated_page() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), blank_pdf(&[(300.0, 300.0)]));
    let font = s.add_asset(fonts().select("Noto Sans", false, false).unwrap());
    let page = s.page_ids()[0];
    commit(&mut s, vec![Command::RotatePage { page, degrees: 90 }]).unwrap();
    let (_, text) = add_text(page, font, (150.0, 50.0), "Upright text line");
    commit(&mut s, vec![text]).unwrap();

    // The journal stores the resolved turns, not None.
    let Command::AddText { turns, .. } = &s.history()[1].commands[0] else {
        panic!("not AddText");
    };
    assert_eq!(*turns, Some(1));
    // The render shows the page as the reader sees it: the text runs left to right.
    let b = s.render(page, 1.0, None, Priority::Normal).unwrap();
    let (w, h) = ink_size(&b);
    assert!(w > 3 * h, "ink is {w} by {h} pixels");
    s.close().unwrap();
}

#[test]
fn text_width_matches_the_engine_layout() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), blank_pdf(&[(400.0, 300.0)]));
    let face = fonts().select("Noto Sans", false, false).unwrap();
    let font = s.add_asset(face.clone());
    let page = s.page_ids()[0];
    let prefix = "Wavy text, ";
    let (_, alone) = add_text(page, font, (20.0, 200.0), "B");
    let (_, after) = add_text(page, font, (20.0, 100.0), &format!("{prefix}B"));
    commit(&mut s, vec![alone, after]).unwrap();
    let runs = s.text_runs(page).unwrap();
    let left_of_b = |y: f64| {
        let run = runs
            .iter()
            .find(|(_, r)| (r.bounds.bottom - y).abs() < 10.0)
            .unwrap();
        run.1
            .chars
            .iter()
            .rev()
            .find(|c| c.c == 'B')
            .unwrap()
            .bounds
            .left
    };
    let moved = left_of_b(100.0) - left_of_b(200.0);
    let width = t_rustable_core::fonts::text_width(&face, prefix, 14.0).unwrap();
    assert!(
        (moved - width).abs() < 0.05,
        "engine {moved}, measure {width}"
    );
    s.close().unwrap();
}

#[test]
fn outline_maps_pages_to_ids() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&e, dir.path(), outline_pdf());
    let [p0, p1, p2] = [s.page_ids()[0], s.page_ids()[1], s.page_ids()[2]];
    let pages = |s: &mut Session| {
        let o = s.outline().unwrap();
        let c = &o[0].children;
        [o[0].page, c[0].page, c[1].page, o[1].page]
    };
    assert_eq!(pages(&mut s), [Some(p0), Some(p1), None, Some(p2)]);
    assert_eq!(s.outline().unwrap()[1].title, "Café");

    // The bookmarks follow the pages when the order changes.
    commit(
        &mut s,
        vec![Command::MovePage {
            page: p1,
            before: Some(p0),
        }],
    )
    .unwrap();
    assert_eq!(s.page_ids(), [p1, p0, p2]);
    assert_eq!(pages(&mut s), [Some(p0), Some(p1), None, Some(p2)]);
    s.close().unwrap();

    // A destination to a page number that does not exist gives no page.
    let mut objects = blank_objects(&[(100.0, 100.0)], "/Outlines 5 0 R ");
    objects.extend([
        "<< /Type /Outlines /First 6 0 R >>".into(),
        "<< /Title (Far) /Parent 5 0 R /Dest [99 /Fit] >>".into(),
    ]);
    let mut s = open(&e, dir.path(), build_pdf(&objects));
    assert_eq!(s.outline().unwrap()[0].page, None);
    s.close().unwrap();
}
