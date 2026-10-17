//! PDFium and qpdf adapters and the engine actor.
//!
//! One thread owns all PDFium calls. Other threads send [`Request`]s with a priority and
//! a cancel flag, and get exactly one reply each. Pages and objects are addressed by index.
//! The core maps stable IDs to indices (ADR 0015).

mod actor;
mod handler;
#[allow(unsafe_code)]
mod pdfium;
pub mod post;
pub mod protocol;

pub use actor::{Engine, Ticket};
pub use protocol::*;

use std::path::PathBuf;

/// File name of the PDFium library on this platform.
pub fn pdfium_library_name() -> &'static str {
    if cfg!(windows) {
        "pdfium.dll"
    } else {
        "libpdfium.so"
    }
}

/// PDFium for tests and developer builds: `T_RUSTABLE_PDFIUM` (the library file), else
/// the build that `scripts/fetch-pdfium.sh` put in `.pdfium/` of this repository.
pub fn dev_pdfium_path() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("T_RUSTABLE_PDFIUM") {
        return Ok(PathBuf::from(p));
    }
    let platform = if cfg!(windows) {
        "win-x64/bin"
    } else {
        "linux-x64/lib"
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.pdfium")
        .join(platform)
        .join(pdfium_library_name());
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "PDFium is missing at {}. Run `bash scripts/fetch-pdfium.sh` in the repository root.",
            path.display()
        ))
    }
}
