//! Loads mods and runs their exports with limits (mod system rules 3 and 4).

use crate::bindings::v0_1::generated::Full;
use crate::bindings::v0_1::generated::exports::t_rustable::api::{batch_job, tool};
use crate::bindings::v0_1::generated::t_rustable::api::types::ApiError;
use crate::state::{
    Call, CallWorld, DocRes, HostState, MemoryLimit, Services, SessionRef, TimeLimit, Timing, TxRes,
};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use t_rustable_core::Change;
use t_rustable_sdk::manifest::{Manifest, World};
use t_rustable_sdk::permissions::check_imports;
use wasmtime::component::{Component, HasSelf, InstancePre, Linker, Resource, ResourceTable};
use wasmtime::{CallHook, Config, Engine, Store, UpdateDeadline};

/// One epoch tick. The time limit is checked at this interval.
const TICK: Duration = Duration::from_millis(10);
/// After this many failures in a row, the host disables the mod.
pub const MAX_FAILURES: u32 = 3;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Time in the mod's own code per interactive call.
    pub interactive_time: Duration,
    /// Time in the mod's own code per batch call.
    pub batch_time: Duration,
    /// Linear memory per mod instance, in bytes.
    pub memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            interactive_time: Duration::from_secs(5),
            batch_time: Duration::from_secs(600),
            memory: 512 << 20,
        }
    }
}

/// A failure of the host or of the mod, not an `api-error` that the mod returned.
#[derive(Clone, Debug, PartialEq)]
pub struct HostError {
    /// A code from `docs/errors.md`, for example `timeout` or `mod-disabled`.
    pub code: &'static str,
    pub message: String,
    /// Commits that the mod made before it failed. They stay in the document.
    pub changes: Vec<Change>,
}

impl HostError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            changes: Vec::new(),
        }
    }
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for HostError {}

/// The result of an export call, and the document changes that it committed.
#[derive(Debug)]
pub struct Outcome<T> {
    pub value: T,
    pub changes: Vec<Change>,
}

struct Ticker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Exports {
    tool: Option<tool::Guest>,
    batch: Option<batch_job::Guest>,
}

struct LoadedMod {
    manifest: Manifest,
    dir: PathBuf,
    pre: InstancePre<HostState>,
    store: Store<HostState>,
    exports: Exports,
    failures: u32,
    disabled: bool,
}

pub struct ModHost {
    engine: Engine,
    linker: Linker<HostState>,
    services: Services,
    limits: Limits,
    mods: BTreeMap<String, LoadedMod>,
    _ticker: Ticker,
}

fn ticks(d: Duration) -> u64 {
    (d.as_millis() / TICK.as_millis()) as u64 + 1
}

fn invalid(message: impl Into<String>) -> HostError {
    HostError::new("invalid-mod", message)
}

fn has_export(component: &Component, engine: &Engine, interface: &str) -> bool {
    let prefix = format!("t-rustable:api/{interface}@0.1.");
    component
        .component_type()
        .exports(engine)
        .any(|(name, _)| name.starts_with(&prefix))
}

