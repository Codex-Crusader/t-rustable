mod common;

use common::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use t_rustable_engine::*;

fn query(e: &Engine, doc: DocKey, q: Query) -> Response {
    call(e, Request::Query { doc, query: q }).expect("query")
}

fn objects(e: &Engine, doc: DocKey, page: usize) -> Vec<ObjectInfo> {
    match query(e, doc, Query::Objects { page }) {
        Response::Objects(o) => o,
        r => panic!("{r:?}"),
    }
}

fn page_text(e: &Engine, doc: DocKey, page: usize) -> String {
    match query(e, doc, Query::PageText { page }) {
        Response::PageText(t) => t,
        r => panic!("{r:?}"),
    }
}

fn page_info(e: &Engine, doc: DocKey, page: usize) -> PageInfo {
    match query(e, doc, Query::PageInfo { page }) {
        Response::PageInfo(i) => i,
        r => panic!("{r:?}"),
    }
}

fn apply(e: &Engine, doc: DocKey, ops: Vec<EditOp>) -> Result<Vec<Applied>, EngineError> {
    match call(e, Request::Apply { doc, ops })? {
        Response::Applied(a) => Ok(a),
        r => panic!("{r:?}"),
    }
}

fn save(e: &Engine, doc: DocKey) -> Vec<u8> {
    match call(e, Request::Save { doc }).expect("save") {
        Response::Saved(b) => b,
        r => panic!("{r:?}"),
    }
}

fn render(e: &Engine, doc: DocKey, page: usize, scale: f64) -> Bitmap {
    match call(
        e,
        Request::Render {
            doc,
            page,
            scale,
            clip: None,
        },
    )
    .expect("render")
    {
        Response::Rendered(b) => b,
        r => panic!("{r:?}"),
    }
}

fn text_op(page: usize, x: f64, y: f64, text: &str) -> EditOp {
    EditOp::AddText {
        page,
        origin: Point { x, y },
        text: text.into(),
        font: font(),
        size: 24.0,
        color: Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        quarter_turns: 0,
    }
}

fn rect_op(page: usize, r: Rect, fill: Rgba) -> EditOp {
    EditOp::AddPath {
        page,
        segments: vec![
            PathSegment::MoveTo(Point {
                x: r.left,
                y: r.bottom,
            }),
            PathSegment::LineTo(Point {
                x: r.right,
                y: r.bottom,
            }),
            PathSegment::LineTo(Point {
                x: r.right,
                y: r.top,
            }),
            PathSegment::LineTo(Point {
                x: r.left,
                y: r.top,
            }),
            PathSegment::Close,
        ],
        stroke: None,
        fill: Some(fill),
    }
}

const RED: Rgba = Rgba {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

#[test]
fn open_summary_and_page_info() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(612.0, 792.0), (300.0, 400.0)]));
    match query(&e, 1, Query::Summary) {
        Response::Summary(s) => {
            assert_eq!(s.page_count, 2);
            assert_eq!(s.signature_count, 0);
        }
        r => panic!("{r:?}"),
    }
    let info = page_info(&e, 1, 1);
    assert_eq!(
        info.media_box,
        Rect {
            left: 0.0,
            bottom: 0.0,
            right: 300.0,
            top: 400.0
        }
    );
    assert_eq!(info.crop_box, info.media_box);
    assert_eq!(info.quarter_turns, 0);
}

#[test]
fn one_engine_per_process() {
    let _e = engine();
    let path = dev_pdfium_path().unwrap();
    let second = Engine::start(&path);
    assert!(
        matches!(second, Err(EngineError::Internal(_))),
        "a second engine started"
    );
}

#[test]
fn bad_bytes_and_unknown_document() {
    let e = engine();
    let r = call(
        &e,
        Request::Open {
            doc: 1,
            bytes: Arc::from(&b"not a pdf"[..]),
            password: None,
        },
    );
    assert!(matches!(r, Err(EngineError::BadFile(_))), "{r:?}");
    let r = call(
        &e,
        Request::Query {
            doc: 99,
            query: Query::Summary,
        },
    );
    assert_eq!(r, Err(EngineError::UnknownDocument));
}

