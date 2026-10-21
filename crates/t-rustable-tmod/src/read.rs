//! The package reader. All checks run before any file is written to disk.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;

use crate::{LIMITS, Limits, Package, PackageError, check_content, names, sha256_hex};

const EOCD: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const ZIP64_LOCATOR: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
const EOCD_LEN: usize = 22;
/// Largest central directory that `LIMITS.entries` entries can need.
const MAX_DIRECTORY: u64 = 1024 * 1024;

/// Reads and checks a package with the standard limits.
pub fn read(bytes: &[u8]) -> Result<Package, PackageError> {
    read_with(bytes, &LIMITS)
}

pub fn read_with(bytes: &[u8], limits: &Limits) -> Result<Package, PackageError> {
    if bytes.len() as u64 > limits.package {
        return Err(too_large(format!(
            "The package is larger than {} MiB.",
            limits.package / (1024 * 1024)
        )));
    }
    let listed = check_end_records(bytes, limits)?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| {
        PackageError::new(
            "not-a-package",
            format!("The file is not a valid zip: {e}."),
        )
    })?;
    if archive.len() > limits.entries {
        return Err(too_large(format!(
            "The package has more than {} files.",
            limits.entries
        )));
    }
    // The zip crate keeps only one entry of each name. Other zip tools can show the
    // other entry, so a package with a hidden duplicate is refused.
    if listed.is_some_and(|n| usize::from(n) != archive.len()) {
        return Err(PackageError::new(
            "duplicate-name",
            "The package has two entries with the same name.",
        ));
    }

    let mut files = BTreeMap::new();
    let mut folders = BTreeSet::new();
    let mut lower: HashMap<String, String> = HashMap::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let (path, is_dir) = {
            let entry = archive
                .by_index_raw(i)
                .map_err(|e| bad_zip(format!("Entry {i} cannot be read: {e}.")))?;
            let raw = entry.name_raw();
            let name = std::str::from_utf8(raw)
                .map_err(|_| unsafe_name(format!("Entry {i} has a name that is not ASCII.")))?;
            let is_dir = entry.is_dir();
            let path = name.strip_suffix('/').unwrap_or(name).to_string();
            names::check_path(&path).map_err(unsafe_name)?;
            if entry.encrypted() {
                return Err(unsupported(format!("{path} is encrypted.")));
            }
            if !matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            ) {
                return Err(unsupported(format!(
                    "{path} uses a compression that is not allowed. Use deflate or no compression."
                )));
            }
            // The kind bits of a Unix mode: only plain files and folders.
            if let Some(mode) = entry.unix_mode() {
                let kind = mode & 0o170000;
                if kind != 0 && kind != 0o100000 && kind != 0o040000 {
                    return Err(unsupported(format!("{path} is a link or a special file.")));
                }
            }
            if !is_dir && entry.size() > limits.file {
                return Err(too_large(format!("{path} is larger than the limit.")));
            }
            (path, is_dir)
        };

        let key = path.to_ascii_lowercase();
        if let Some(other) = lower.get(&key) {
            if *other == path && is_dir && folders.contains(&path) {
                continue;
            }
            let code = if *other == path {
                "duplicate-name"
            } else {
                "case-collision"
            };
            return Err(PackageError::new(
                code,
                format!("{path} and {other} have the same name on Windows."),
            ));
        }
        lower.insert(key, path.clone());

        // Each folder above the file, so that a file cannot have the name of a folder.
        let mut parent = path.as_str();
        while let Some((up, _)) = parent.rsplit_once('/') {
            folders.insert(up.to_string());
            parent = up;
        }
        if is_dir {
            folders.insert(path);
            continue;
        }

        let mut entry = archive
            .by_index(i)
            .map_err(|e| bad_zip(format!("{path} cannot be read: {e}.")))?;
        let packed = entry.compressed_size();
        let mut data = Vec::new();
        (&mut entry)
            .take(limits.file + 1)
            .read_to_end(&mut data)
            .map_err(|e| bad_zip(format!("{path} cannot be unpacked: {e}.")))?;
        let size = data.len() as u64;
        if size > limits.file {
            return Err(too_large(format!("{path} is larger than the limit.")));
        }
        total += size;
        if total > limits.total {
            return Err(too_large(format!(
                "The files are larger than {} MiB together.",
                limits.total / (1024 * 1024)
            )));
        }
        if size > limits.ratio_from && size > packed.saturating_mul(limits.ratio) {
            return Err(too_large(format!(
                "{path} is packed too tightly, which a zip bomb does."
            )));
        }
        files.insert(path, data);
    }

    // Folders: no folder with the name of a file, and no two that differ only in case.
    let mut lower_folders: HashMap<String, &String> = HashMap::new();
    for folder in &folders {
        if files.contains_key(folder) {
            return Err(PackageError::new(
                "duplicate-name",
                format!("{folder} is both a file and a folder."),
            ));
        }
        let key = folder.to_ascii_lowercase();
        let other = lower_folders
            .get(&key)
            .copied()
            .or_else(|| lower.get(&key).filter(|p| files.contains_key(*p)));
        if let Some(other) = other {
            return Err(PackageError::new(
                "case-collision",
                format!("{folder} and {other} have the same name on Windows."),
            ));
        }
        lower_folders.insert(key, folder);
    }

    let manifest = check_content(&files)?;
    Ok(Package {
        manifest,
        files,
        sha256: sha256_hex(bytes),
    })
}

/// Reads the end records before the zip parser does. A package must not use zip64, and
/// each end record must name a small directory. This keeps the parser from allocating
/// for a huge entry count. Returns the entry count of the end record that ends the file.
fn check_end_records(bytes: &[u8], limits: &Limits) -> Result<Option<u16>, PackageError> {
    if bytes.len() < EOCD_LEN {
        return Err(bad_zip("The file is too small to be a zip."));
    }
    // The end record is at most 22 bytes plus a 65535-byte comment from the end.
    let start = bytes.len().saturating_sub(EOCD_LEN + 0xffff);
    let mut found = false;
    let mut listed = None;
    for at in start..=bytes.len() - EOCD_LEN {
        if bytes[at..at + 4] != EOCD {
            continue;
        }
        found = true;
        // A zip64 locator is 20 bytes long and comes just before the end record.
        if at >= 20 && bytes[at - 20..at - 16] == ZIP64_LOCATOR {
            return Err(unsupported(
                "The package uses zip64. Packages are small and never need it.",
            ));
        }
        let r = &bytes[at..at + EOCD_LEN];
        let u16_at = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
        let entries = u16_at(10).max(u16_at(8));
        if at + EOCD_LEN + usize::from(u16_at(20)) == bytes.len() {
            listed = Some(u16_at(10));
        }
        let directory = u32::from_le_bytes([r[12], r[13], r[14], r[15]]);
        if usize::from(entries) > limits.entries || u64::from(directory) > MAX_DIRECTORY {
            return Err(too_large(format!(
                "The package has more than {} files.",
                limits.entries
            )));
        }
    }
    if !found {
        return Err(bad_zip("The file is not a zip."));
    }
    Ok(listed)
}

fn too_large(message: impl Into<String>) -> PackageError {
    PackageError::new("package-too-large", message)
}

fn bad_zip(message: impl Into<String>) -> PackageError {
    PackageError::new("not-a-package", message)
}

fn unsafe_name(message: impl Into<String>) -> PackageError {
    PackageError::new("unsafe-name", message)
}

fn unsupported(message: impl Into<String>) -> PackageError {
    PackageError::new("unsupported-entry", message)
}
