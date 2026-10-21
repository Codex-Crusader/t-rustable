//! Builds packages from mod folders, for `t-rustable mods pack` and xtask.

use std::path::{Path, PathBuf};

use t_rustable_sdk::manifest::{Manifest, World};
use t_rustable_sdk::permissions::check_imports;
use wasmparser::{Parser, Payload};

use crate::{Package, PackageError};

/// Wraps a core module as a component. A component stays as it is.
pub fn componentize(wasm: &[u8]) -> Result<Vec<u8>, PackageError> {
    if wasmparser::Parser::is_component(wasm) {
        return Ok(wasm.to_vec());
    }
    wit_component::ComponentEncoder::default()
        .validate(true)
        .module(wasm)
        .and_then(|e| e.encode())
        .map_err(|e| PackageError::new("invalid-component", format!("{e:#}")))
}

/// The import and export names at the top level of a component.
fn top_level_names(component: &[u8]) -> Result<(Vec<String>, Vec<String>), PackageError> {
    let bad =
        |e: wasmparser::BinaryReaderError| PackageError::new("invalid-component", e.to_string());
    let (mut imports, mut exports) = (Vec::new(), Vec::new());
    // Each nested module or component starts with a version payload and ends with End.
    let mut depth = 0usize;
    for payload in Parser::new(0).parse_all(component) {
        match payload.map_err(bad)? {
            Payload::Version { .. } => depth += 1,
            Payload::End(_) => depth = depth.saturating_sub(1),
            Payload::ComponentImportSection(section) if depth == 1 => {
                for import in section {
                    imports.push(import.map_err(bad)?.name.full_name().into_owned());
                }
            }
            Payload::ComponentExportSection(section) if depth == 1 => {
                for export in section {
                    exports.push(export.map_err(bad)?.name.full_name().into_owned());
                }
            }
            _ => {}
        }
    }
    Ok((imports, exports))
}

/// The checks of the install that need no WASM engine: the imports match the manifest
/// permissions, and each manifest world is exported. The install also checks that the
/// host has each imported function.
pub fn check_component(manifest: &Manifest, component: &[u8]) -> Result<(), PackageError> {
    if !Parser::is_component(component) {
        return Err(PackageError::new(
            "invalid-component",
            format!("{} is not a WebAssembly component.", manifest.entry),
        ));
    }
    let (imports, exports) = top_level_names(component)?;
    let problems = check_imports(imports.iter().map(String::as_str), &manifest.permissions);
    if !problems.is_empty() {
        let text: Vec<_> = problems.iter().map(ToString::to_string).collect();
        return Err(PackageError::new("invalid-component", text.join(" ")));
    }
    for (world, interface) in [(World::Interactive, "tool"), (World::Batch, "batch-job")] {
        let prefix = format!("t-rustable:api/{interface}@0.1.");
        if manifest.worlds.contains(&world) && !exports.iter().any(|e| e.starts_with(&prefix)) {
            return Err(PackageError::new(
                "invalid-component",
                format!(
                    "The manifest lists the {world:?} world, but the mod does not export {interface}."
                ),
            ));
        }
    }
    Ok(())
}

/// Where cargo puts the core module of a mod in `dir`. The `entry` of the manifest is
/// also the file name of the core module.
pub fn cargo_output(dir: &Path, entry: &str) -> PathBuf {
    dir.join("target/wasm32-unknown-unknown/release")
        .join(entry)
}

/// Builds a package from a mod folder. The component comes from `module` if given,
/// else from the cargo output in the folder, else from the entry file in the folder.
/// Returns the package bytes, checked by `read`.
pub fn pack(dir: &Path, module: Option<&Path>) -> Result<(Package, Vec<u8>), PackageError> {
    let (manifest, mut files) = crate::collect_folder(dir)?;
    let source = match module {
        Some(path) => Some(path.to_path_buf()),
        None => {
            let built = cargo_output(dir, &manifest.entry);
            built.is_file().then_some(built)
        }
    };
    if let Some(path) = source {
        let wasm = std::fs::read(&path).map_err(|e| {
            PackageError::new(
                "io-error",
                format!("{} cannot be read: {e}.", path.display()),
            )
        })?;
        files.insert(manifest.entry.clone(), componentize(&wasm)?);
    } else if let Some(wasm) = files.get(&manifest.entry) {
        let component = componentize(wasm)?;
        files.insert(manifest.entry.clone(), component);
    }
    let bytes = crate::write(&files)?;
    let package = crate::read(&bytes)?;
    check_component(&package.manifest, package.component())?;
    Ok((package, bytes))
}