impl ModHost {
    pub fn new(services: Services, limits: Limits) -> Result<Self, HostError> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| HostError::new("internal", e.to_string()))?;
        let mut linker = Linker::new(&engine);
        Full::add_to_linker::<HostState, HasSelf<HostState>>(&mut linker, |s| s)
            .map_err(|e| HostError::new("internal", e.to_string()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (engine, stop) = (engine.clone(), stop.clone());
            std::thread::Builder::new()
                .name("t-rustable-mod-epoch".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(TICK);
                        engine.increment_epoch();
                    }
                })
                .map_err(|e| HostError::new("internal", e.to_string()))?
        };
        Ok(Self {
            engine,
            linker,
            services,
            limits,
            mods: BTreeMap::new(),
            _ticker: Ticker {
                stop,
                thread: Some(thread),
            },
        })
    }

    /// Loads an unpacked mod folder: `manifest.json` and the component that it names.
    /// Returns the mod ID.
    pub fn load_dir(&mut self, dir: &Path) -> Result<String, HostError> {
        let json = std::fs::read_to_string(dir.join("manifest.json"))
            .map_err(|e| invalid(format!("manifest.json cannot be read: {e}")))?;
        let manifest = Manifest::parse(&json).map_err(|problems| {
            invalid(
                problems
                    .iter()
                    .map(|p| p.0.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        })?;
        if self.mods.contains_key(&manifest.id) {
            return Err(invalid(format!(
                "A mod with the ID {} is already loaded.",
                manifest.id
            )));
        }
        let bytes = std::fs::read(dir.join(&manifest.entry))
            .map_err(|e| invalid(format!("{} cannot be read: {e}", manifest.entry)))?;
        let pre = self.prepare(&manifest, &bytes)?;
        let (store, exports) = self.instantiate(&manifest, &pre)?;
        let id = manifest.id.clone();
        self.mods.insert(
            id.clone(),
            LoadedMod {
                manifest,
                dir: dir.to_path_buf(),
                pre,
                store,
                exports,
                failures: 0,
                disabled: false,
            },
        );
        Ok(id)
    }

    /// Checks a component against its manifest without running any of its code: the
    /// imports match the permissions, this host has each imported function, and the
    /// manifest worlds are exported. The install uses it before it writes any file.
    pub fn check_component(&self, manifest: &Manifest, bytes: &[u8]) -> Result<(), HostError> {
        self.prepare(manifest, bytes).map(drop)
    }

    fn prepare(
        &self,
        manifest: &Manifest,
        bytes: &[u8],
    ) -> Result<InstancePre<HostState>, HostError> {
        let component = Component::new(&self.engine, bytes)
            .map_err(|e| invalid(format!("{} is not a valid component: {e}", manifest.entry)))?;

        // Permissions come from imports, before instantiation (ADR 0007).
        let imports: Vec<String> = component
            .component_type()
            .imports(&self.engine)
            .map(|(name, _)| name.to_string())
            .collect();
        let problems = check_imports(imports.iter().map(String::as_str), &manifest.permissions);
        if !problems.is_empty() {
            return Err(invalid(
                problems
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            ));
        }
        // Linking fails when the mod imports a function that this host does not have,
        // for example from a newer API version.
        let pre = self.linker.instantiate_pre(&component).map_err(|e| {
            invalid(format!(
                "The mod needs a function that this version of T-Rustable does not have: {e}"
            ))
        })?;
        for (world, interface) in [(World::Interactive, "tool"), (World::Batch, "batch-job")] {
            if manifest.worlds.contains(&world) && !has_export(&component, &self.engine, interface)
            {
                return Err(invalid(format!(
                    "The manifest lists the {world:?} world, but the mod does not export {interface}."
                )));
            }
        }
        Ok(pre)
    }

    fn instantiate(
        &self,
        manifest: &Manifest,
        pre: &InstancePre<HostState>,
    ) -> Result<(Store<HostState>, Exports), HostError> {
        let state = HostState {
            mod_id: manifest.id.clone(),
            services: self.services.clone(),
            table: ResourceTable::new(),
            memory_limit: self.limits.memory,
            memory_exceeded: false,
            generation: 0,
            call: None,
            timing: Timing::new(self.limits.interactive_time),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| s);
        store.call_hook(|mut ctx, hook| {
            let t = &mut ctx.data_mut().timing;
            match hook {
                CallHook::CallingHost => t.in_host_since = Some(std::time::Instant::now()),
                CallHook::ReturningFromHost => {
                    if let Some(since) = t.in_host_since.take() {
                        t.host += since.elapsed();
                    }
                }
                _ => {}
            }
            Ok(())
        });
        store.epoch_deadline_callback(|ctx| {
            let t = &ctx.data().timing;
            let guest = t.guest();
            if guest >= t.limit {
                return Err(wasmtime::Error::new(TimeLimit));
            }
            Ok(UpdateDeadline::Continue(ticks(t.limit - guest)))
        });
        store.set_epoch_deadline(ticks(self.limits.interactive_time));
        let instance = pre
            .instantiate(&mut store)
            .map_err(|e| invalid(format!("The mod cannot start: {e}")))?;
        let load = |world: World| manifest.worlds.contains(&world);
        let tool = if load(World::Interactive) {
            let guest = tool::GuestIndices::new(pre)
                .and_then(|i| i.load(&mut store, &instance))
                .map_err(|e| invalid(e.to_string()))?;
            Some(guest)
        } else {
            None
        };
        let batch = if load(World::Batch) {
            let guest = batch_job::GuestIndices::new(pre)
                .and_then(|i| i.load(&mut store, &instance))
                .map_err(|e| invalid(e.to_string()))?;
            Some(guest)
        } else {
            None
        };
        Ok((store, Exports { tool, batch }))
    }

    pub fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.mods.values().map(|m| &m.manifest)
    }

    pub fn mod_dir(&self, id: &str) -> Option<&Path> {
        self.mods.get(id).map(|m| m.dir.as_path())
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.mods.get(id).is_some_and(|m| m.disabled)
    }

    /// The user turns a disabled mod on again.
    pub fn enable(&mut self, id: &str) {
        if let Some(m) = self.mods.get_mut(id) {
            m.disabled = false;
            m.failures = 0;
        }
    }

    pub fn unload(&mut self, id: &str) -> bool {
        self.mods.remove(id).is_some()
    }

    /// Runs one export call with a new time budget. A trap, a timeout, or the memory limit
    /// counts as a failure and gives the mod a new instance.
    fn call<R>(
        &mut self,
        id: &str,
        world: CallWorld,
        doc: Option<&SessionRef>,
        cancel: Arc<AtomicBool>,
        run: impl FnOnce(
            &Exports,
            &mut Store<HostState>,
            Option<Resource<DocRes>>,
        ) -> Result<wasmtime::Result<R>, HostError>,
    ) -> Result<Outcome<R>, HostError> {
        let limit = match world {
            CallWorld::Interactive => self.limits.interactive_time,
            CallWorld::Batch => self.limits.batch_time,
        };
        let m = self
            .mods
            .get_mut(id)
            .ok_or_else(|| HostError::new("not-found", format!("No mod {id} is loaded.")))?;
        if m.disabled {
            return Err(HostError::new(
                "mod-disabled",
                format!("The mod {id} is disabled after {MAX_FAILURES} failures."),
            ));
        }
        let state = m.store.data_mut();
        state.generation += 1;
        state.memory_exceeded = false;
        state.timing = Timing::new(limit);
        state.call = Some(Call {
            world,
            cancel,
            changes: Vec::new(),
            transactions: Vec::new(),
        });
        let doc_res = match doc {
            Some(s) => Some(
                state
                    .table
                    .push(DocRes { session: s.clone() })
                    .map_err(|e| HostError::new("internal", e.to_string()))?,
            ),
            None => None,
        };
        let doc_rep = doc_res.as_ref().map(|r| r.rep());
        m.store.set_epoch_deadline(ticks(limit));

        let result = catch_unwind(AssertUnwindSafe(|| run(&m.exports, &mut m.store, doc_res)));

        // The call ends: the document goes back, and open transactions expire.
        let state = m.store.data_mut();
        if let Some(rep) = doc_rep {
            let _ = state.table.delete(Resource::<DocRes>::new_own(rep));
        }
        let (changes, transactions) = state
            .call
            .take()
            .map(|c| (c.changes, c.transactions))
            .unwrap_or_default();
        for rep in transactions {
            if let Ok(tx) = state.table.get_mut(&Resource::<TxRes>::new_own(rep)) {
                tx.tx = None;
            }
        }
        let memory_exceeded = state.memory_exceeded;

        let failure = match result {
            Ok(Ok(Ok(value))) => {
                m.failures = 0;
                return Ok(Outcome { value, changes });
            }
            Ok(Err(e)) => return Err(HostError { changes, ..e }),
            Ok(Ok(Err(trap))) => {
                if trap.is::<TimeLimit>() {
                    HostError::new(
                        "timeout",
                        format!(
                            "The mod used more than {} ms of its own time.",
                            limit.as_millis()
                        ),
                    )
                } else if trap.is::<MemoryLimit>() || memory_exceeded {
                    HostError::new(
                        "out-of-memory",
                        format!(
                            "The mod used more than {} MB of memory.",
                            self.limits.memory >> 20
                        ),
                    )
                } else {
                    HostError::new(
                        "mod-failed",
                        format!("The mod stopped because of an error in its code: {trap}"),
                    )
                }
            }
            // The panic hook of the front end writes the panic message.
            Err(_) => HostError::new(
                "internal",
                "The host failed during the mod call. The log has the details.",
            ),
        };
        log::warn!(target: id, "The call failed: {failure}");
        m.failures += 1;
        if m.failures >= MAX_FAILURES {
            m.disabled = true;
            log::warn!(target: id, "The mod is disabled after {MAX_FAILURES} failures in a row.");
        }
        let manifest = m.manifest.clone();
        let pre = m.pre.clone();
        match self.instantiate(&manifest, &pre) {
            Ok((store, exports)) => {
                let m = self.mods.get_mut(id).unwrap();
                m.store = store;
                m.exports = exports;
            }
            Err(e) => {
                log::error!(target: id, "The mod did not start again, so it is disabled: {e}");
                self.mods.get_mut(id).unwrap().disabled = true;
            }
        }
        Err(HostError { changes, ..failure })
    }

    fn tool<'a>(e: &'a Exports, id: &str) -> Result<&'a tool::Guest, HostError> {
        e.tool.as_ref().ok_or_else(|| {
            HostError::new(
                "no-export",
                format!("The mod {id} has no interactive tool."),
            )
        })
    }

    fn no_cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    pub fn activate(&mut self, id: &str) -> Result<Outcome<Result<(), ApiError>>, HostError> {
        let owner = id.to_string();
        self.call(
            id,
            CallWorld::Interactive,
            None,
            Self::no_cancel(),
            |e, s, _| Ok(Self::tool(e, &owner)?.call_activate(s)),
        )
    }

    pub fn deactivate(&mut self, id: &str) -> Result<Outcome<()>, HostError> {
        let owner = id.to_string();
        self.call(
            id,
            CallWorld::Interactive,
            None,
            Self::no_cancel(),
            |e, s, _| Ok(Self::tool(e, &owner)?.call_deactivate(s)),
        )
    }

    pub fn run_action(
        &mut self,
        id: &str,
        doc: Option<&SessionRef>,
        action_id: &str,
        args: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<Outcome<Result<(), ApiError>>, HostError> {
        let owner = id.to_string();
        self.call(id, CallWorld::Interactive, doc, cancel, |e, s, d| {
            Ok(Self::tool(e, &owner)?.call_run_action(s, d, action_id, args))
        })
    }

    pub fn on_pointer(
        &mut self,
        id: &str,
        doc: &SessionRef,
        event: tool::PointerEvent,
    ) -> Result<Outcome<tool::PointerResult>, HostError> {
        let owner = id.to_string();
        self.call(
            id,
            CallWorld::Interactive,
            Some(doc),
            Self::no_cancel(),
            |e, s, d| Ok(Self::tool(e, &owner)?.call_on_pointer(s, d.unwrap(), event)),
        )
        .map(|mut out| {
            let shapes = std::mem::take(&mut out.value.overlay);
            out.value.overlay = crate::imports::labelled(id, shapes);
            out
        })
    }

    pub fn on_key(
        &mut self,
        id: &str,
        doc: &SessionRef,
        event: tool::KeyEvent,
    ) -> Result<Outcome<tool::KeyResult>, HostError> {
        let owner = id.to_string();
        self.call(
            id,
            CallWorld::Interactive,
            Some(doc),
            Self::no_cancel(),
            |e, s, d| Ok(Self::tool(e, &owner)?.call_on_key(s, d.unwrap(), &event)),
        )
    }

    pub fn on_text_input(
        &mut self,
        id: &str,
        doc: &SessionRef,
        text: &str,
    ) -> Result<Outcome<()>, HostError> {
        let owner = id.to_string();
        self.call(
            id,
            CallWorld::Interactive,
            Some(doc),
            Self::no_cancel(),
            |e, s, d| Ok(Self::tool(e, &owner)?.call_on_text_input(s, d.unwrap(), text)),
        )
    }

    pub fn on_panel_message(
        &mut self,
        id: &str,
        panel_id: &str,
        message: &str,
    ) -> Result<Outcome<Result<(), ApiError>>, HostError> {
        let owner = id.to_string();
        self.call(
            id,
            CallWorld::Interactive,
            None,
            Self::no_cancel(),
            |e, s, _| Ok(Self::tool(e, &owner)?.call_on_panel_message(s, panel_id, message)),
        )
    }

    pub fn on_document_changed(
        &mut self,
        id: &str,
        doc: &SessionRef,
        change: &Change,
    ) -> Result<Outcome<()>, HostError> {
        let owner = id.to_string();
        let change = tool::DocumentChange {
            transaction: change.transaction,
            affected_pages: change
                .affected_pages
                .iter()
                .map(|p| crate::bindings::v0_1::api_page(*p))
                .collect(),
            structure_changed: change.structure_changed,
        };
        self.call(
            id,
            CallWorld::Interactive,
            Some(doc),
            Self::no_cancel(),
            |e, s, d| Ok(Self::tool(e, &owner)?.call_on_document_changed(s, d.unwrap(), &change)),
        )
    }

    /// Runs the batch job of a mod. `args` and the result are JSON strings.
    pub fn run_batch(
        &mut self,
        id: &str,
        doc: &SessionRef,
        args: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<Outcome<Result<String, ApiError>>, HostError> {
        let owner = id.to_string();
        self.call(id, CallWorld::Batch, Some(doc), cancel, |e, s, d| {
            let batch = e.batch.as_ref().ok_or_else(|| {
                HostError::new("no-export", format!("The mod {owner} has no batch job."))
            })?;
            Ok(batch.call_run(s, d.unwrap(), args))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{MemorySettings, NoFiles};

    struct NoUi;

    impl crate::services::UiBridge for NoUi {
        fn register_action(&self, _: &str, _: crate::api::ui::Action) -> Result<(), String> {
            Ok(())
        }
        fn register_toolbar_item(
            &self,
            _: &str,
            _: crate::api::ui::ToolbarItem,
        ) -> Result<(), String> {
            Ok(())
        }
        fn register_panel(&self, _: &str, _: crate::api::ui::Panel) -> Result<(), String> {
            Ok(())
        }
        fn suggest_shortcut(&self, _: &str, _: crate::api::ui::Shortcut) -> Result<(), String> {
            Ok(())
        }
        fn set_overlay(&self, _: &str, _: Vec<crate::api::types::OverlayShape>) {}
        fn clear_overlay(&self, _: &str) {}
        fn register_overlay_image(&self, _: &str, _: Vec<u8>) -> Result<u32, String> {
            Ok(1)
        }
        fn post_to_panel(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn notify(&self, _: &str, _: crate::api::ui::NotifyLevel, _: &str) {}
        fn confirm(&self, _: &str, _: &str, _: &str) -> bool {
            false
        }
        fn context(&self, _: &str) -> crate::api::ui::UiContext {
            unimplemented!()
        }
        fn display_scale(&self, _: crate::api::types::PageId) -> Option<f64> {
            None
        }
        fn progress(&self, _: &str, _: f32, _: &str) {}
    }

    fn host() -> ModHost {
        let fonts = t_rustable_core::fonts::Fonts::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts"),
        )
        .unwrap();
        let services = Services {
            ui: Arc::new(NoUi),
            files: Arc::new(NoFiles),
            settings: Arc::new(MemorySettings::default()),
            fonts: Arc::new(fonts),
        };
        ModHost::new(services, Limits::default()).unwrap()
    }

    fn links(host: &ModHost, version: &str) -> bool {
        let wat = format!(
            r#"(component (import "t-rustable:api/host@{version}" (instance (export "is-cancelled" (func (result bool))))))"#
        );
        let component = Component::new(&host.engine, wat::parse_str(&wat).unwrap()).unwrap();
        host.linker.instantiate_pre(&component).is_ok()
    }

    #[test]
    fn mods_for_older_0_1_versions_still_link() {
        let host = host();
        assert!(links(&host, "0.1.0"));
        assert!(links(&host, "0.1.1"));
        assert!(!links(&host, "0.2.0"));
    }
}