#[test]
fn page_ops_change_the_page_list() {
    let e = engine();
    open(
        &e,
        1,
        blank_pdf(&[(100.0, 100.0), (200.0, 200.0), (300.0, 300.0)]),
    );
    let width = |e: &Engine, p| page_info(e, 1, p).media_box.right;
    apply(&e, 1, vec![EditOp::MovePage { page: 2, to: 0 }]).unwrap();
    assert_eq!(
        [width(&e, 0), width(&e, 1), width(&e, 2)],
        [300.0, 100.0, 200.0]
    );
    apply(&e, 1, vec![EditOp::DeletePage { page: 1 }]).unwrap();
    apply(
        &e,
        1,
        vec![EditOp::InsertBlankPage {
            at: 2,
            width: 50.0,
            height: 60.0,
        }],
    )
    .unwrap();
    assert_eq!(
        [width(&e, 0), width(&e, 1), width(&e, 2)],
        [300.0, 200.0, 50.0]
    );
    apply(
        &e,
        1,
        vec![EditOp::SetRotation {
            page: 0,
            quarter_turns: 1,
        }],
    )
    .unwrap();
    assert_eq!(page_info(&e, 1, 0).quarter_turns, 1);

    // The changes survive save and reload.
    let bytes = save(&e, 1);
    open(&e, 2, bytes);
    assert_eq!(page_info(&e, 2, 0).quarter_turns, 1);
    assert_eq!(page_info(&e, 2, 2).media_box.top, 60.0);

    let r = apply(&e, 1, vec![EditOp::DeletePage { page: 9 }]);
    assert!(
        matches!(r, Err(EngineError::OpFailed { index: 0, .. })),
        "{r:?}"
    );
}

#[test]
fn add_text_extracts_and_survives_save() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(612.0, 792.0)]));
    let applied = apply(
        &e,
        1,
        vec![text_op(0, 72.0, 700.0, "Hello World Grüße ж €")],
    )
    .unwrap();
    assert_eq!(applied, vec![Applied::Object { index: 0 }]);
    assert!(page_text(&e, 1, 0).contains("Hello World Grüße ж €"));

    match query(&e, 1, Query::TextRuns { page: 0 }) {
        Response::TextRuns(runs) => {
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].object, Some(0));
            assert!(
                runs[0].font_name.contains("NotoSans"),
                "{}",
                runs[0].font_name
            );
            assert!((runs[0].font_size - 24.0).abs() < 0.01);
            assert_eq!(runs[0].chars.len(), runs[0].text.chars().count());
            assert!(runs[0].bounds.left >= 71.0 && runs[0].bounds.bottom >= 690.0);
        }
        r => panic!("{r:?}"),
    }

    let bytes = save(&e, 1);
    open(&e, 2, bytes);
    assert!(page_text(&e, 2, 0).contains("Hello World Grüße ж €"));
}

#[test]
fn quarter_turns_turn_text_and_images() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(400.0, 400.0)]));
    let mut text = text_op(0, 200.0, 100.0, "Turned text");
    if let EditOp::AddText { quarter_turns, .. } = &mut text {
        *quarter_turns = 1;
    }
    let image = |turns: u8| EditOp::AddImage {
        page: 0,
        bounds: Rect {
            left: 10.0,
            bottom: 300.0,
            right: 90.0,
            top: 340.0,
        },
        data: ImageData::Png(png(20, 10).into()),
        quarter_turns: turns,
    };
    apply(&e, 1, vec![text, image(1)]).unwrap();
    let found = objects(&e, 1, 0);
    // The text runs up from its origin, so its box is taller than wide, left of x = 200.
    let t = found[0].bounds;
    assert!(t.height() > 3.0 * t.width(), "{t:?}");
    assert!(t.right <= 200.5 && t.bottom >= 99.5, "{t:?}");
    // The turned image fills the same bounds.
    let b = found[1].bounds;
    assert_eq!(
        [b.left, b.bottom, b.right, b.top],
        [10.0, 300.0, 90.0, 340.0]
    );
    assert_eq!(page_text(&e, 1, 0).trim(), "Turned text");
    let bad = apply(&e, 1, vec![image(4)]).unwrap_err();
    assert!(format!("{bad:?}").contains("InvalidArgument"), "{bad:?}");
}

