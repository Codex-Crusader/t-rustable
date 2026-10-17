//! Limits, failures, handles, and the permission check, with the fixture mods.

mod common;

use common::*;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use t_rustable_plugin_host::{Limits, MAX_FAILURES};

fn limits(time_ms: u64) -> Limits {
    Limits {
        interactive_time: Duration::from_millis(time_ms),
        memory: 64 << 20,
        ..Limits::default()
    }
}

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

const MISBEHAVE: &str = "test.misbehave";

#[test]
fn an_endless_loop_stops_at_the_time_limit() {
    let mut host = host(TestUi::new(Duration::ZERO), limits(300));
    host.load_dir(&mod_dir(MISBEHAVE)).unwrap();
    let started = Instant::now();
    let e = host
        .run_action(MISBEHAVE, None, "loop", "", no_cancel())
        .unwrap_err();
    assert_eq!(e.code, "timeout");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "stopped after {:?}",
        started.elapsed()
    );
    // The mod gets a new instance and works again.
    host.run_action(MISBEHAVE, None, "ok", "", no_cancel())
        .unwrap()
        .value
        .unwrap();
}

#[test]
fn memory_above_the_limit_traps() {
    let mut host = host(TestUi::new(Duration::ZERO), limits(5_000));
    host.load_dir(&mod_dir(MISBEHAVE)).unwrap();
    host.run_action(MISBEHAVE, None, "alloc", "8", no_cancel())
        .unwrap()
        .value
        .unwrap();
    let e = host
        .run_action(MISBEHAVE, None, "alloc", "128", no_cancel())
        .unwrap_err();
    assert_eq!(e.code, "out-of-memory");
    host.run_action(MISBEHAVE, None, "ok", "", no_cancel())
        .unwrap()
        .value
        .unwrap();
}

#[test]
fn three_failures_in_a_row_disable_the_mod() {
    let mut host = host(TestUi::new(Duration::ZERO), limits(5_000));
    host.load_dir(&mod_dir(MISBEHAVE)).unwrap();
    // A success resets the count.
    for _ in 0..MAX_FAILURES - 1 {
        assert_eq!(
            host.run_action(MISBEHAVE, None, "trap", "", no_cancel())
                .unwrap_err()
                .code,
            "mod-failed"
        );
    }
    host.run_action(MISBEHAVE, None, "ok", "", no_cancel())
        .unwrap()
        .value
        .unwrap();
    for _ in 0..MAX_FAILURES {
        assert_eq!(
            host.run_action(MISBEHAVE, None, "trap", "", no_cancel())
                .unwrap_err()
                .code,
            "mod-failed"
        );
    }
    assert!(host.is_disabled(MISBEHAVE));
    let e = host
        .run_action(MISBEHAVE, None, "ok", "", no_cancel())
        .unwrap_err();
    assert_eq!(e.code, "mod-disabled");
    host.enable(MISBEHAVE);
    host.run_action(MISBEHAVE, None, "ok", "", no_cancel())
        .unwrap()
        .value
        .unwrap();
}

#[test]
fn time_in_host_calls_does_not_count() {
    // Five host calls of 150 ms each: 750 ms in the host, with a limit of 300 ms.
    let mut host = host(TestUi::new(Duration::from_millis(150)), limits(300));
    host.load_dir(&mod_dir(MISBEHAVE)).unwrap();
    let started = Instant::now();
    host.run_action(MISBEHAVE, None, "slow-host", "5", no_cancel())
        .unwrap()
        .value
        .unwrap();
    assert!(started.elapsed() >= Duration::from_millis(750));
}

#[test]
fn commits_before_a_trap_stay_in_the_document() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 2);
    let first = doc.lock().unwrap().page_ids()[0];
    let mut host = host(TestUi::new(Duration::ZERO), limits(5_000));
    host.load_dir(&mod_dir(MISBEHAVE)).unwrap();
    let err = host
        .run_action(MISBEHAVE, Some(&doc), "commit-then-trap", "", no_cancel())
        .unwrap_err();
    assert_eq!(err.code, "mod-failed");
    assert_eq!(err.changes.len(), 1);
    assert_eq!(err.changes[0].affected_pages, vec![first]);
    assert_eq!(quarter_turns(&doc, first), 1);
    // The user can undo the change of the failed mod.
    doc.lock().unwrap().undo().unwrap();
    assert_eq!(quarter_turns(&doc, first), 0);
}

