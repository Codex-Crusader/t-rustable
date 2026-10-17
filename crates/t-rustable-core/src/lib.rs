//! Sessions, stable IDs, commands, and the journal.

pub mod assets;
pub mod coords;
pub mod error;
pub mod fonts;
pub mod ids;

pub use assets::{AssetId, Assets};
pub use error::{CoreError, Result};
pub use ids::{ObjectId, PageId, Seed};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
