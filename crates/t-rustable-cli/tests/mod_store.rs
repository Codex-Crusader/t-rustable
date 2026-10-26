//! `mods list|install|remove|verify` and a batch run of an installed mod, through the
//! real program.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

fn package() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/examples/example.page-numbers.tmod");
    assert!(p.is_file(), "Run: cargo xtask build-mods");
    p
}

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_t-rustable"))
        .args(args)
        .env("T_RUSTABLE_HOME", home)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn json(home: &Path, args: &[&str]) -> (i32, Value) {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let out = run(home, &all);
    let report = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (out.status.code().unwrap(), report)
}

fn ids(list: &Value) -> Vec<(String, String)> {
    list["mods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["id"].as_str().unwrap().to_string(),
                m["source"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn install_run_verify_and_remove() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let package = package();
    let package = package.to_str().unwrap();
    let id = "example.page-numbers";

    // Without --yes and without a terminal, nothing is installed.
    let out = run(&home, &["mods", "install", package]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Permissions: document.read, document.edit")
    );
    let (_, list) = json(&home, &["mods", "list"]);
    assert!(!ids(&list).iter().any(|(i, _)| i == id));
    assert!(ids(&list).contains(&("t-rustable.page-tools".into(), "builtin".into())));

    let (code, r) = json(&home, &["mods", "install", package, "--yes"]);
    assert_eq!(code, 0, "{r}");
    assert_eq!(r["mod"]["batch"], true);
    let (code, r) = json(&home, &["mods", "install", package, "--yes"]);
    assert_eq!(
        (code, &r["error"]["code"]),
        (1, &"already-installed".into())
    );
    let (_, list) = json(&home, &["mods", "list"]);
    assert!(ids(&list).contains(&(id.into(), "installed".into())));

    // The installed mod runs as a batch job.
    let pdf = tmp.path().join("a.pdf");
    std::fs::write(
        &pdf,
        t_rustable_engine::testing::blank_pdf(&[(300.0, 300.0); 2]),
    )
    .unwrap();
    let out_dir = tmp.path().join("out");
    let (code, r) = json(
        &home,
        &[
            "run",
            id,
            pdf.to_str().unwrap(),
            "--out-dir",
            out_dir.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{r}");
    let (code, r) = json(
        &home,
        &[
            "export",
            out_dir.join("a.pdf").to_str().unwrap(),
            "--format",
            "txt",
            "--out-dir",
            out_dir.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{r}");
    let text = std::fs::read_to_string(out_dir.join("a.txt")).unwrap();
    assert!(text.contains("Page 2 of 2"), "{text:?}");

    for target in [package, id, "t-rustable.page-tools"] {
        let (code, r) = json(&home, &["mods", "verify", target]);
        assert_eq!(code, 0, "{target}: {r}");
    }
    let junk = tmp.path().join("junk.tmod");
    std::fs::write(&junk, b"not a zip").unwrap();
    let (code, _) = json(&home, &["mods", "verify", junk.to_str().unwrap()]);
    assert_eq!(code, 1);

    let (code, _) = json(&home, &["mods", "remove", "t-rustable.page-tools"]);
    assert_eq!(code, 2);
    let (code, r) = json(&home, &["mods", "remove", id]);
    assert_eq!(code, 0, "{r}");
    let (code, r) = json(&home, &["mods", "remove", id]);
    assert_eq!((code, &r["error"]["code"]), (1, &"not-installed".into()));
    let (code, r) = json(&home, &["run", id, pdf.to_str().unwrap(), "--dry-run"]);
    assert_eq!((code, &r["error"]["code"]), (2, &"not-found".into()));
}
