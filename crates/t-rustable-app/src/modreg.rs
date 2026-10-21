//! The mod registry (ADR 0019): which mods load, from where, and in which state.
//!
//! Sources, in priority order: built-in mods, developer folders (only in developer
//! mode), and installed mods. A mod ID loads from the first source that has it.
//! `config/mods.json` holds the mods that the user turned off and the developer folders.
//! All of this runs on the mod thread.

use crate::bridge::{ActionDto, GuiBridge, ModUi, PanelDto};
use crate::state::Shared;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use t_rustable_plugin_host::install::{ModStore, Offer};
use t_rustable_plugin_host::{HostError, ModHost};
use t_rustable_sdk::manifest::Manifest;

const CONFIG: &str = "mods.json";
const DEV_CACHE: &str = ".dev";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModsConfig {
    /// Mods that the user turned off.
    pub disabled: BTreeSet<String>,
    pub developer: bool,
    pub dev_folders: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Builtin,
    Developer,
    Installed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModState {
    On,
    /// Turned off by the user.
    Off,
    /// Stopped by the host after 3 failures in a row.
    Stopped,
    /// The mod did not load.
    Failed,
}

/// One row of the mod manager.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRow {
    /// The mod ID, or the folder for a developer folder without a valid manifest.
    pub key: String,
    pub id: Option<String>,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub source: Source,
    pub state: ModState,
    pub error: Option<String>,
    pub permissions: Vec<String>,
    pub folder: Option<String>,
}

/// A package that waits for the user's answer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferDto {
    pub token: u64,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub permissions: Vec<String>,
    /// Permissions that the installed version does not have.
    pub new_permissions: Vec<String>,
    pub previous_version: Option<String>,
    pub sha256: String,
}

/// Size and change time of the files that a developer folder build depends on.
type Signature = Vec<(PathBuf, u64, Option<SystemTime>)>;

struct Entry {
    source: Source,
    /// The source folder. For a developer folder, the author's folder.
    dir: PathBuf,
    manifest: Option<Manifest>,
    error: Option<String>,
    loaded: bool,
    /// The unpacked build of a developer folder that the host loaded.
    cache: Option<PathBuf>,
}

#[derive(Default)]
struct Watch {
    /// The signature of the loaded build.
    loaded: Signature,
    /// The signature at the last tick. A reload waits until two ticks agree.
    seen: Signature,
}

/// What `sync` wants loaded.
struct Want {
    key: String,
    source: Source,
    dir: PathBuf,
    manifest: Option<Manifest>,
    error: Option<String>,
}

pub struct Mods {
    pub host: ModHost,
    store: ModStore,
    config_file: PathBuf,
    pub config: ModsConfig,
    builtin: PathBuf,
    tests: bool,
    entries: BTreeMap<String, Entry>,
    watches: HashMap<PathBuf, Watch>,
    offers: HashMap<u64, Offer>,
    next_offer: u64,
    next_build: u64,
    shared: Arc<Shared>,
    bridge: Arc<GuiBridge>,
}

fn host_error(e: HostError) -> String {
    e.message
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let json = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|e| format!("manifest.json cannot be read: {e}."))?;
    Manifest::parse(&json).map_err(|problems| {
        let text: Vec<_> = problems.into_iter().map(|p| p.0).collect();
        text.join(" ")
    })
}

fn mod_ui(manifest: &Manifest, dir: PathBuf) -> ModUi {
    ModUi {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        disabled: false,
        actions: manifest
            .actions
            .iter()
            .map(|a| ActionDto {
                id: a.id.clone(),
                title: a.title.clone(),
                description: a.description.clone(),
                tool: a.tool,
                shortcut: a.shortcut.clone(),
            })
            .collect(),
        toolbar: Vec::new(),
        panels: manifest
            .panels
            .iter()
            .map(|p| PanelDto {
                id: p.id.clone(),
                title: p.title.clone(),
                entry: p.entry.clone(),
            })
            .collect(),
        dir,
        developer: false,
    }
}