#[test]
fn shapes_images_and_remove() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(200.0, 200.0)]));
    let ops = vec![
        rect_op(
            0,
            Rect {
                left: 0.0,
                bottom: 0.0,
                right: 100.0,
                top: 100.0,
            },
            RED,
        ),
        EditOp::AddImage {
            page: 0,
            bounds: Rect {
                left: 100.0,
                bottom: 100.0,
                right: 200.0,
                top: 200.0,
            },
            data: ImageData::Png(png(20, 20).into()),
            quarter_turns: 0,
        },
        text_op(0, 10.0, 150.0, "Hi"),
    ];
    let applied = apply(&e, 1, ops).unwrap();
    assert_eq!(
        applied,
        (0..3)
            .map(|index| Applied::Object { index })
            .collect::<Vec<_>>()
    );
    let kinds: Vec<ObjectKind> = objects(&e, 1, 0).iter().map(|o| o.kind).collect();
    assert_eq!(
        kinds,
        [ObjectKind::Path, ObjectKind::Image, ObjectKind::Text]
    );

    // Render at 1 px per point. The bitmap y axis goes down.
    let b = render(&e, 1, 0, 1.0);
    assert_eq!((b.width, b.height), (200, 200));
    assert_eq!(pixel(&b, 50, 150), [0, 0, 255, 255], "red rect");
    assert_eq!(
        pixel(&b, 125, 50),
        [255, 255, 255, 255],
        "transparent half of the image"
    );
    assert_eq!(
        pixel(&b, 175, 50),
        [0, 0, 255, 255],
        "opaque half of the image"
    );

    apply(&e, 1, vec![EditOp::RemoveObject { page: 0, index: 1 }]).unwrap();
    let kinds: Vec<ObjectKind> = objects(&e, 1, 0).iter().map(|o| o.kind).collect();
    assert_eq!(kinds, [ObjectKind::Path, ObjectKind::Text]);
    assert_eq!(pixel(&render(&e, 1, 0, 1.0), 175, 50), [255, 255, 255, 255]);
}

#[test]
fn one_apply_mixes_content_and_page_ops() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(200.0, 200.0), (200.0, 200.0)]));
    let square = Rect {
        left: 0.0,
        bottom: 0.0,
        right: 10.0,
        top: 10.0,
    };
    let applied = apply(
        &e,
        1,
        vec![
            text_op(0, 10.0, 100.0, "first"),
            rect_op(0, square, RED),
            EditOp::RemoveObject { page: 0, index: 0 },
            EditOp::SetRotation {
                page: 1,
                quarter_turns: 2,
            },
            text_op(1, 10.0, 100.0, "second"),
            text_op(0, 10.0, 50.0, "third"),
        ],
    )
    .unwrap();
    use Applied::*;
    assert_eq!(
        applied,
        [
            Object { index: 0 },
            Object { index: 1 },
            Done,
            Done,
            Object { index: 0 },
            Object { index: 1 }
        ]
    );
    let kinds: Vec<ObjectKind> = objects(&e, 1, 0).iter().map(|o| o.kind).collect();
    assert_eq!(kinds, [ObjectKind::Path, ObjectKind::Text]);
    assert!(page_text(&e, 1, 0).contains("third") && !page_text(&e, 1, 0).contains("first"));
    assert!(page_text(&e, 1, 1).contains("second"));

    // An error names the index of the failed op in the whole list.
    let r = apply(
        &e,
        1,
        vec![
            rect_op(0, square, RED),
            EditOp::RemoveObject { page: 0, index: 99 },
        ],
    );
    assert!(
        matches!(r, Err(EngineError::OpFailed { index: 1, .. })),
        "{r:?}"
    );
}

#[test]
fn many_adds_in_one_apply() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(612.0, 792.0)]));
    let ops: Vec<EditOp> = (0..2000)
        .map(|i| {
            let (x, y) = ((i % 100) as f64 * 6.0, (i / 100) as f64 * 3.0);
            rect_op(
                0,
                Rect {
                    left: x,
                    bottom: y,
                    right: x + 5.0,
                    top: y + 2.0,
                },
                RED,
            )
        })
        .collect();
    let started = Instant::now();
    assert_eq!(apply(&e, 1, ops).unwrap().len(), 2000);
    assert_eq!(objects(&e, 1, 0).len(), 2000);
    assert!(
        started.elapsed().as_secs() < 20,
        "2000 adds took {:?}",
        started.elapsed()
    );
}

