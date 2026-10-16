//! Spike 4: check the PDFium edit APIs.
//!
//! Modes:
//!   edit [third-party.pdf]   text add/remove, fonts, glyph coverage, character boxes
//!   progressive <file.pdf> [page]   progressive render with a pause callback, and cancel
//!   heavy                    writes out/heavy.pdf with many objects per page
//!   reinit <file.pdf>        destroy and init PDFium again in one process

mod raw;

use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn spikes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn pdfium_lib_dir() -> PathBuf {
    let platform = if cfg!(windows) {
        "win-x64/bin"
    } else {
        "linux-x64/lib"
    };
    spikes_dir().join(".pdfium").join(platform)
}

fn out_dir() -> PathBuf {
    let dir = spikes_dir().join("out");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn report(name: &str, ok: bool, detail: impl AsRef<str>) {
    let mark = if ok { "PASS" } else { "FAIL" };
    println!("[{mark}] {name}: {}", detail.as_ref());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("progressive") => raw::progressive(
            &pdfium_lib_dir(),
            Path::new(&args[2]),
            args.get(3).map(|p| p.parse().unwrap()),
        ),
        Some("heavy") => heavy(),
        Some("reinit") => raw::reinit(&pdfium_lib_dir(), Path::new(&args[2])),
        _ => edit(args.get(2).map(PathBuf::from)),
    }
}

fn edit(third_party: Option<PathBuf>) {
    let lib = Pdfium::pdfium_platform_library_name_at_path(&pdfium_lib_dir());
    let pdfium = Pdfium::new(Pdfium::bind_to_library(lib).expect("load PDFium"));
    let font_bytes = std::fs::read(spikes_dir().join("assets/NotoSans-Regular.ttf")).unwrap();
    let out = out_dir();

    // 1. Add text with an embedded OFL font, save, reopen, extract.
    let latin = "Hello T-Rustable";
    let remove_me = "Remove me";
    let unicode = "Grüße ж € ‰";
    {
        let mut doc = pdfium.create_new_pdf().unwrap();
        let simple = doc
            .fonts_mut()
            .load_true_type_from_bytes(&font_bytes, false)
            .unwrap();
        let cid = doc
            .fonts_mut()
            .load_true_type_from_bytes(&font_bytes, true)
            .unwrap();
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .unwrap();
        let objects = page.objects_mut();
        objects
            .create_text_object(
                PdfPoints::new(72.0),
                PdfPoints::new(700.0),
                latin,
                simple,
                PdfPoints::new(24.0),
            )
            .unwrap();
        objects
            .create_text_object(
                PdfPoints::new(72.0),
                PdfPoints::new(650.0),
                remove_me,
                simple,
                PdfPoints::new(24.0),
            )
            .unwrap();
        objects
            .create_text_object(
                PdfPoints::new(72.0),
                PdfPoints::new(600.0),
                unicode,
                simple,
                PdfPoints::new(24.0),
            )
            .unwrap();
        objects
            .create_text_object(
                PdfPoints::new(72.0),
                PdfPoints::new(550.0),
                unicode,
                cid,
                PdfPoints::new(24.0),
            )
            .unwrap();
        doc.save_to_file(&out.join("edit-1.pdf")).unwrap();
    }
    let text = page_text(&pdfium, &out.join("edit-1.pdf"));
    report(
        "add text, simple font",
        text.contains(latin),
        format!("extracted: {:?}", text.lines().next().unwrap_or("")),
    );
    let unicode_lines: Vec<&str> = text.lines().filter(|l| l.contains("Gr")).collect();
    let simple_line = unicode_lines.first().map(|l| l.trim()).unwrap_or("");
    let cid_line = unicode_lines.get(1).map(|l| l.trim()).unwrap_or("");
    report(
        "add non-Latin text, simple TrueType font",
        simple_line == unicode,
        format!("extracted {simple_line:?}"),
    );
    report(
        "add non-Latin text, CID TrueType font",
        cid_line == unicode,
        format!("extracted {cid_line:?}"),
    );
    let embedded = {
        let doc = pdfium
            .load_pdf_from_file(&out.join("edit-1.pdf"), None)
            .unwrap();
        let page = doc.pages().get(0).unwrap();
        page.objects()
            .iter()
            .filter_map(|o| {
                o.as_text_object()
                    .map(|t| t.font().is_embedded().unwrap_or(false))
            })
            .collect::<Vec<_>>()
    };
    report(
        "font embedded",
        embedded.iter().all(|e| *e),
        format!("{embedded:?}"),
    );

    // 2. Remove a text object, save, reopen, confirm it is gone.
    {
        let doc = pdfium
            .load_pdf_from_file(&out.join("edit-1.pdf"), None)
            .unwrap();
        let mut page = doc.pages().get(0).unwrap();
        let index = page
            .objects()
            .iter()
            .position(|o| o.as_text_object().is_some_and(|t| t.text() == remove_me))
            .expect("find object");
        page.objects_mut().remove_object_at_index(index).unwrap();
        page.regenerate_content().unwrap();
        drop(page);
        doc.save_to_file(&out.join("edit-2.pdf")).unwrap();
    }
    let text = page_text(&pdfium, &out.join("edit-2.pdf"));
    report(
        "remove text object",
        !text.contains(remove_me) && text.contains(latin),
        "own file",
    );

    if let Some(path) = third_party {
        remove_from_third_party(&pdfium, &path, &out);
    }

    // 3. Glyph coverage through ttf-parser.
    let face = ttf_parser::Face::parse(&font_bytes, 0).unwrap();
    let coverage: Vec<(char, bool)> = ['A', 'ü', 'ж', '€', '‰', '中', '🙂']
        .iter()
        .map(|c| (*c, face.glyph_index(*c).is_some()))
        .collect();
    let expected = [true, true, true, true, true, false, false];
    report(
        "glyph coverage (ttf-parser)",
        coverage.iter().map(|c| c.1).eq(expected),
        format!("{coverage:?}"),
    );

    // 4. Character boxes: each glyph box must contain ink in the render.
    char_boxes(&pdfium, &out.join("edit-2.pdf"));
}

