//! The data of one mod store: services, resources, limits, and the current call.

use crate::services::{FilePicker, SettingsStore, UiBridge};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use t_rustable_core::fonts::Fonts;
use t_rustable_core::{Change, Session, Transaction};
use wasmtime::component::ResourceTable;

/// A session that the host can lend to mods. The caller must not hold the lock during a call.
pub type SessionRef = Arc<Mutex<Session>>;

/// Services that all mods of one host share.
#[derive(Clone)]
pub struct Services {
    pub ui: Arc<dyn UiBridge>,
    pub files: Arc<dyn FilePicker>,
    pub settings: Arc<dyn SettingsStore>,
    pub fonts: Arc<Fonts>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallWorld {
    Interactive,
    Batch,
}

/// A document lent to one call.
pub struct DocRes {
    pub session: SessionRef,
}

/// A transaction. Valid only during the call that opened it.
pub struct TxRes {
    pub generation: u64,
    pub session: SessionRef,
    /// None after commit, or after the call ends.
    pub tx: Option<Transaction>,
}

/// A file that the user picked. Valid only during the call that picked it.
pub struct PickedRes {
    pub generation: u64,
    pub name: String,
    pub size: u64,
    pub path: PathBuf,
}

/// The current call. The host sets it before each export call.
pub struct Call {
    pub world: CallWorld,
    pub cancel: Arc<AtomicBool>,
    pub changes: Vec<Change>,
    /// Transactions of this call, to discard at the end.
    pub transactions: Vec<u32>,
}

/// Time in the mod's own code. Time in host imports does not count.
pub struct Timing {
    pub started: Instant,
    pub host: Duration,
    pub in_host_since: Option<Instant>,
    pub limit: Duration,
}

impl Timing {
    pub fn new(limit: Duration) -> Self {
        Self {
            started: Instant::now(),
            host: Duration::ZERO,
            in_host_since: None,
            limit,
        }
    }

    pub fn guest(&self) -> Duration {
        self.started.elapsed().saturating_sub(self.host)
    }
}

/// The memory limit was reached.
#[derive(Debug)]
pub struct MemoryLimit;

impl std::fmt::Display for MemoryLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the mod used more than its memory limit")
    }
}

impl std::error::Error for MemoryLimit {}

/// The time limit was reached.
#[derive(Debug)]
pub struct TimeLimit;

impl std::fmt::Display for TimeLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the mod used more than its time limit")
    }
}

impl std::error::Error for TimeLimit {}

/// Most transactions that a mod can keep open in one call.
pub const MAX_OPEN_TRANSACTIONS: usize = 64;
/// Largest picked file that `read` returns.
pub const MAX_PICKED_FILE: u64 = 256 << 20;
/// Settings of one mod.
pub const MAX_SETTINGS: usize = 64 << 10;
/// Largest overlay image.
pub const MAX_OVERLAY_IMAGE: usize = 16 << 20;

pub struct HostState {
    pub mod_id: String,
    pub services: Services,
    pub table: ResourceTable,
    pub memory_limit: usize,
    pub memory_exceeded: bool,
    pub generation: u64,
    pub call: Option<Call>,
    pub timing: Timing,
}

impl wasmtime::ResourceLimiter for HostState {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _max: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.memory_limit {
            self.memory_exceeded = true;
            return Err(wasmtime::Error::new(MemoryLimit));
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _max: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= 100_000)
    }
}