#[test]
fn bad_image_is_refused() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(200.0, 200.0)]));
    let r = apply(
        &e,
        1,
        vec![EditOp::AddImage {
            page: 0,
            bounds: Rect {
                left: 0.0,
                bottom: 0.0,
                right: 10.0,
                top: 10.0,
            },
            data: ImageData::Png(Arc::from(&b"not a png"[..])),
            quarter_turns: 0,
        }],
    );
    match r {
        Err(EngineError::OpFailed { index: 0, error }) => {
            assert!(
                matches!(*error, EngineError::UnsupportedImage(_)),
                "{error:?}"
            )
        }
        r => panic!("{r:?}"),
    }
    assert!(objects(&e, 1, 0).is_empty());
}

#[test]
fn rewrite_check_refuses_type3_loss() {
    let e = engine();
    open(&e, 1, type3_pdf());
    let before = objects(&e, 1, 0);
    assert_eq!(before.len(), 3);
    let r = apply(&e, 1, vec![EditOp::RemoveObject { page: 0, index: 1 }]);
    match r {
        Err(EngineError::OpFailed { index: 0, error }) => {
            assert!(
                matches!(*error, EngineError::UnsupportedContent(_)),
                "{error:?}"
            )
        }
        r => panic!("PDFium wrote the Type3 page without loss. Update ADR 0015. Result: {r:?}"),
    }
    // Adding to the same page is safe, because PDFium adds a new content stream.
    open(&e, 2, type3_pdf());
    apply(
        &e,
        2,
        vec![rect_op(
            0,
            Rect {
                left: 0.0,
                bottom: 0.0,
                right: 10.0,
                top: 10.0,
            },
            RED,
        )],
    )
    .unwrap();
    assert_eq!(objects(&e, 2, 0).len(), 4);
}

#[test]
fn annotations_and_form_fields() {
    let e = engine();
    open(&e, 1, annotated_pdf());
    let annots = match query(&e, 1, Query::Annotations { page: 0 }) {
        Response::Annotations(a) => a,
        r => panic!("{r:?}"),
    };
    assert_eq!(annots.len(), 2);
    assert_eq!(annots[0].subtype, "Widget");
    assert_eq!(annots[1].subtype, "Square");
    assert_eq!(annots[1].contents.as_deref(), Some("A note"));
    assert_eq!(
        annots[1].bounds,
        Rect {
            left: 100.0,
            bottom: 100.0,
            right: 200.0,
            top: 150.0
        }
    );

    match query(&e, 1, Query::FormFields) {
        Response::FormFields(f) => {
            assert_eq!(f.len(), 1);
            assert_eq!((f[0].page, f[0].annotation), (0, 0));
            assert_eq!(f[0].name, "name");
            assert_eq!(f[0].kind, FieldKind::Text);
            assert_eq!(f[0].value.as_deref(), Some("Ada"));
            assert!(f[0].read_only);
        }
        r => panic!("{r:?}"),
    }

    apply(&e, 1, vec![EditOp::RemoveAnnotation { page: 0, index: 1 }]).unwrap();
    match query(&e, 1, Query::Annotations { page: 0 }) {
        Response::Annotations(a) => assert_eq!(a.len(), 1),
        r => panic!("{r:?}"),
    }
}

#[test]
fn search_finds_text_with_boxes() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(612.0, 792.0), (612.0, 792.0)]));
    apply(
        &e,
        1,
        vec![
            text_op(0, 72.0, 700.0, "alpha beta"),
            text_op(1, 72.0, 700.0, "beta gamma beta"),
        ],
    )
    .unwrap();
    let options = SearchOptions {
        match_case: false,
        whole_word: true,
        max_hits: 10,
    };
    match query(
        &e,
        1,
        Query::Search {
            query: "BETA".into(),
            options: options.clone(),
        },
    ) {
        Response::SearchHits(h) => {
            assert_eq!(h.iter().map(|h| h.page).collect::<Vec<_>>(), [0, 1, 1]);
            assert!(
                h.iter()
                    .all(|h| h.text.eq_ignore_ascii_case("beta") && !h.boxes.is_empty())
            );
        }
        r => panic!("{r:?}"),
    }
    let limited = SearchOptions {
        max_hits: 2,
        ..options
    };
    match query(
        &e,
        1,
        Query::Search {
            query: "beta".into(),
            options: limited,
        },
    ) {
        Response::SearchHits(h) => assert_eq!(h.len(), 2),
        r => panic!("{r:?}"),
    }
}