/// Writes out/heavy.pdf: page 1 has 20,000 paths, page 2 has 5,000 text objects.
fn heavy() {
    let lib = Pdfium::pdfium_platform_library_name_at_path(&pdfium_lib_dir());
    let pdfium = Pdfium::new(Pdfium::bind_to_library(lib).expect("load PDFium"));
    let font_bytes = std::fs::read(spikes_dir().join("assets/NotoSans-Regular.ttf")).unwrap();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let font = doc
        .fonts_mut()
        .load_true_type_from_bytes(&font_bytes, true)
        .unwrap();

    let mut page = doc
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())
        .unwrap();
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    for i in 0..20_000 {
        let (x, y) = ((i % 140) as f32 * 4.0 + 10.0, (i / 140) as f32 * 5.5 + 10.0);
        let color = PdfColor::new((i % 255) as u8, ((i * 7) % 255) as u8, 120, 255);
        page.objects_mut()
            .create_path_object_rect(
                PdfRect::new_from_values(y, x, y + 4.0, x + 3.0),
                None,
                None,
                Some(color),
            )
            .unwrap();
    }
    page.regenerate_content().unwrap();
    drop(page);

    let mut page = doc
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())
        .unwrap();
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    for i in 0..5_000 {
        let (x, y) = ((i % 50) as f32 * 11.0 + 10.0, (i / 50) as f32 * 8.0 + 10.0);
        page.objects_mut()
            .create_text_object(
                PdfPoints::new(x),
                PdfPoints::new(y),
                "Ab",
                font,
                PdfPoints::new(6.0),
            )
            .unwrap();
    }
    page.regenerate_content().unwrap();
    drop(page);

    doc.save_to_file(&out_dir().join("heavy.pdf")).unwrap();
    println!("wrote {}", out_dir().join("heavy.pdf").display());
}

fn page_text(pdfium: &Pdfium, path: &Path) -> String {
    let doc = pdfium.load_pdf_from_file(path, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    page.text().unwrap().all()
}

fn remove_from_third_party(pdfium: &Pdfium, path: &Path, out: &Path) {
    let doc = pdfium.load_pdf_from_file(path, None).unwrap();
    let mut page = doc.pages().get(0).unwrap();
    let before = page.text().unwrap().all();
    let Some(index) = page.objects().iter().position(|o| {
        o.as_text_object()
            .is_some_and(|t| t.text().trim().chars().count() >= 4)
    }) else {
        report(
            "remove text object, third-party file",
            false,
            "no text object found on page 1",
        );
        return;
    };
    let removed = page
        .objects()
        .get(index)
        .unwrap()
        .as_text_object()
        .unwrap()
        .text();
    page.objects_mut().remove_object_at_index(index).unwrap();
    page.regenerate_content().unwrap();
    drop(page);
    doc.save_to_file(&out.join("third-party-edited.pdf"))
        .unwrap();

    let after = page_text(pdfium, &out.join("third-party-edited.pdf"));
    let removed = removed.trim().to_string();
    let count = |s: &str| s.matches(removed.as_str()).count();
    report(
        "remove text object, third-party file",
        count(&after) < count(&before),
        format!(
            "removed {:?}; occurrences on page 1 before {} after {}; objects on page {}",
            removed,
            count(&before),
            count(&after),
            doc_object_count(pdfium, &out.join("third-party-edited.pdf"))
        ),
    );
}

fn doc_object_count(pdfium: &Pdfium, path: &Path) -> usize {
    let doc = pdfium.load_pdf_from_file(path, None).unwrap();
    doc.pages().get(0).unwrap().objects().len()
}

fn char_boxes(pdfium: &Pdfium, path: &Path) {
    let doc = pdfium.load_pdf_from_file(path, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let scale = 2.0_f32;
    let started = Instant::now();
    let bitmap = page
        .render_with_config(&PdfRenderConfig::new().scale_page_by_factor(scale))
        .unwrap();
    let render_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (w, h) = (bitmap.width() as usize, bitmap.height() as usize);
    let pixels = bitmap.as_raw_bytes();
    let page_h = page.height().value;

    let text = page.text().unwrap();
    let (mut checked, mut with_ink, mut misses) = (0, 0, Vec::new());
    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        if c.is_whitespace() || c.is_control() {
            continue;
        }
        let Ok(rect) = ch.tight_bounds() else {
            continue;
        };
        checked += 1;
        let x0 = (rect.left().value * scale) as usize;
        let x1 = ((rect.right().value * scale) as usize).min(w - 1);
        let y0 = ((page_h - rect.top().value) * scale) as usize;
        let y1 = (((page_h - rect.bottom().value) * scale) as usize).min(h - 1);
        let ink = (y0..=y1).any(|y| (x0..=x1).any(|x| pixels[(y * w + x) * 4] < 128));
        if ink {
            with_ink += 1;
        } else {
            misses.push(c);
        }
    }
    report(
        "character boxes match the render",
        checked > 0 && with_ink == checked,
        format!(
            "{with_ink}/{checked} glyph boxes contain ink; misses {misses:?}; page render at 200% took {render_ms:.1} ms"
        ),
    );
}
