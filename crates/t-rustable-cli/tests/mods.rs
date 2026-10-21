//! `t-rustable mods new` and `mods pack`, through the real program.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn t_rustable(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_t-rustable"))
        .args(args)
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn new_then_build_then_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("my-tool");
    let sdk = repo().join("crates/t-rustable-sdk");
    let out = t_rustable(&[
        "mods",
        "new",
        "My tool",
        "--dir",
        dir.to_str().unwrap(),
        "--sdk",
        sdk.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out));
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    let manifest = t_rustable_sdk::manifest::Manifest::parse(&manifest).unwrap();
    assert_eq!(manifest.id, "com.example.my-tool");
    assert_eq!(manifest.name, "My tool");
    assert_eq!(manifest.entry, "my_tool.wasm");
    let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("name = \"my-tool\""), "{cargo}");

    // The folder is not empty now, so a second `new` stops.
    let again = t_rustable(&["mods", "new", "My tool", "--dir", dir.to_str().unwrap()]);
    assert!(!again.status.success());

    // Build the new mod as an author does, with a shared target folder to save time.
    let target = repo().join("target/cli-test-mods");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let build = Command::new(cargo)
        .args(["build", "--release", "--quiet"])
        .current_dir(&dir)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .unwrap();
    assert!(build.status.success(), "{}", text(&build));
    let module = target.join("wasm32-unknown-unknown/release/my_tool.wasm");

    let package = tmp.path().join("out.tmod");
    let out = t_rustable(&[
        "mods",
        "pack",
        dir.to_str().unwrap(),
        "--module",
        module.to_str().unwrap(),
        "-o",
        package.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("document.read, document.edit, ui"),
        "{}",
        text(&out)
    );
    let read = t_rustable_tmod::read(&std::fs::read(&package).unwrap()).unwrap();
    assert_eq!(read.manifest.id, "com.example.my-tool");
    assert!(read.files.contains_key("my_tool.wasm"));
    assert!(
        !read
            .files
            .keys()
            .any(|k| k.contains("Cargo") || k.starts_with("src"))
    );
}

#[test]
fn pack_refuses_a_manifest_that_does_not_match_the_component() {
    let example = repo().join("target/examples/example.stamp");
    assert!(
        example.join("manifest.json").is_file(),
        "Run: cargo xtask build-mods"
    );
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("stamp");
    copy(&example, &dir);
    let path = dir.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["permissions"] = serde_json::json!(["document.read", "document.edit"]);
    manifest["panels"] = serde_json::json!([]);
    std::fs::write(&path, manifest.to_string()).unwrap();

    let out = t_rustable(&["mods", "pack", dir.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(text(&out).contains("ui"), "{}", text(&out));
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
