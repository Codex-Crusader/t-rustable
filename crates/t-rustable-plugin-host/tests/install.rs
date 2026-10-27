//! The install store: install, update, permission changes, and refusals.

mod common;

use std::time::Duration;

use common::{TestUi, host, mod_dir};
use t_rustable_plugin_host::install::ModStore;
use t_rustable_plugin_host::{Limits, ModHost};
use t_rustable_sdk::permissions::Permission;

/// A package from a built mod, with a new ID and version.
fn package(from: &str, id: &str, version: &str) -> Vec<u8> {
    let (_, mut files) = t_rustable_tmod::collect_folder(&mod_dir(from)).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
    manifest["id"] = id.into();
    manifest["version"] = version.into();
    files.insert("manifest.json".into(), manifest.to_string().into_bytes());
    t_rustable_tmod::write(&files).unwrap()
}

fn new_host() -> ModHost {
    host(TestUi::new(Duration::ZERO), Limits::default())
}

#[test]
fn install_then_load() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path());
    let mut host = new_host();
    let bytes = package("com.example.rotate-page", "com.example.grow", "1.0.0");
    let offer = store
        .stage(&bytes, |m, c| host.check_component(m, c))
        .unwrap();
    assert!(offer.previous.is_none());
    assert_eq!(
        offer.new_permissions,
        [
            Permission::DocumentRead,
            Permission::DocumentEdit,
            Permission::Ui
        ]
    );
    let installed = store.install(offer).unwrap();
    assert_eq!(installed.dir, dir.path().join("com.example.grow"));
    assert_eq!(store.list().len(), 1);
    assert_eq!(host.load_dir(&installed.dir).unwrap(), "com.example.grow");
    // Nothing is left in the staging folder.
    let staging = dir.path().join(".staging");
    assert!(!staging.exists() || std::fs::read_dir(&staging).unwrap().count() == 0);
}

#[test]
fn the_same_package_twice_is_already_installed() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path());
    let host = new_host();
    let bytes = package("com.example.rotate-page", "com.example.same", "1.0.0");
    let offer = store
        .stage(&bytes, |m, c| host.check_component(m, c))
        .unwrap();
    store.install(offer).unwrap();
    let e = store
        .stage(&bytes, |m, c| host.check_component(m, c))
        .unwrap_err();
    assert_eq!(e.code, "already-installed");
}

#[test]
fn an_update_lists_only_new_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path());
    let host = new_host();
    // 1.0.0 is a batch job with document.read and document.edit.
    let v1 = package("test.batch-only", "com.example.grow", "1.0.0");
    let offer = store.stage(&v1, |m, c| host.check_component(m, c)).unwrap();
    store.install(offer).unwrap();
    // 1.1.0 is a tool that also needs ui.
    let v2 = package("com.example.rotate-page", "com.example.grow", "1.1.0");
    let offer = store.stage(&v2, |m, c| host.check_component(m, c)).unwrap();
    assert_eq!(offer.previous.as_ref().unwrap().manifest.version, "1.0.0");
    assert_eq!(offer.new_permissions, [Permission::Ui]);
    store.install(offer).unwrap();
    let now = store.get("com.example.grow").unwrap();
    assert_eq!(now.manifest.version, "1.1.0");
    assert!(now.dir.join("rotate_page.wasm").is_file());
    assert!(
        !now.dir.join("fixture_batch_only.wasm").exists(),
        "old files must go"
    );
    // Back to 1.0.0: no new permissions.
    let offer = store.stage(&v1, |m, c| host.check_component(m, c)).unwrap();
    assert!(offer.new_permissions.is_empty());
    store.discard(offer);
    assert_eq!(
        store.get("com.example.grow").unwrap().manifest.version,
        "1.1.0"
    );
}

#[test]
fn a_damaged_index_stops_install_and_remove_and_stays() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path());
    let host = new_host();
    let bytes = package("com.example.rotate-page", "com.example.kept", "1.0.0");
    let offer = store
        .stage(&bytes, |m, c| host.check_component(m, c))
        .unwrap();
    store.install(offer).unwrap();
    let index = dir.path().join("index.json");
    std::fs::write(&index, b"{ damaged").unwrap();

    let bytes = package("com.example.rotate-page", "com.example.new", "1.0.0");
    let offer = store
        .stage(&bytes, |m, c| host.check_component(m, c))
        .unwrap();
    assert_eq!(store.install(offer).unwrap_err().code, "damaged-file");
    assert!(!dir.path().join("com.example.new").exists());
    assert_eq!(
        store.remove("com.example.kept").unwrap_err().code,
        "damaged-file"
    );
    assert!(dir.path().join("com.example.kept").is_dir());
    assert_eq!(std::fs::read(&index).unwrap(), b"{ damaged");
    assert!(store.list().is_empty());
}

