//! WASM mod host: loads mods, checks their permissions, and runs their exports with
//! memory and time limits. The GUI and the CLI give it the outside world as `Services`.

pub mod bindings;
mod host;
mod imports;
pub mod services;
mod state;

pub use host::{HostError, Limits, MAX_FAILURES, ModHost, Outcome};
pub use state::{Services, SessionRef};

/// API 0.1 types: overlay shapes, UI records, events, and errors.
pub mod api {
    pub use crate::bindings::v0_1::generated::exports::t_rustable::api::{batch_job, tool};
    pub use crate::bindings::v0_1::generated::t_rustable::api::{
        document, edit, files, host, types, ui,
    };
}

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
