//! Writes packages. The output is the same for the same files: sorted names, a fixed
//! time, and fixed permissions.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;

use t_rustable_sdk::manifest::Manifest;
use zip::write::SimpleFileOptions;

use crate::{PackageError, check_content, names};

/// Writes a package from its files. The files get the same checks as in `read`.
pub fn write(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, PackageError> {
    for path in files.keys() {
        names::check_path(path).map_err(|m| PackageError::new("unsafe-name", m))?;
    }
    check_content(files)?;
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let io = |e: std::io::Error| PackageError::new("io-error", e.to_string());
    for (path, bytes) in files {
        zip.start_file(path.as_str(), options)
            .map_err(|e| PackageError::new("io-error", e.to_string()))?;
        zip.write_all(bytes).map_err(io)?;
    }
    let cursor = zip
        .finish()
        .map_err(|e| PackageError::new("io-error", e.to_string()))?;
    Ok(cursor.into_inner())
}

/// Collects the files of a mod folder: `manifest.json`, the entry if the folder has it,
/// `README.md` and the license files, and the panel files. A panel in a subfolder brings
/// all files of that folder. A panel at the top brings only its HTML file. Hidden files
/// (names that start with ".") are skipped.
pub fn collect_folder(dir: &Path) -> Result<(Manifest, BTreeMap<String, Vec<u8>>), PackageError> {
    let read = |rel: &str| {
        std::fs::read(dir.join(rel))
            .map_err(|e| PackageError::new("io-error", format!("{rel} cannot be read: {e}.")))
    };
    let json = read("manifest.json")?;
    let text = std::str::from_utf8(&json)
        .map_err(|_| PackageError::new("invalid-manifest", "manifest.json is not UTF-8 text."))?;
    let manifest = Manifest::parse(text).map_err(|problems| {
        let text: Vec<_> = problems.into_iter().map(|p| p.0).collect();
        PackageError::new("invalid-manifest", text.join(" "))
    })?;
    let mut files = BTreeMap::new();
    files.insert("manifest.json".to_string(), json);
    if dir.join(&manifest.entry).is_file() {
        files.insert(manifest.entry.clone(), read(&manifest.entry)?);
    }
    for doc in names::DOC_FILES {
        if dir.join(doc).is_file() {
            files.insert(doc.to_string(), read(doc)?);
        }
    }
    for panel in &manifest.panels {
        match panel.entry.rsplit_once('/') {
            Some((folder, _)) => add_folder(dir, folder, &mut files)?,
            None => {
                files.insert(panel.entry.clone(), read(&panel.entry)?);
            }
        }
    }
    Ok((manifest, files))
}

fn add_folder(
    root: &Path,
    rel: &str,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), PackageError> {
    let io = |e: std::io::Error| PackageError::new("io-error", format!("{rel}: {e}."));
    let mut entries: Vec<_> = std::fs::read_dir(root.join(rel))
        .map_err(io)?
        .collect::<Result<_, _>>()
        .map_err(io)?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{rel}/{name}");
        if entry.file_type().map_err(io)?.is_dir() {
            add_folder(root, &path, files)?;
        } else {
            let bytes = std::fs::read(entry.path()).map_err(io)?;
            files.insert(path, bytes);
        }
    }
    Ok(())
}