#[test]
fn render_rotation_and_clip() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(200.0, 100.0)]));
    apply(
        &e,
        1,
        vec![rect_op(
            0,
            Rect {
                left: 0.0,
                bottom: 0.0,
                right: 20.0,
                top: 20.0,
            },
            RED,
        )],
    )
    .unwrap();
    let b = render(&e, 1, 0, 2.0);
    assert_eq!((b.width, b.height), (400, 200));
    assert_eq!(pixel(&b, 10, 190), [0, 0, 255, 255], "bottom left is red");

    apply(
        &e,
        1,
        vec![EditOp::SetRotation {
            page: 0,
            quarter_turns: 1,
        }],
    )
    .unwrap();
    let b = render(&e, 1, 0, 1.0);
    assert_eq!(
        (b.width, b.height),
        (100, 200),
        "a quarter turn swaps width and height"
    );
    assert_eq!(
        pixel(&b, 5, 5),
        [0, 0, 255, 255],
        "after a clockwise turn, the red corner is top left"
    );

    let clip = Clip {
        x: 0,
        y: 0,
        width: 10,
        height: 10,
    };
    match call(
        &e,
        Request::Render {
            doc: 1,
            page: 0,
            scale: 1.0,
            clip: Some(clip),
        },
    )
    .unwrap()
    {
        Response::Rendered(b) => {
            assert_eq!((b.width, b.height), (10, 10));
            assert_eq!(pixel(&b, 5, 5), [0, 0, 255, 255]);
        }
        r => panic!("{r:?}"),
    }
}

#[test]
fn cancel_before_start_and_during_render() {
    let e = engine();
    open(&e, 1, heavy_pdf(3000));

    // Cancelled while in the queue: the request does not start.
    let blocker = e.submit(
        Priority::Normal,
        Request::Render {
            doc: 1,
            page: 0,
            scale: 4.0,
            clip: None,
        },
    );
    let queued = e.submit(
        Priority::Normal,
        Request::Query {
            doc: 1,
            query: Query::Summary,
        },
    );
    queued.cancel();
    assert_eq!(queued.wait(), Err(EngineError::Cancelled));
    assert!(blocker.wait().is_ok());

    // Cancelled during a render: the render stops at its next pause. The render must be
    // long compared with sleep and thread wakeup times, so the page grows until it is.
    let mut doc = 1;
    let mut rects = 3000;
    let mut full = Duration::ZERO;
    while full < Duration::from_millis(200) && rects <= 200_000 {
        doc += 1;
        rects *= 3;
        open(&e, doc, heavy_pdf(rects));
        let started = Instant::now();
        render(&e, doc, 0, 6.0);
        full = started.elapsed();
    }
    let t = e.submit(
        Priority::Normal,
        Request::Render {
            doc,
            page: 0,
            scale: 6.0,
            clip: None,
        },
    );
    std::thread::sleep(full / 4);
    let started = Instant::now();
    t.cancel();
    let r = t.wait();
    assert_eq!(
        r,
        Err(EngineError::Cancelled),
        "full render took {full:?} with {rects} rectangles"
    );
    assert!(
        started.elapsed() < full,
        "cancel took {:?}, full render {full:?}",
        started.elapsed()
    );
}

#[test]
fn higher_priority_runs_first() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(612.0, 792.0)]));
    // The sleep holds the engine while the three requests go into the queue.
    let hold = e.submit(Priority::Normal, Request::TestSleep { millis: 300 });
    let insert = |width| Request::Apply {
        doc: 1,
        ops: vec![EditOp::InsertBlankPage {
            at: 0,
            width,
            height: 10.0,
        }],
    };
    let tickets = [
        e.submit(Priority::Background, insert(1.0)),
        e.submit(Priority::Normal, insert(2.0)),
        e.submit(Priority::Interactive, insert(3.0)),
    ];
    hold.wait().unwrap();
    for t in tickets {
        t.wait().unwrap();
    }
    // Each insert goes to index 0, so the last one to run is first.
    let widths: Vec<f64> = (0..3)
        .map(|p| page_info(&e, 1, p).media_box.right)
        .collect();
    assert_eq!(
        widths,
        [1.0, 2.0, 3.0],
        "run order was interactive, normal, background"
    );
}

