//! Builds packages from mod folders, for `t-rustable mods pack` and xtask.

use std::path::{Path, PathBuf};

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
    Ok((package, bytes))
}
