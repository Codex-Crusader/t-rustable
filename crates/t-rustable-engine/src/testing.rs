//! Helpers for tests of the engine and of the crates above it: one engine at a time, the
//! bundled font, and small PDFs built in code. Only with the `test-hooks` feature.

use crate::{DocKey, Engine, EngineError, Priority, Request, Response};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

/// PDFium allows one engine per process, so tests take turns.
static ENGINE_LOCK: Mutex<()> = Mutex::new(());

/// An engine for one test. Fields drop in order: the engine stops before the lock opens.
pub struct TestEngine {
    engine: Engine,
    _turn: MutexGuard<'static, ()>,
}

impl Deref for TestEngine {
    type Target = Engine;
    fn deref(&self) -> &Engine {
        &self.engine
    }
}

pub fn engine() -> TestEngine {
    let turn = ENGINE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = crate::dev_pdfium_path().unwrap_or_else(|e| panic!("{e}"));
    TestEngine {
        engine: Engine::start(&path).expect("start the engine"),
        _turn: turn,
    }
}

pub fn font() -> Arc<[u8]> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fonts/NotoSans-Regular.ttf");
    std::fs::read(path)
        .expect("read fonts/NotoSans-Regular.ttf")
        .into()
}

pub fn call(e: &Engine, req: Request) -> Result<Response, EngineError> {
    e.call(Priority::Normal, req)
}

pub fn open(e: &Engine, doc: DocKey, bytes: Vec<u8>) {
    let r = call(
        e,
        Request::Open {
            doc,
            bytes: bytes.into(),
            password: None,
        },
    );
    assert!(matches!(r, Ok(Response::Opened(_))), "open failed: {r:?}");
}

/// Builds a PDF from object bodies. Object `n` is `objects[n - 1]`. Object 1 is the catalog.
pub fn build_pdf(objects: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

pub fn stream(content: &str) -> String {
    format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )
}

/// Blank pages with the given sizes. Content streams are empty.
pub fn blank_pdf(sizes: &[(f64, f64)]) -> Vec<u8> {
    let n = sizes.len();
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + 2 * i)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")),
    ];
    for (i, (w, h)) in sizes.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Contents {} 0 R >>",
            4 + 2 * i
        ));
        objects.push(stream(""));
    }
    build_pdf(&objects)
}

/// One page with `rects` small filled rectangles, each its own path object. Slow to render
/// at a high scale.
pub fn heavy_pdf(rects: usize) -> Vec<u8> {
    let content: String = (0..rects)
        .map(|i| {
            format!(
                "1 0 0 rg {} {} 5 2 re f
",
                (i % 100) * 6,
                (i / 100) * 3 % 780
            )
        })
        .collect();
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>".into(),
        stream(&content),
    ])
}

/// One page with three text objects in a Type3 font. PDFium cannot write such text again.
pub fn type3_pdf() -> Vec<u8> {
    let content = "BT /T3 12 Tf 72 700 Td (aaa) Tj ET\nBT /T3 12 Tf 72 680 Td (aa) Tj ET\nBT /T3 12 Tf 72 660 Td (a) Tj ET";
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /T3 5 0 R >> >> >>".into(),
        stream(content),
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1000 1000] /FontMatrix [0.001 0 0 0.001 0 0] \
         /CharProcs << /a 6 0 R >> /Encoding << /Type /Encoding /Differences [97 /a] >> \
         /FirstChar 97 /LastChar 97 /Widths [1000] /Resources << >> >>"
            .into(),
        stream("1000 0 0 0 1000 1000 d1 0 0 1000 1000 re f"),
    ])
}

/// One page with a Square annotation and a read-only text field.
pub fn annotated_pdf() -> Vec<u8> {
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Annots [5 0 R 6 0 R] >>".into(),
        stream(""),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Ada) /Ff 1 /Rect [100 600 300 630] /P 3 0 R >>".into(),
        "<< /Type /Annot /Subtype /Square /Rect [100 100 200 150] /Contents (A note) /P 3 0 R >>".into(),
    ])
}

/// A PNG of `w` x `h` pixels: left half transparent, right half opaque red.
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let mut data = Vec::new();
    for _ in 0..h {
        for x in 0..w {
            data.extend_from_slice(if x < w / 2 {
                &[0, 0, 0, 0]
            } else {
                &[255, 0, 0, 255]
            });
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&data).unwrap();
    }
    out
}

/// BGRA of one pixel.
pub fn pixel(b: &crate::Bitmap, x: u32, y: u32) -> [u8; 4] {
    let i = (y * b.stride + x * 4) as usize;
    [b.bgra[i], b.bgra[i + 1], b.bgra[i + 2], b.bgra[i + 3]]
}