#[test]
fn restart_after_panic() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(612.0, 792.0)]));
    assert_eq!(call(&e, Request::TestPanic), Err(EngineError::Restarted));
    assert_eq!(e.restarts(), 1);
    assert_eq!(
        call(
            &e,
            Request::Query {
                doc: 1,
                query: Query::Summary
            }
        ),
        Err(EngineError::UnknownDocument)
    );
    open(&e, 1, blank_pdf(&[(612.0, 792.0)]));
    apply(&e, 1, vec![text_op(0, 72.0, 700.0, "after restart")]).unwrap();
    assert!(page_text(&e, 1, 0).contains("after restart"));
}

#[test]
fn qpdf_post_process_keeps_render_and_text() {
    let e = engine();
    open(&e, 1, blank_pdf(&[(200.0, 200.0), (200.0, 200.0)]));
    apply(
        &e,
        1,
        vec![
            text_op(0, 10.0, 100.0, "qpdf check"),
            rect_op(
                1,
                Rect {
                    left: 0.0,
                    bottom: 0.0,
                    right: 50.0,
                    top: 50.0,
                },
                RED,
            ),
        ],
    )
    .unwrap();
    let pdfium_bytes = save(&e, 1);
    let qpdf_bytes = post::post_process(&pdfium_bytes).expect("post-process");
    open(&e, 2, pdfium_bytes);
    open(&e, 3, qpdf_bytes);
    for page in 0..2 {
        assert_eq!(page_text(&e, 2, page), page_text(&e, 3, page));
        assert_eq!(render(&e, 2, page, 1.0), render(&e, 3, page, 1.0));
    }
}

fn outline(e: &Engine, doc: DocKey) -> Vec<OutlineItem> {
    match query(e, doc, Query::Outline) {
        Response::Outline(o) => o,
        r => panic!("{r:?}"),
    }
}

fn item(title: &str, page: Option<usize>, children: Vec<OutlineItem>) -> OutlineItem {
    OutlineItem {
        title: title.into(),
        page,
        children,
    }
}

#[test]
fn outline_gives_the_bookmark_tree() {
    let e = engine();
    open(&e, 1, outline_pdf());
    assert_eq!(
        outline(&e, 1),
        [
            item(
                "Chapter 1",
                Some(0),
                vec![
                    item("Section 1.1", Some(1), vec![]),
                    item("No destination", None, vec![]),
                ]
            ),
            item("Café", Some(2), vec![]),
        ]
    );
    open(&e, 2, blank_pdf(&[(100.0, 100.0)]));
    assert_eq!(outline(&e, 2), []);
}

#[test]
fn outline_stops_at_cycles_and_limits() {
    let e = engine();
    // "B" is a child of "A", and its /First goes back to "A". The /Next of "C" goes back
    // to "B".
    let mut objects = blank_objects(&[(100.0, 100.0)], "/Outlines 5 0 R ");
    objects.extend([
        "<< /Type /Outlines /First 6 0 R /Last 6 0 R >>".into(),
        "<< /Title (A) /Parent 5 0 R /First 7 0 R /Last 8 0 R /Dest [3 0 R /Fit] >>".into(),
        "<< /Title (B) /Parent 6 0 R /First 6 0 R /Last 6 0 R /Next 8 0 R >>".into(),
        "<< /Title (C) /Parent 6 0 R /Prev 7 0 R /Next 7 0 R >>".into(),
    ]);
    open(&e, 1, build_pdf(&objects));
    assert_eq!(
        outline(&e, 1),
        [item(
            "A",
            Some(0),
            vec![item("B", None, vec![]), item("C", None, vec![])]
        )]
    );

    // 100 levels: the outline stops at 64.
    let mut objects = blank_objects(&[(100.0, 100.0)], "/Outlines 5 0 R ");
    objects.push("<< /Type /Outlines /First 6 0 R >>".into());
    for i in 0..100 {
        let first = if i < 99 {
            format!("/First {} 0 R", 7 + i)
        } else {
            String::new()
        };
        objects.push(format!("<< /Title (L{i}) /Parent {} 0 R {first} >>", 5 + i));
    }
    open(&e, 2, build_pdf(&objects));
    let mut depth = 0;
    let mut level = outline(&e, 2);
    while let Some(first) = level.into_iter().next() {
        depth += 1;
        level = first.children;
    }
    assert_eq!(depth, 64);

    // 10,050 items on one level: the outline stops at 10,000.
    let mut objects = blank_objects(&[(100.0, 100.0)], "/Outlines 5 0 R ");
    objects.push("<< /Type /Outlines /First 6 0 R >>".into());
    for i in 0..10_050 {
        let next = if i < 10_049 {
            format!("/Next {} 0 R", 7 + i)
        } else {
            String::new()
        };
        objects.push(format!("<< /Title (I{i}) /Parent 5 0 R {next} >>"));
    }
    open(&e, 3, build_pdf(&objects));
    assert_eq!(outline(&e, 3).len(), 10_000);
}

