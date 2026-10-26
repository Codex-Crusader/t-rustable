//! Finds the mods that a command needs. The command line uses the built-in mods and the
//! installed mods, the same as the app. It never loads developer folders, and it only
//! reads the mod settings of the app (`config/mods.json`). Install and remove change
//! only the mods folder, so a running app does not lose a change.

use std::collections::BTreeSet;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use serde_json::{Value, json};
use t_rustable_core::fonts::Fonts;
use t_rustable_core::paths::{Folders, Resources};
use t_rustable_plugin_host::install::ModStore;
use t_rustable_plugin_host::services::MemorySettings;
use t_rustable_plugin_host::{Limits, ModHost, Services};
use t_rustable_sdk::manifest::{Manifest, World};

use crate::bridge::{CliFiles, CliUi, Notes};
use crate::report::{self, Problem};
use crate::run::ModRef;

/// IDs of the mods that the user turned off in the app.
pub fn turned_off(folders: &Folders) -> BTreeSet<String> {
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct Config {
        disabled: BTreeSet<String>,
    }
    std::fs::read(folders.config.join("mods.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Config>(&b).ok())
        .unwrap_or_default()
        .disabled
}

fn read_manifest(dir: &std::path::Path) -> Option<Manifest> {
    let json = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    Manifest::parse(&json).ok()
}

/// Finds a mod with a batch job.
pub fn resolve(id: &str, folders: &Folders, resources: &Resources) -> Result<ModRef, Problem> {
    let found: Option<(PathBuf, Manifest)> = if id.starts_with("t-rustable.") {
        let builtin = resources
            .builtin_mods()
            .map_err(|e| Problem::new("internal", e))?;
        let dir = builtin.join(id);
        read_manifest(&dir).map(|m| (dir, m))
    } else {
        ModStore::new(&folders.mods)
            .get(id)
            .map(|m| (m.dir, m.manifest))
    };
    let Some((dir, manifest)) = found else {
        return Err(
            Problem::new("not-found", format!("The mod {id} is not installed."))
                .hint("Run: t-rustable mods list"),
        );
    };
    if turned_off(folders).contains(id) {
        return Err(Problem::new(
            "mod-disabled",
            format!("The mod {id} is turned off in the app."),
        )
        .hint("Turn it on in the app: Mods, Manage mods."));
    }
    if !manifest.worlds.contains(&World::Batch) {
        return Err(Problem::new(
            "no-batch-job",
            format!(
                "The mod {} ({id}) has no batch job. It works only in the app.",
                manifest.name
            ),
        ));
    }
    Ok(ModRef { id: id.into(), dir })
}

/// A mod host only for the component checks. It runs no mod code.
fn checker(resources: &Resources) -> Result<ModHost, Problem> {
    let notes = Arc::new(Notes::new(false));
    let services = Services {
        ui: Arc::new(CliUi(notes.clone())),
        files: Arc::new(CliFiles(notes)),
        settings: Arc::new(MemorySettings::default()),
        fonts: Arc::new(Fonts::load(&resources.fonts)?),
    };
    Ok(ModHost::new(services, Limits::default())?)
}

fn row(m: &Manifest, source: &str, on: bool, sha256: Option<&str>) -> Value {
    json!({
        "id": m.id,
        "name": m.name,
        "version": m.version,
        "source": source,
        "on": on,
        "batch": m.worlds.contains(&World::Batch),
        "permissions": m.permissions,
        "sha256": sha256,
    })
}

fn permissions(m: &Manifest) -> String {
    if m.permissions.is_empty() {
        "none".into()
    } else {
        let p: Vec<String> = m.permissions.iter().map(ToString::to_string).collect();
        p.join(", ")
    }
}

/// `mods list`: the built-in and the installed mods.
pub fn list(json: bool) -> ExitCode {
    let folders = Folders::standard();
    let off = turned_off(&folders);
    let mut rows = Vec::new();
    match Resources::find().and_then(|r| r.builtin_mods().map(Path::to_path_buf)) {
        Ok(dir) => {
            let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .filter_map(|e| Some(e.ok()?.path()))
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("t-rustable."))
                })
                .collect();
            dirs.sort();
            for d in dirs {
                if let Some(m) = read_manifest(&d) {
                    rows.push(row(&m, "builtin", !off.contains(&m.id), None));
                }
            }
        }
        Err(e) => eprintln!("warning: {e}"),
    }
    for m in ModStore::new(&folders.mods).list() {
        let on = !off.contains(&m.manifest.id);
        rows.push(row(&m.manifest, "installed", on, Some(&m.sha256)));
    }
    if json {
        let report = json!({ "ok": true, "mods": rows });
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return ExitCode::SUCCESS;
    }
    for r in &rows {
        let mut marks = vec![r["source"].as_str().unwrap_or_default()];
        if r["on"] == false {
            marks.push("off");
        }
        if r["batch"] == true {
            marks.push("batch");
        }
        println!(
            "{} {} ({}) [{}]",
            r["id"].as_str().unwrap_or_default(),
            r["version"].as_str().unwrap_or_default(),
            r["name"].as_str().unwrap_or_default(),
            marks.join(", ")
        );
    }
    if rows.is_empty() {
        println!("No mods.");
    }
    ExitCode::SUCCESS
}

fn done(json: bool, value: Value, text: &str) -> ExitCode {
    if json {
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
    } else {
        println!("{text}");
    }
    ExitCode::SUCCESS
}

fn fail(json: bool, e: &Problem, exit: u8) -> ExitCode {
    if json {
        let report = json!({ "ok": false, "error": e });
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        eprintln!("error: {e}");
    }
    ExitCode::from(exit)
}