/// The files that a developer folder build reads: the manifest, the component or the
/// cargo output, and the panel files.
fn signature(dir: &Path) -> Signature {
    let mut files = vec![dir.join("manifest.json")];
    if let Ok(m) = read_manifest(dir) {
        files.push(dir.join(&m.entry));
        files.push(t_rustable_tmod::build::cargo_output(dir, &m.entry));
        for p in &m.panels {
            match p.entry.rsplit_once('/') {
                Some((folder, _)) => walk(&dir.join(folder), 0, &mut files),
                None => files.push(dir.join(&p.entry)),
            }
        }
    }
    files
        .into_iter()
        .map(|f| {
            let meta = std::fs::metadata(&f).ok();
            let len = meta.as_ref().map_or(0, |m| m.len());
            let time = meta.and_then(|m| m.modified().ok());
            (f, len, time)
        })
        .collect()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            if depth < t_rustable_tmod::names::MAX_DEPTH {
                walk(&p, depth + 1, out);
            }
        } else {
            out.push(p);
        }
    }
}

impl Mods {
    pub fn start(
        host: ModHost,
        shared: Arc<Shared>,
        bridge: Arc<GuiBridge>,
        builtin: PathBuf,
        tests: bool,
    ) -> Self {
        let store = ModStore::new(&shared.folders.mods);
        store.clean();
        let _ = std::fs::remove_dir_all(shared.folders.mods.join(DEV_CACHE));
        let config_file = shared.folders.config.join(CONFIG);
        let config = std::fs::read(&config_file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut mods = Self {
            host,
            store,
            config_file,
            config,
            builtin,
            tests,
            entries: BTreeMap::new(),
            watches: HashMap::new(),
            offers: HashMap::new(),
            next_offer: 1,
            next_build: 1,
            shared,
            bridge,
        };
        mods.sync();
        mods
    }

    fn log(&self, level: &str, message: &str) {
        self.shared.log.write(level, "mods", message);
    }

    fn save_config(&self) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&self.config).map_err(|e| e.to_string())?;
        let tmp = self.config_file.with_extension("json.tmp");
        std::fs::write(&tmp, json)
            .and_then(|()| std::fs::rename(&tmp, &self.config_file))
            .map_err(|e| format!("{}: {e}", self.config_file.display()))
    }

    pub fn builtin_dir(&self) -> &Path {
        &self.builtin
    }

    /// IDs of loaded mods that the host stopped after errors.
    pub fn stopped(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(id, e)| e.loaded && self.host.is_disabled(id))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// What each source offers, in priority order, without duplicate IDs.
    fn wanted(&self) -> Vec<Want> {
        let mut out: Vec<Want> = Vec::new();
        let mut taken = BTreeSet::new();
        if let Ok(entries) = std::fs::read_dir(&self.builtin) {
            let mut dirs: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
            dirs.sort();
            for dir in dirs {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let wanted =
                    name.starts_with("t-rustable.") || (self.tests && name.starts_with("test."));
                if !wanted || !dir.join("manifest.json").is_file() {
                    continue;
                }
                let (manifest, error) = match read_manifest(&dir) {
                    Ok(m) => (Some(m), None),
                    Err(e) => (None, Some(e)),
                };
                taken.insert(name.clone());
                out.push(Want {
                    key: name,
                    source: Source::Builtin,
                    dir,
                    manifest,
                    error,
                });
            }
        }
        if self.config.developer {
            for dir in &self.config.dev_folders {
                let want = match read_manifest(dir) {
                    Ok(m) if taken.contains(&m.id) => Want {
                        key: dir.display().to_string(),
                        source: Source::Developer,
                        dir: dir.clone(),
                        error: Some(format!("Another mod with the ID {} is loaded.", m.id)),
                        manifest: Some(m),
                    },
                    Ok(m) => Want {
                        key: m.id.clone(),
                        source: Source::Developer,
                        dir: dir.clone(),
                        manifest: Some(m),
                        error: None,
                    },
                    Err(e) => Want {
                        key: dir.display().to_string(),
                        source: Source::Developer,
                        dir: dir.clone(),
                        manifest: None,
                        error: Some(e),
                    },
                };
                taken.insert(want.key.clone());
                out.push(want);
            }
        }
        for installed in self.store.list() {
            // A developer folder with the same ID replaces the installed mod.
            if taken.insert(installed.manifest.id.clone()) {
                out.push(Want {
                    key: installed.manifest.id.clone(),
                    source: Source::Installed,
                    dir: installed.dir,
                    manifest: Some(installed.manifest),
                    error: None,
                });
            }
        }
        out
    }

    /// Loads and unloads mods until the loaded set matches the sources and the
    /// configuration. Mods that did not change stay loaded.
    pub fn sync(&mut self) {
        let wanted = self.wanted();
        let keys: BTreeSet<&String> = wanted.iter().map(|w| &w.key).collect();
        let gone: Vec<String> = self
            .entries
            .iter()
            .filter(|(key, e)| {
                !keys.contains(key)
                    || wanted
                        .iter()
                        .any(|w| &w.key == *key && (w.source != e.source || w.dir != e.dir))
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in gone {
            self.take_down(&key, false);
            self.entries.remove(&key);
        }
        self.watches
            .retain(|dir, _| self.config.developer && self.config.dev_folders.contains(dir));
        for want in wanted {
            let off = self.config.disabled.contains(&want.key);
            match self.entries.get(&want.key) {
                Some(e) if e.loaded && !off => continue,
                Some(e) if e.loaded => self.take_down(&want.key, false),
                _ => {}
            }
            let mut entry = Entry {
                source: want.source,
                dir: want.dir,
                manifest: want.manifest,
                error: want.error,
                loaded: false,
                cache: None,
            };
            if !off && entry.error.is_none() {
                let built = match entry.source {
                    Source::Developer => self.build_dev(&entry.dir).inspect(|c| {
                        entry.cache = Some(c.clone());
                    }),
                    _ => Ok(entry.dir.clone()),
                };
                match built.and_then(|dir| self.load(entry.source, dir)) {
                    Ok(manifest) => {
                        entry.manifest = Some(manifest);
                        entry.loaded = true;
                    }
                    Err(e) => {
                        self.log("error", &format!("{}: {e}", entry.dir.display()));
                        entry.error = Some(e);
                    }
                }
            }
            if entry.source == Source::Developer {
                let sig = signature(&entry.dir);
                self.watches.insert(
                    entry.dir.clone(),
                    Watch {
                        loaded: sig.clone(),
                        seen: sig,
                    },
                );
            }
            self.entries.insert(want.key, entry);
        }
        self.bridge.registry_changed();
    }

    /// Loads one unpacked mod into the host and the UI registry.
    fn load(&mut self, source: Source, dir: PathBuf) -> Result<Manifest, String> {
        let id = self.host.load_dir(&dir).map_err(host_error)?;
        let manifest = self
            .host
            .manifests()
            .find(|m| m.id == id)
            .cloned()
            .ok_or("The mod did not load.")?;
        let mut ui = mod_ui(&manifest, dir);
        ui.developer = source == Source::Developer;
        let mut registry = self.bridge.mods.lock().unwrap();
        // Keep what the mod registered while it loaded.
        if let Some(old) = registry.remove(&id) {
            ui.toolbar = old.toolbar;
        }
        registry.insert(id, ui);
        Ok(manifest)
    }

    /// Packs a developer folder with the install rules and unpacks it into a new cache
    /// folder.
    fn build_dev(&mut self, dir: &Path) -> Result<PathBuf, String> {
        let (package, _) = t_rustable_tmod::build::pack(dir, None)
            .map_err(|e| format!("{} ({})", e.message, e.code))?;
        let cache = self
            .shared
            .folders
            .mods
            .join(DEV_CACHE)
            .join(&package.manifest.id)
            .join(self.next_build.to_string());
        self.next_build += 1;
        package
            .extract(&cache)
            .map_err(|e| format!("{}: {e}", cache.display()))?;
        Ok(cache)
    }

    /// Unloads a mod: no active tool, no panels (unless it loads again at once), no
    /// registry entry.
    fn take_down(&mut self, id: &str, keep_panels: bool) {
        let active = self.shared.active_tool.lock().unwrap().clone();
        if active.as_deref() == Some(id) {
            let _ = self.host.deactivate(id);
            *self.shared.active_tool.lock().unwrap() = None;
            self.bridge.clear_overlay_of(id);
        }
        if !keep_panels {
            self.shared
                .open_panels
                .lock()
                .unwrap()
                .retain(|(m, _)| m != id);
        }
        self.host.unload(id);
        self.bridge.mods.lock().unwrap().remove(id);
        if let Some(e) = self.entries.get_mut(id) {
            e.loaded = false;
        }
    }

    pub fn rows(&self) -> Vec<ModRow> {
        self.entries
            .iter()
            .map(|(key, e)| {
                let m = e.manifest.as_ref();
                let state = if e.loaded && self.host.is_disabled(key) {
                    ModState::Stopped
                } else if e.loaded {
                    ModState::On
                } else if self.config.disabled.contains(key) {
                    ModState::Off
                } else {
                    ModState::Failed
                };
                ModRow {
                    key: key.clone(),
                    id: m.map(|m| m.id.clone()),
                    name: m.map_or_else(|| key.clone(), |m| m.name.clone()),
                    version: m.map(|m| m.version.clone()).unwrap_or_default(),
                    author: m.map(|m| m.author.clone()).unwrap_or_default(),
                    description: m.map(|m| m.description.clone()).unwrap_or_default(),
                    source: e.source,
                    state,
                    error: e.error.clone(),
                    permissions: m
                        .map(|m| m.permissions.iter().map(ToString::to_string).collect())
                        .unwrap_or_default(),
                    folder: (e.source == Source::Developer).then(|| e.dir.display().to_string()),
                }
            })
            .collect()
    }

    /// Checks a package and keeps it until the user answers.
    pub fn stage(&mut self, bytes: &[u8]) -> Result<OfferDto, HostError> {
        let host = &self.host;
        let offer = self.store.stage(bytes, |m, c| host.check_component(m, c))?;
        let token = self.next_offer;
        self.next_offer += 1;
        let names = |p: &[t_rustable_sdk::permissions::Permission]| {
            p.iter().map(ToString::to_string).collect::<Vec<_>>()
        };
        let m = &offer.manifest;
        let dto = OfferDto {
            token,
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            author: m.author.clone(),
            description: m.description.clone(),
            permissions: names(&m.permissions),
            new_permissions: names(&offer.new_permissions),
            previous_version: offer.previous.as_ref().map(|p| p.manifest.version.clone()),
            sha256: offer.sha256.clone(),
        };
        self.offers.insert(token, offer);
        Ok(dto)
    }

    /// Installs or discards a staged package. Returns the mod ID after an install.
    pub fn answer(&mut self, token: u64, install: bool) -> Result<Option<String>, HostError> {
        let Some(offer) = self.offers.remove(&token) else {
            return Ok(None);
        };
        if !install {
            self.store.discard(offer);
            return Ok(None);
        }
        let id = offer.manifest.id.clone();
        // Unload first, so that nothing uses the folder that the update replaces.
        let installed_loaded = self
            .entries
            .get(&id)
            .is_some_and(|e| e.loaded && e.source == Source::Installed);
        if installed_loaded {
            self.take_down(&id, true);
        }
        self.store.install(offer)?;
        if installed_loaded {
            self.entries.remove(&id);
        }
        self.sync();
        Ok(Some(id))
    }

    /// Turns a mod on or off. On also starts a mod that the host stopped after errors.
    pub fn set_on(&mut self, key: &str, on: bool) -> Result<(), String> {
        if on && self.host.is_disabled(key) {
            self.host.enable(key);
            self.bridge.registry_changed();
            return Ok(());
        }
        let changed = if on {
            self.config.disabled.remove(key)
        } else {
            self.config.disabled.insert(key.to_string())
        };
        if changed {
            self.save_config()?;
            if !on {
                self.take_down(key, false);
            } else if let Some(e) = self.entries.get_mut(key) {
                e.error = None;
            }
            self.sync();
        }
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<(), HostError> {
        let installed = self
            .entries
            .get(id)
            .is_some_and(|e| e.source == Source::Installed);
        if installed {
            self.take_down(id, false);
            self.entries.remove(id);
        }
        self.store.remove(id)?;
        if self.config.disabled.remove(id) {
            let _ = self.save_config();
        }
        self.sync();
        Ok(())
    }

    pub fn set_developer(&mut self, on: bool) -> Result<(), String> {
        self.config.developer = on;
        self.save_config()?;
        self.sync();
        Ok(())
    }

    pub fn add_dev_folder(&mut self, dir: PathBuf) -> Result<(), String> {
        if !self.config.dev_folders.contains(&dir) {
            self.config.dev_folders.push(dir);
            self.config.developer = true;
            self.save_config()?;
        }
        self.sync();
        Ok(())
    }

    pub fn remove_dev_folder(&mut self, dir: &Path) -> Result<(), String> {
        self.config.dev_folders.retain(|d| d != dir);
        self.save_config()?;
        self.sync();
        Ok(())
    }

    /// Loads a developer folder again. On an error, the old build stays loaded.
    pub fn reload(&mut self, key: &str) -> Result<(), String> {
        let Some(entry) = self.entries.get(key) else {
            return Err("The mod is not in the list.".into());
        };
        if entry.source != Source::Developer {
            return Err("Only developer folders load again.".into());
        }
        let (dir, loaded, old) = (entry.dir.clone(), entry.loaded, entry.cache.clone());
        let sig = signature(&dir);
        if let Some(w) = self.watches.get_mut(&dir) {
            w.loaded = sig.clone();
            w.seen = sig;
        }
        let same_id = read_manifest(&dir).is_ok_and(|m| m.id == key);
        if !same_id || !loaded {
            // The ID changed, or the old build did not load: start again from the sources.
            self.take_down(key, false);
            self.entries.remove(key);
            self.sync();
            return e_result(self.entries.values().find(|e| e.dir == dir));
        }
        let new = self.build_dev(&dir)?;
        self.take_down(key, true);
        let result = self.load(Source::Developer, new.clone());
        let (keep, stale) = match &result {
            Ok(_) => (Some(new), old),
            Err(_) => (old, Some(new)),
        };
        if let Some(d) = stale {
            let _ = std::fs::remove_dir_all(d);
        }
        let back = match (&result, &keep) {
            (Ok(m), _) => Some(m.clone()),
            (Err(_), Some(old)) => self.load(Source::Developer, old.clone()).ok(),
            (Err(_), None) => None,
        };
        let e = self.entries.get_mut(key).expect("entry");
        e.cache = keep;
        e.loaded = back.is_some();
        if let Some(m) = back {
            e.manifest = Some(m);
        }
        e.error = result.as_ref().err().cloned();
        if !e.loaded {
            self.shared
                .open_panels
                .lock()
                .unwrap()
                .retain(|(m, _)| m != key);
        }
        self.bridge.registry_changed();
        self.bridge.mod_reloaded(key);
        result.map(drop)
    }

    /// One tick of the developer folder watch. A folder loads again when its files
    /// changed and then stayed the same for one tick.
    pub fn poll(&mut self) {
        if !self.config.developer {
            return;
        }
        let mut due = Vec::new();
        for (dir, w) in &mut self.watches {
            let sig = signature(dir);
            if sig != w.seen {
                w.seen = sig;
            } else if sig != w.loaded {
                due.push(dir.clone());
            }
        }
        for dir in due {
            let key = self
                .entries
                .iter()
                .find(|(_, e)| e.dir == dir && e.source == Source::Developer)
                .map(|(k, _)| k.clone());
            let Some(key) = key else { continue };
            let result = self.reload(&key);
            let (level, message) = match &result {
                Ok(()) => ("info", format!("Loaded {} again.", dir.display())),
                Err(e) => ("error", format!("{} did not load: {e}", dir.display())),
            };
            self.log(level, &message);
            self.bridge.app_notice(level, &message);
        }
    }
}

fn e_result(entry: Option<&Entry>) -> Result<(), String> {
    match entry.and_then(|e| e.error.clone()) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_or_partial_config_file_still_reads() {
        let c: ModsConfig = serde_json::from_str(r#"{"disabled": ["a.b"]}"#).unwrap();
        assert!(c.disabled.contains("a.b") && !c.developer && c.dev_folders.is_empty());
    }

    #[test]
    fn the_signature_sees_the_manifest_and_panel_files() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = r#"{"id": "com.example.sig", "name": "Sig", "version": "0.1.0",
            "apiVersion": "0.1", "author": "A", "description": "D",
            "permissions": ["ui"], "entry": "sig.wasm", "worlds": ["interactive"],
            "panels": [{"id": "p", "title": "P", "entry": "panel/index.html"}]}"#;
        std::fs::write(dir.path().join("manifest.json"), manifest).unwrap();
        std::fs::create_dir(dir.path().join("panel")).unwrap();
        std::fs::write(dir.path().join("panel/index.html"), "a").unwrap();
        let before = signature(dir.path());
        assert!(
            before
                .iter()
                .any(|(p, _, _)| p.ends_with("panel/index.html"))
        );
        std::fs::write(dir.path().join("panel/index.html"), "ab").unwrap();
        assert_ne!(before, signature(dir.path()));
    }
}