#[test]
fn a_transaction_expires_at_the_end_of_its_call() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 1);
    let mut host = host(TestUi::new(Duration::ZERO), limits(5_000));
    host.load_dir(&mod_dir(MISBEHAVE)).unwrap();
    host.run_action(MISBEHAVE, Some(&doc), "keep-transaction", "", no_cancel())
        .unwrap()
        .value
        .unwrap();
    let reply = host
        .run_action(
            MISBEHAVE,
            Some(&doc),
            "use-kept-transaction",
            "",
            no_cancel(),
        )
        .unwrap();
    assert_eq!(reply.value.unwrap_err().code, "expired");
    assert!(reply.changes.is_empty());
}

#[test]
fn a_batch_mod_runs_its_job() {
    let e = engine();
    let dir = tempfile::tempdir().unwrap();
    let doc = session(&e, dir.path(), 1);
    let mut host = host(TestUi::new(Duration::ZERO), Limits::default());
    let id = host.load_dir(&mod_dir("test.batch-only")).unwrap();
    let out = host
        .run_batch(&id, &doc, r#"{"count": 2}"#, no_cancel())
        .unwrap();
    let reply: serde_json::Value = serde_json::from_str(&out.value.unwrap()).unwrap();
    assert_eq!(
        reply,
        serde_json::json!({"pages_before": 1, "pages_after": 3})
    );
    assert_eq!(out.changes.len(), 1);
    assert!(out.changes[0].structure_changed);
    assert_eq!(doc.lock().unwrap().page_ids().len(), 3);
    // The mod has no interactive tool.
    assert_eq!(host.activate(&id).unwrap_err().code, "no-export");
    // Bad arguments come back as the mod's own error, not as a failure.
    let out = host.run_batch(&id, &doc, "[]", no_cancel()).unwrap();
    assert_eq!(out.value.unwrap_err().code, "invalid-argument");
    assert!(!host.is_disabled(&id));
}

/// Copies a built mod to a temporary folder and changes its manifest.
fn changed_copy(id: &str, change: impl FnOnce(&mut serde_json::Value)) -> tempfile::TempDir {
    let from = mod_dir(id);
    let dir = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(&from).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, dir.path().join(path.file_name().unwrap())).unwrap();
    }
    let path = dir.path().join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut manifest);
    std::fs::write(&path, manifest.to_string()).unwrap();
    dir
}

#[test]
fn imports_must_match_the_manifest_permissions() {
    let dir = changed_copy(MISBEHAVE, |m| {
        m["permissions"] = serde_json::json!(["document.read", "document.edit"])
    });
    let mut host = host(TestUi::new(Duration::ZERO), Limits::default());
    let e = host.load_dir(dir.path()).unwrap_err();
    assert_eq!(e.code, "invalid-mod");
    assert!(e.message.contains("ui"), "{}", e.message);
    assert_eq!(host.manifests().count(), 0);
}

#[test]
fn each_manifest_world_needs_its_export() {
    let dir = changed_copy("test.batch-only", |m| {
        m["worlds"] = serde_json::json!(["batch", "interactive"])
    });
    let mut host = host(TestUi::new(Duration::ZERO), Limits::default());
    let e = host.load_dir(dir.path()).unwrap_err();
    assert_eq!(e.code, "invalid-mod");
    assert!(e.message.contains("tool"), "{}", e.message);
}

#[test]
fn a_mod_id_loads_only_once() {
    let mut host = host(TestUi::new(Duration::ZERO), Limits::default());
    host.load_dir(&mod_dir(MISBEHAVE)).unwrap();
    assert_eq!(
        host.load_dir(&mod_dir(MISBEHAVE)).unwrap_err().code,
        "invalid-mod"
    );
    assert!(host.unload(MISBEHAVE));
    host.load_dir(&mod_dir(MISBEHAVE)).unwrap();
}
