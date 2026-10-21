//! Installed mods (ADR 0019): `<dir>/<id>/` for each mod, and `<dir>/index.json` with
//! the SHA-256, version, and permissions of each package. The GUI and the CLI use it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use t_rustable_sdk::manifest::Manifest;
use t_rustable_sdk::permissions::Permission;

use crate::HostError;

const INDEX: &str = "index.json";
const STAGING: &str = ".staging";
const OLD: &str = ".old";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Index {
    mods: BTreeMap<String, IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    sha256: String,
    version: String,
    permissions: Vec<Permission>,
}

/// An installed mod.
#[derive(Debug, Clone)]
pub struct Installed {
    pub manifest: Manifest,
    pub sha256: String,
    pub dir: PathBuf,
}

/// A checked package, unpacked into the staging folder. It waits for the user's answer:
/// `ModStore::install` or `ModStore::discard`.
#[derive(Debug)]
pub struct Offer {
    pub manifest: Manifest,
    pub sha256: String,
    /// The installed version of the same mod, for an update.
    pub previous: Option<Installed>,
    /// Permissions that the installed version does not have. All of them for a new mod.
    pub new_permissions: Vec<Permission>,
    staging: PathBuf,
}

pub struct ModStore {
    dir: PathBuf,
}

fn error(code: &'static str, message: impl Into<String>) -> HostError {
    HostError {
        code,
        message: message.into(),
        changes: Vec::new(),
    }
}

fn io(context: &Path, e: std::io::Error) -> HostError {
    error("io-error", format!("{}: {e}", context.display()))
}

impl ModStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Deletes what an install that stopped halfway left behind.
    pub fn clean(&self) {
        for name in [STAGING, OLD] {
            let _ = std::fs::remove_dir_all(self.dir.join(name));
        }
    }

    /// The installed mods. A folder whose manifest cannot be read is skipped.
    pub fn list(&self) -> Vec<Installed> {
        let index = self.read_index();
        index
            .mods
            .iter()
            .filter_map(|(id, entry)| {
                let dir = self.dir.join(id);
                let json = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
                let manifest = Manifest::parse(&json).ok()?;
                (manifest.id == *id).then(|| Installed {
                    manifest,
                    sha256: entry.sha256.clone(),
                    dir,
                })
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Installed> {
        self.list().into_iter().find(|m| m.manifest.id == id)
    }

    /// Reads and checks a package, then unpacks it into the staging folder. `check`
    /// is the component check of the host (`ModHost::check_component`).
    pub fn stage(
        &self,
        bytes: &[u8],
        check: impl FnOnce(&Manifest, &[u8]) -> Result<(), HostError>,
    ) -> Result<Offer, HostError> {
        let package = t_rustable_tmod::read(bytes).map_err(|e| error(e.code, e.message))?;
        check(&package.manifest, package.component())?;
        let previous = self.get(&package.manifest.id);
        if let Some(p) = &previous
            && p.sha256 == package.sha256
        {
            return Err(error(
                "already-installed",
                format!(
                    "{} {} is already installed.",
                    package.manifest.name, package.manifest.version
                ),
            ));
        }
        let new_permissions = package
            .manifest
            .permissions
            .iter()
            .filter(|p| {
                previous
                    .as_ref()
                    .is_none_or(|old| !old.manifest.permissions.contains(p))
            })
            .copied()
            .collect();
        let staging = self.dir.join(STAGING).join(token());
        std::fs::create_dir_all(&staging).map_err(|e| io(&staging, e))?;
        if let Err(e) = package.extract(&staging) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(io(&staging, e));
        }
        Ok(Offer {
            manifest: package.manifest,
            sha256: package.sha256,
            previous,
            new_permissions,
            staging,
        })
    }

    /// Moves a staged package into place. An installed version is replaced.
    pub fn install(&self, offer: Offer) -> Result<Installed, HostError> {
        let id = offer.manifest.id.clone();
        let target = self.dir.join(&id);
        let old = self.dir.join(OLD).join(token());
        let had_old = target.exists();
        if had_old {
            std::fs::create_dir_all(old.parent().unwrap_or(&self.dir)).map_err(|e| io(&old, e))?;
            std::fs::rename(&target, &old).map_err(|e| io(&target, e))?;
        }
        if let Err(e) = std::fs::rename(&offer.staging, &target) {
            if had_old {
                let _ = std::fs::rename(&old, &target);
            }
            let _ = std::fs::remove_dir_all(&offer.staging);
            return Err(io(&target, e));
        }
        let mut index = self.read_index();
        index.mods.insert(
            id,
            IndexEntry {
                sha256: offer.sha256.clone(),
                version: offer.manifest.version.clone(),
                permissions: offer.manifest.permissions.clone(),
            },
        );
        self.write_index(&index)?;
        if had_old {
            let _ = std::fs::remove_dir_all(&old);
        }
        Ok(Installed {
            manifest: offer.manifest,
            sha256: offer.sha256,
            dir: target,
        })
    }

    /// The user said no. Deletes the staged files.
    pub fn discard(&self, offer: Offer) {
        let _ = std::fs::remove_dir_all(&offer.staging);
    }

    /// Removes an installed mod.
    pub fn remove(&self, id: &str) -> Result<(), HostError> {
        let mut index = self.read_index();
        if index.mods.remove(id).is_none() {
            return Err(error("not-installed", format!("{id} is not installed.")));
        }
        self.write_index(&index)?;
        let target = self.dir.join(id);
        let old = self.dir.join(OLD).join(token());
        // Rename first, so a folder that cannot be deleted at once does not load again.
        let _ = std::fs::create_dir_all(self.dir.join(OLD));
        match std::fs::rename(&target, &old) {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&old);
            }
            Err(_) => {
                let _ = std::fs::remove_dir_all(&target);
            }
        }
        Ok(())
    }

    fn read_index(&self) -> Index {
        std::fs::read_to_string(self.dir.join(INDEX))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn write_index(&self, index: &Index) -> Result<(), HostError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| io(&self.dir, e))?;
        let path = self.dir.join(INDEX);
        let tmp = self.dir.join(format!("{INDEX}.{}", token()));
        let json = serde_json::to_string_pretty(index).unwrap_or_default();
        std::fs::write(&tmp, json).map_err(|e| io(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| io(&path, e))
    }
}

/// A name for a temporary folder that no other install in this process or another uses.
fn token() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!(
        "{}-{nanos:x}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