fn read_package(file: &Path) -> Result<t_rustable_tmod::Package, Problem> {
    let bytes = std::fs::read(file)
        .map_err(|e| Problem::new("io-error", format!("{}: {e}", file.display())))?;
    t_rustable_tmod::read(&bytes).map_err(|e| Problem::new(e.code, e.message))
}

/// `mods install`: the same checks and permission list as the app.
pub fn install(file: &Path, yes: bool, json: bool) -> ExitCode {
    let folders = Folders::standard();
    let store = ModStore::new(&folders.mods);
    let staged = std::fs::read(file)
        .map_err(|e| Problem::new("io-error", format!("{}: {e}", file.display())))
        .and_then(|bytes| {
            let resources = Resources::find().map_err(|e| Problem::new("internal", e))?;
            let host = checker(&resources)?;
            Ok(store.stage(&bytes, |m, b| host.check_component(m, b))?)
        });
    let offer = match staged {
        Ok(o) => o,
        Err(e) => return fail(json, &e, report::FAILED),
    };
    let m = &offer.manifest;
    let out = &mut std::io::stderr();
    let _ = writeln!(out, "{} {} by {} ({})", m.name, m.version, m.author, m.id);
    if let Some(p) = &offer.previous {
        let _ = writeln!(out, "This updates version {}.", p.manifest.version);
    }
    let _ = writeln!(out, "Permissions: {}", permissions(m));
    if offer.previous.is_some() && !offer.new_permissions.is_empty() {
        let new: Vec<String> = offer
            .new_permissions
            .iter()
            .map(ToString::to_string)
            .collect();
        let _ = writeln!(out, "New permissions: {}", new.join(", "));
    }
    let _ = writeln!(out, "SHA-256: {}", offer.sha256);
    if !yes && (json || !std::io::stdin().is_terminal()) {
        store.discard(offer);
        let e = Problem::new("not-confirmed", "The install needs a yes.")
            .hint("Read the permissions, then add --yes.");
        return fail(json, &e, report::USAGE);
    }
    if !yes {
        eprint!("Install? [y/N] ");
        let mut answer = String::new();
        let _ = std::io::stdin().read_line(&mut answer);
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            store.discard(offer);
            let e = Problem::new("cancelled", "Nothing was installed.");
            return fail(json, &e, report::FAILED);
        }
    }
    match store.install(offer) {
        Ok(m) => done(
            json,
            json!({ "ok": true, "mod": row(&m.manifest, "installed", true, Some(&m.sha256)) }),
            &format!(
                "Installed {} {}. A running app loads it after a restart.",
                m.manifest.name, m.manifest.version
            ),
        ),
        Err(e) => fail(json, &e.into(), report::FAILED),
    }
}

/// `mods remove`: only installed mods.
pub fn remove(id: &str, json: bool) -> ExitCode {
    if id.starts_with("t-rustable.") {
        let e = Problem::new("invalid-argument", "Built-in mods cannot be removed.")
            .hint("Turn it off in the app: Mods, Manage mods.");
        return fail(json, &e, report::USAGE);
    }
    match ModStore::new(&Folders::standard().mods).remove(id) {
        Ok(()) => done(
            json,
            json!({ "ok": true, "removed": id }),
            &format!("Removed {id}. A running app keeps it until a restart."),
        ),
        Err(e) => fail(json, &e.into(), report::FAILED),
    }
}

fn verify_folder(host: &ModHost, dir: &Path) -> Result<Manifest, Problem> {
    let text = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|e| Problem::new("invalid-mod", format!("manifest.json: {e}")))?;
    let manifest = Manifest::parse(&text).map_err(|p| {
        let all: Vec<&str> = p.iter().map(|x| x.0.as_str()).collect();
        Problem::new("invalid-manifest", all.join(" "))
    })?;
    let bytes = std::fs::read(dir.join(&manifest.entry))
        .map_err(|e| Problem::new("invalid-mod", format!("{}: {e}", manifest.entry)))?;
    host.check_component(&manifest, &bytes)?;
    Ok(manifest)
}

/// `mods verify`: the checks of an install, on a package file or on a mod by ID.
pub fn verify(target: &str, json: bool) -> ExitCode {
    let checked = Resources::find()
        .map_err(|e| Problem::new("internal", e))
        .and_then(|resources| {
            let host = checker(&resources)?;
            let path = Path::new(target);
            if path.is_file() {
                let package = read_package(path)?;
                host.check_component(&package.manifest, package.component())?;
                return Ok((package.manifest, Some(package.sha256)));
            }
            let dir = if target.starts_with("t-rustable.") {
                let builtin = resources
                    .builtin_mods()
                    .map_err(|e| Problem::new("internal", e))?;
                builtin.join(target)
            } else {
                ModStore::new(&Folders::standard().mods)
                    .get(target)
                    .map(|m| m.dir)
                    .ok_or_else(|| {
                        Problem::new(
                            "not-found",
                            format!("{target} is not a package file or an installed mod."),
                        )
                    })?
            };
            Ok((verify_folder(&host, &dir)?, None))
        });
    match checked {
        Ok((m, sha256)) => {
            let mut text = format!(
                "OK: {} {} ({}). Permissions: {}.",
                m.name,
                m.version,
                m.id,
                permissions(&m)
            );
            if let Some(s) = &sha256 {
                text.push_str(&format!(" SHA-256: {s}"));
            }
            let source = if sha256.is_some() {
                "package"
            } else {
                "folder"
            };
            let value = json!({ "ok": true, "mod": row(&m, source, true, sha256.as_deref()) });
            done(json, value, &text)
        }
        Err(e) => fail(json, &e, report::FAILED),
    }
}
