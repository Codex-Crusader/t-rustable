//! `.tmod` mod packages (ADR 0009, ADR 0019): a zip with `manifest.json`, one WASM
//! component, and optional panel files. The reader trusts nothing in the file.

pub mod names;
mod read;
mod write;

#[cfg(feature = "build")]
pub mod build;

use std::collections::BTreeMap;
use std::fmt;

use t_rustable_sdk::manifest::Manifest;

pub use read::{read, read_with};
pub use write::{collect_folder, write};

/// Size limits for one package.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The `.tmod` file.
    pub package: u64,
    pub entries: usize,
    /// One unpacked file.
    pub file: u64,
    /// All unpacked files.
    pub total: u64,
    /// Highest unpacked size divided by packed size, for files over `ratio_from` bytes.
    pub ratio: u64,
    pub ratio_from: u64,
}

const MIB: u64 = 1024 * 1024;

pub const LIMITS: Limits = Limits {
    package: 32 * MIB,
    entries: 1000,
    file: 64 * MIB,
    total: 128 * MIB,
    ratio: 100,
    ratio_from: MIB,
};

/// Mod ID prefixes that only built-in and test mods use.
pub const RESERVED_ID_PREFIXES: [&str; 2] = ["t-rustable.", "test."];

/// A package that passed all checks except the component check, which needs wasmtime.
#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: Manifest,
    /// Path to content. Folders are not listed.
    pub files: BTreeMap<String, Vec<u8>>,
    /// SHA-256 of the package file, in lowercase hex.
    pub sha256: String,
}

impl Package {
    /// The bytes of the component.
    pub fn component(&self) -> &[u8] {
        &self.files[&self.manifest.entry]
    }

    /// Writes the files into an empty folder.
    pub fn extract(&self, dir: &std::path::Path) -> std::io::Result<()> {
        for (path, bytes) in &self.files {
            let target = dir.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, bytes)?;
        }
        Ok(())
    }
}

/// Why a package was refused. `code` is stable (docs/errors.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageError {
    pub code: &'static str,
    pub message: String,
}

impl PackageError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PackageError {}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Checks the files and the manifest of a package. The reader and `collect_folder`
/// both use it, so a package that `mods pack` writes passes the install.
pub(crate) fn check_content(files: &BTreeMap<String, Vec<u8>>) -> Result<Manifest, PackageError> {
    let json = files
        .get("manifest.json")
        .ok_or_else(|| PackageError::new("missing-file", "The package has no manifest.json."))?;
    let json = std::str::from_utf8(json)
        .map_err(|_| PackageError::new("invalid-manifest", "manifest.json is not UTF-8 text."))?;
    let manifest = Manifest::parse(json).map_err(|problems| {
        let text: Vec<_> = problems.into_iter().map(|p| p.0).collect();
        PackageError::new("invalid-manifest", text.join(" "))
    })?;
    if let Some(prefix) = RESERVED_ID_PREFIXES
        .iter()
        .find(|p| manifest.id.starts_with(**p))
    {
        return Err(PackageError::new(
            "reserved-id",
            format!(
                "The mod ID {} starts with {prefix:?}, which only built-in mods can use.",
                manifest.id
            ),
        ));
    }
    if !files.contains_key(&manifest.entry) {
        return Err(PackageError::new(
            "missing-file",
            format!(
                "The package has no {}, the entry of the manifest.",
                manifest.entry
            ),
        ));
    }
    for panel in &manifest.panels {
        if !files.contains_key(&panel.entry) {
            return Err(PackageError::new(
                "missing-file",
                format!(
                    "The package has no {}, the entry of panel {}.",
                    panel.entry, panel.id
                ),
            ));
        }
    }
    for path in files.keys() {
        let allowed = path == "manifest.json"
            || *path == manifest.entry
            || names::DOC_FILES.contains(&path.as_str())
            || names::is_ui_file(path);
        if !allowed {
            return Err(PackageError::new(
                "file-not-allowed",
                format!(
                    "{path} is not allowed in a package. A package has manifest.json, the component, panel files ({}), and README.md or LICENSE.",
                    names::UI_TYPES.join(", ")
                ),
            ));
        }
    }
    Ok(manifest)
}
