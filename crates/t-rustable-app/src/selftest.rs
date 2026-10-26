//! `--self-test <pdf>`: the UI runs a scenario through the normal app code and reports
//! the result here. The app then writes the report and stops.

use crate::commands::App;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
    let _ = std::fs::write(&path, bitmap.to_png());
    path
}

/// Calls of `self_test_secret`. A panel must never reach it (the M1 escape checks).
static SECRET_CALLS: AtomicUsize = AtomicUsize::new(0);

#[tauri::command]
pub fn self_test_secret() -> &'static str {
    SECRET_CALLS.fetch_add(1, Ordering::SeqCst);
    "secret"
}

#[tauri::command]
pub fn self_test_secret_calls() -> usize {
    SECRET_CALLS.load(Ordering::SeqCst)
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

/// The packages and the developer folder for the mod steps: the stamp example, the
/// same mod as version 0.2.0, and a copy of the unpacked stamp with another ID.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTestPackages {
    stamp: String,
    stamp_update: String,
    dev_folder: String,
}

const DEV_ID: &str = "example.dev-stamp";

fn dev_folder(app: &App) -> PathBuf {
    app.shared.folders.log.join("self-test-dev").join(DEV_ID)
}

fn set_manifest(path: &Path, f: impl FnOnce(&mut serde_json::Value)) -> Result<Vec<u8>, String> {
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    f(&mut json);
    let bytes = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
    std::fs::write(path, &bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn self_test_packages(app: State<'_, App>) -> Result<SelfTestPackages, String> {
    if app.self_test.is_none() {
        return Err("Only for the self-test.".into());
    }
    if std::env::var_os("T_RUSTABLE_HOME").is_none() {
        return Err(
            "Skipped: set T_RUSTABLE_HOME, so that the mod steps do not change your mods.".into(),
        );
    }
    let builtin = app.mods.call(|m| m.builtin_dir().to_path_buf()).await?;
    let examples = builtin
        .parent()
        .ok_or("The built-in mod folder has no parent.")?
        .join("examples");
    let stamp = examples.join("example.stamp.tmod");
    let mut package = t_rustable_tmod::read(
        &std::fs::read(&stamp)
            .map_err(|e| format!("{}: {e}. Run: cargo xtask build-mods", stamp.display()))?,
    )
    .map_err(|e| e.message)?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&package.files["manifest.json"]).map_err(|e| e.to_string())?;
    manifest["version"] = "0.2.0".into();
    package.files.insert(
        "manifest.json".into(),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    );
    let update = app.shared.folders.log.join("self-test-stamp-0.2.0.tmod");
    let bytes = t_rustable_tmod::write(&package.files).map_err(|e| e.message)?;
    std::fs::write(&update, bytes).map_err(|e| e.to_string())?;

    let dev = dev_folder(&app);
    let _ = std::fs::remove_dir_all(&dev);
    copy_dir(&examples.join("example.stamp"), &dev).map_err(|e| e.to_string())?;
    set_manifest(&dev.join("manifest.json"), |m| {
        m["id"] = DEV_ID.into();
        m["name"] = "Developer stamp".into();
    })?;
    Ok(SelfTestPackages {
        stamp: stamp.display().to_string(),
        stamp_update: update.display().to_string(),
        dev_folder: dev.display().to_string(),
    })
}

/// Changes the name in the manifest of the developer folder copy.
#[tauri::command]
pub fn self_test_rename_dev(app: State<'_, App>, name: String) -> Result<(), String> {
    if app.self_test.is_none() {
        return Err("Only for the self-test.".into());
    }
    set_manifest(&dev_folder(&app).join("manifest.json"), |m| {
        m["name"] = name.into();
    })
    .map(drop)
}
