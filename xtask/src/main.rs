//! Developer tasks.
//!
//! `cargo xtask build-mods` builds the template, the built-in mods, the test fixture mods,
//! and the example mods for `wasm32-unknown-unknown`, and wraps each as a component.
//! It writes `target/mods/<mod id>/` with `manifest.json`, the component, and the panel
//! files. Examples go to `target/examples/<mod id>/` and `target/examples/<mod id>.tmod`
//! instead, because the app does not load them. It needs no network.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

type Result<T> = std::result::Result<T, String>;

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        Some("build-mods") => build_mods(),
        _ => Err("Usage: cargo xtask build-mods".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Mod folders in a workspace: each folder with a `manifest.json`.
fn mod_folders(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.join("manifest.json").is_file() {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn build_mods() -> Result<()> {
    let root = root();
    let mods = root.join("mods");
    let out = root.join("target/mods");
    let examples = root.join("target/examples");
    cargo_build(&mods)?;
    cargo_build(&mods.join("template"))?;
    let modules = |workspace: &Path| workspace.join("target/wasm32-unknown-unknown/release");
    let installed = [
        mod_folders(&mods.join("builtin"))?,
        mod_folders(&mods.join("fixtures"))?,
    ]
    .concat();
    for folder in &installed {
        let id = package(folder, &modules(&mods), &out, false)?;
        println!("Built {id}");
    }
    let template = mods.join("template");
    let id = package(&template, &modules(&template), &out, false)?;
    println!("Built {id}");
    for folder in mod_folders(&mods.join("examples"))? {
        let id = package(&folder, &modules(&mods), &examples, true)?;
        println!("Built {id} and target/examples/{id}.tmod");
    }
    Ok(())
}

fn cargo_build(workspace: &Path) -> Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    // The current folder selects `.cargo/config.toml`, which sets the wasm target.
    let status = Command::new(cargo)
        .args(["build", "--release", "--quiet"])
        .current_dir(workspace)
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .map_err(|e| format!("cargo cannot start: {e}"))?;
    if !status.success() {
        return Err(format!(
            "The build in {} failed. If the wasm target is missing, run: rustup target add wasm32-unknown-unknown",
            workspace.display()
        ));
    }
    Ok(())
}

/// Wraps the core module of one mod as a component and writes the mod files into
/// `out/<id>/`. With `tmod`, it also writes `out/<id>.tmod`, which passes the install
/// checks. The `entry` of the manifest is also the file name of the core module.
fn package(folder: &Path, modules: &Path, out: &Path, tmod: bool) -> Result<String> {
    let fail = |e: t_rustable_tmod::PackageError| format!("{}: {e}", folder.display());
    let (manifest, mut files) = t_rustable_tmod::collect_folder(folder).map_err(fail)?;
    let module_path = modules.join(&manifest.entry);
    let module =
        std::fs::read(&module_path).map_err(|e| format!("{}: {e}", module_path.display()))?;
    let component = t_rustable_tmod::build::componentize(&module).map_err(fail)?;
    files.insert(manifest.entry.clone(), component);
    let dir = out.join(&manifest.id);
    let _ = std::fs::remove_dir_all(&dir);
    for (path, bytes) in &files {
        let target = dir.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    if tmod {
        let bytes = t_rustable_tmod::write(&files).map_err(fail)?;
        t_rustable_tmod::read(&bytes).map_err(fail)?;
        let path = out.join(format!("{}.tmod", manifest.id));
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(manifest.id)
}