#[test]
fn an_index_key_that_is_a_path_removes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mods = dir.path().join("mods");
    let victim = dir.path().join("victim");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::write(victim.join("keep.txt"), b"x").unwrap();
    let entry = r#"{"sha256": "00", "version": "1.0.0", "permissions": []}"#;
    let index = format!(r#"{{"mods": {{"../victim": {entry}}}}}"#);
    std::fs::write(mods.join("index.json"), index).unwrap();
    let store = ModStore::new(&mods);
    assert_eq!(store.remove("../victim").unwrap_err().code, "not-installed");
    assert!(victim.join("keep.txt").exists());
}

#[test]
fn remove_deletes_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path());
    let host = new_host();
    let bytes = package("com.example.rotate-page", "com.example.gone", "1.0.0");
    let offer = store
        .stage(&bytes, |m, c| host.check_component(m, c))
        .unwrap();
    store.install(offer).unwrap();
    store.remove("com.example.gone").unwrap();
    assert!(store.list().is_empty());
    assert!(!dir.path().join("com.example.gone").exists());
    assert_eq!(
        store.remove("com.example.gone").unwrap_err().code,
        "not-installed"
    );
}

#[test]
fn package_errors_keep_their_codes() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path());
    let host = new_host();
    let e = store
        .stage(b"not a zip", |m, c| host.check_component(m, c))
        .unwrap_err();
    assert_eq!(e.code, "not-a-package");
}

#[test]
fn a_component_that_breaks_its_manifest_is_refused_before_any_file_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path());
    let host = new_host();
    // The manifest says batch only, but the component imports ui.
    let (_, mut files) =
        t_rustable_tmod::collect_folder(&mod_dir("com.example.rotate-page")).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
    manifest["id"] = "com.example.liar".into();
    manifest["permissions"] = serde_json::json!(["document.read", "document.edit"]);
    files.insert("manifest.json".into(), manifest.to_string().into_bytes());
    let bytes = t_rustable_tmod::write(&files).unwrap();
    let e = store
        .stage(&bytes, |m, c| host.check_component(m, c))
        .unwrap_err();
    assert_eq!(e.code, "invalid-mod");
    assert!(!dir.path().join(".staging").exists());
}

#[test]
fn a_mod_that_needs_a_newer_host_is_refused() {
    let host = new_host();
    let component = wat::parse_str(
        r#"(component
            (import "t-rustable:api/host@0.1.9" (instance
                (export "future-func" (func))
            ))
        )"#,
    )
    .unwrap();
    let manifest = t_rustable_sdk::manifest::Manifest::parse(
        r#"{"id":"com.example.future","name":"Future","version":"1.0.0","apiVersion":"0.1",
            "author":"A","description":"D","permissions":[],"entry":"m.wasm","worlds":["batch"]}"#,
    )
    .unwrap();
    let e = host.check_component(&manifest, &component).unwrap_err();
    assert_eq!(e.code, "invalid-mod");
    assert!(e.message.contains("does not have"), "{}", e.message);
}

#[test]
fn clean_removes_what_a_stopped_install_left() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".staging/x")).unwrap();
    std::fs::create_dir_all(dir.path().join(".old/y")).unwrap();
    ModStore::new(dir.path()).clean();
    assert!(!dir.path().join(".staging").exists());
    assert!(!dir.path().join(".old").exists());
}

#[test]
fn the_example_packages_install_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let store = ModStore::new(dir.path());
    let mut host = new_host();
    for id in ["example.stamp", "example.page-numbers"] {
        let path = common::repo().join(format!("target/examples/{id}.tmod"));
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{}: {e}. Run: cargo xtask build-mods", path.display()));
        let offer = store
            .stage(&bytes, |m, c| host.check_component(m, c))
            .unwrap();
        let installed = store.install(offer).unwrap();
        assert_eq!(host.load_dir(&installed.dir).unwrap(), id);
    }
    assert!(dir.path().join("example.stamp/panel/panel.js").is_file());
}
