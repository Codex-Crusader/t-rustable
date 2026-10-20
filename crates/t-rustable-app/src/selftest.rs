//! `--self-test <pdf>`: the UI runs a scenario through the normal app code and reports
//! the result here. The app then writes the report and stops.

use crate::commands::App;
use crate::tiles::encode_png;
use std::path::{Path, PathBuf};
use t_rustable_engine::Bitmap;
use tauri::State;

/// Writes a six-page PDF with text and a filled rectangle on each page. CI uses it, so
/// the repository needs no PDF fixture.
pub fn sample(dir: &Path) -> PathBuf {
    const PAGES: usize = 6;
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Count {PAGES} /Kids [{}] >>",
            (0..PAGES)
                .map(|i| format!("{} 0 R", 4 + 2 * i))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    for i in 0..PAGES {
        let mut content = format!(
            "BT /F1 24 Tf 72 720 Td (Page {}) Tj ET
",
            i + 1
        );
        for line in 0..20 {
            content += &format!(
                "BT /F1 11 Tf 72 {} Td (The quick brown fox jumps over the lazy dog.) Tj ET
",
                680 - 16 * line
            );
        }
        content += "0.2 0.4 0.8 rg 72 120 200 100 re f
";
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]              /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * i
        ));
        objects.push(format!(
            "<< /Length {} >>
stream
{content}endstream",
            content.len()
        ));
    }
    let mut pdf = b"%PDF-1.7
"
    .to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(
            format!(
                "{} 0 obj
{o}
endobj
",
                i + 1
            )
            .bytes(),
        );
    }
    let xref = pdf.len();
    pdf.extend(
        format!(
            "xref
0 {}
0000000000 65535 f 
",
            objects.len() + 1
        )
        .bytes(),
    );
    for o in offsets {
        pdf.extend(
            format!(
                "{o:010} 00000 n 
"
            )
            .bytes(),
        );
    }
    pdf.extend(
        format!(
            "trailer
<< /Size {} /Root 1 0 R >>
startxref
{xref}
%%EOF
",
            objects.len() + 1
        )
        .bytes(),
    );
    let path = dir.join("self-test-in.pdf");
    let _ = std::fs::write(&path, pdf);
    path
}

/// A 40 x 20 px PNG for the add-image step.
pub fn picture(dir: &Path) -> PathBuf {
    let bitmap = Bitmap {
        width: 40,
        height: 20,
        stride: 160,
        bgra: (0..800)
            .flat_map(|i| [(i % 40 * 6) as u8, 80, 200, 255])
            .collect(),
    };
    let path = dir.join("self-test.png");
    let _ = std::fs::write(&path, encode_png(&bitmap));
    path
}

/// Stops the app with exit code 2 if the UI does not send a report in time.
pub fn watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(180));
        eprintln!("The self-test did not finish in 180 seconds.");
        std::process::exit(2);
    });
}

/// Writes the report next to the app folders and stops the app. Exit code 0 if all
/// steps passed, else 1.
#[tauri::command]
pub fn self_test_report(app: State<'_, App>, report: serde_json::Value) {
    if app.self_test.is_none() {
        return;
    }
    let ok = report["ok"].as_bool().unwrap_or(false);
    let path = app.shared.folders.log.join("self-test.json");
    let text = serde_json::to_string_pretty(&report).unwrap_or_default();
    let _ = std::fs::write(&path, &text);
    println!("{text}");
    println!("Report: {}", path.display());
    std::process::exit(if ok { 0 } else { 1 });
}