fn summary(e: &Engine, doc: DocKey) -> DocSummary {
    match query(e, doc, Query::Summary) {
        Response::Summary(s) => s,
        r => panic!("{r:?}"),
    }
}

fn import(source: &[u8], pages: Vec<usize>, at: usize) -> EditOp {
    EditOp::ImportPages {
        source: Arc::from(source),
        pages,
        at,
    }
}

#[test]
fn import_pages_copies_pages_that_outlive_the_source() {
    let e = engine();
    open(&e, 1, text_pdf(&["Base A", "Base B"]));
    let source = text_pdf(&["Src 1", "Src 2", "Src 3"]);
    apply(&e, 1, vec![import(&source, vec![2, 0], 1)]).unwrap();
    apply(&e, 1, vec![import(&source, vec![1], 0)]).unwrap();
    apply(&e, 1, vec![import(&source, vec![1], 5)]).unwrap();
    drop(source);
    let texts: Vec<String> = (0..6).map(|p| page_text(&e, 1, p)).collect();
    assert_eq!(
        texts,
        ["Src 2", "Base A", "Src 3", "Src 1", "Base B", "Src 2"]
    );
    // The copied pages render and survive save and reload.
    let before = render(&e, 1, 2, 1.0);
    let bytes = post::post_process(&save(&e, 1)).unwrap();
    open(&e, 2, bytes);
    assert_eq!(summary(&e, 2).page_count, 6);
    assert_eq!(page_text(&e, 2, 3), "Src 1");
    assert_eq!(render(&e, 2, 2, 1.0), before);
}

#[test]
fn import_pages_refuses_bad_sources_and_indices() {
    let e = engine();
    open(&e, 1, text_pdf(&["Base"]));
    let source = text_pdf(&["Src"]);
    let failed = |r: Result<Vec<Applied>, EngineError>| match r {
        Err(EngineError::OpFailed { error, .. }) => *error,
        r => panic!("{r:?}"),
    };
    assert!(matches!(
        failed(apply(&e, 1, vec![import(&source, vec![1], 0)])),
        EngineError::InvalidArgument(_)
    ));
    assert!(matches!(
        failed(apply(&e, 1, vec![import(&source, vec![0], 2)])),
        EngineError::InvalidArgument(_)
    ));
    assert!(matches!(
        failed(apply(&e, 1, vec![import(b"not a pdf", vec![0], 0)])),
        EngineError::BadFile(_)
    ));
    // Owner password only: the file opens, but a copy would drop the restrictions.
    for user in ["", "secret"] {
        let locked = encrypt(&source, user);
        match failed(apply(&e, 1, vec![import(&locked, vec![0], 0)])) {
            EngineError::InvalidArgument(m) => assert!(m.contains("encrypted"), "{m}"),
            r => panic!("{r:?}"),
        }
    }
    assert_eq!(summary(&e, 1).page_count, 1);
    assert!(!summary(&e, 1).encrypted);
    open(&e, 2, encrypt(&source, ""));
    assert!(summary(&e, 2).encrypted);
}
