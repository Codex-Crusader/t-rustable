//! Sessions, stable IDs, commands, and the journal.

pub mod assets;
pub mod command;
pub mod coords;
pub mod error;
pub mod fonts;
pub mod ids;
pub mod journal;
pub mod save;
pub mod session;

pub use assets::{AssetId, Assets};
pub use error::{CoreError, Result};
pub use ids::{ObjectId, PageId, Seed};
pub use session::{Change, OpenOptions, SaveOptions, Session, Transaction};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
