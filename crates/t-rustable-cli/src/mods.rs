//! Finds the mods that a command needs. The command line uses the built-in mods and the
//! installed mods, the same as the app. It never loads developer folders, and it only
//! reads the mod settings of the app (`config/mods.json`).

use std::collections::BTreeSet;
use std::path::PathBuf;

use t_rustable_core::paths::{Folders, Resources};
use t_rustable_plugin_host::install::ModStore;
use t_rustable_sdk::manifest::{Manifest, World};

use crate::report::Problem;
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
