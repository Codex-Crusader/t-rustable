//! Developer tasks.
//!
//! `cargo xtask build-mods` builds the template, the built-in mods, and the test fixture
//! mods for `wasm32-unknown-unknown`, wraps each as a component, and writes
//! `target/mods/<mod id>/` with `manifest.json` and the component. It needs no network.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use t_rustable_sdk::manifest::Manifest;

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
    // Each workspace, and the folders of the mods that it builds.
    let workspaces = [
        (
            mods.clone(),
            [
                mod_folders(&mods.join("builtin"))?,
                mod_folders(&mods.join("fixtures"))?,
            ]
            .concat(),
        ),
        (mods.join("template"), vec![mods.join("template")]),
    ];
    for (workspace, folders) in &workspaces {
        cargo_build(workspace)?;
        let modules = workspace.join("target/wasm32-unknown-unknown/release");
        for folder in folders {
            let id = package(folder, &modules, &out)?;
            println!("Built {id}");
        }
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

/// Wraps the core module of one mod as a component. The `entry` of the manifest is
/// also the file name of the core module that cargo builds.
fn package(folder: &Path, modules: &Path, out: &Path) -> Result<String> {
    let manifest_path = folder.join("manifest.json");
    let json = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let manifest = Manifest::parse(&json).map_err(|problems| {
        let text: Vec<_> = problems.into_iter().map(|p| p.0).collect();
        format!("{}: {}", manifest_path.display(), text.join(" "))
    })?;
    let module_path = modules.join(&manifest.entry);
    let module =
        std::fs::read(&module_path).map_err(|e| format!("{}: {e}", module_path.display()))?;
    let component = wit_component::ComponentEncoder::default()
        .validate(true)
        .module(&module)
        .and_then(|e| e.encode())
        .map_err(|e| format!("{}: {e:#}", module_path.display()))?;
    let dir = out.join(&manifest.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(dir.join("manifest.json"), json).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&manifest.entry), component).map_err(|e| e.to_string())?;
    Ok(manifest.id)
}
