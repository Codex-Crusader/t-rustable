//! The qpdf step of the save pipeline (ADR 0014). qpdf has no shared state with PDFium,
//! so this runs on the caller's thread.

use qpdf::{ObjectStreamMode, QPdf};

/// Repairs and writes the file again with compressed object streams. Then reads the result
/// back and checks its structure.
pub fn post_process(pdf: &[u8]) -> Result<Vec<u8>, String> {
    let doc = QPdf::read_from_memory(pdf).map_err(|e| format!("qpdf cannot read the file: {e}"))?;
    let out = doc
        .writer()
        .object_stream_mode(ObjectStreamMode::Generate)
        .compress_streams(true)
        .write_to_memory()
        .map_err(|e| format!("qpdf cannot write the file: {e}"))?;
    QPdf::read_from_memory(&out)
        .and_then(|check| check.check_pdf())
        .map_err(|e| format!("the written file failed the qpdf structure check: {e}"))?;
    Ok(out)
}
