//! M3 spike: qpdf as the save post-process.
//! Checks: load from memory, structure check, write with object streams,
//! reload of the result, recovery of a damaged file, and the time per file.
//!
//! Run: cargo run --release -p spike-qpdf-save -- <file.pdf>...

use qpdf::{ObjectStreamMode, QPdf};
use std::time::Instant;

fn report(name: &str, ok: bool, detail: impl AsRef<str>) {
    let mark = if ok { "PASS" } else { "FAIL" };
    println!("[{mark}] {name}: {}", detail.as_ref());
}

fn post_process(bytes: &[u8]) -> qpdf::Result<Vec<u8>> {
    let pdf = QPdf::read_from_memory(bytes)?;
    pdf.check_pdf()?;
    pdf.writer()
        .object_stream_mode(ObjectStreamMode::Generate)
        .compress_streams(true)
        .write_to_memory()
}

fn main() {
    println!("qpdf {}", QPdf::library_version());
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("qdf") {
        // Uncompressed, normalized copy for reading content streams.
        let pdf = QPdf::read(&args[2]).expect("read");
        let out = pdf
            .writer()
            .compress_streams(false)
            .normalize_content(true)
            .object_stream_mode(ObjectStreamMode::Disable)
            .write_to_memory()
            .expect("write");
        std::fs::write(&args[3], out).expect("write output");
        return;
    }
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read input");
        let started = Instant::now();
        let result = post_process(&bytes);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        match result {
            Ok(out) => {
                let pages_in = QPdf::read_from_memory(&bytes)
                    .and_then(|p| p.get_num_pages())
                    .unwrap_or(0);
                let reload = QPdf::read_from_memory(&out).and_then(|p| {
                    p.check_pdf()?;
                    p.get_num_pages()
                });
                report(
                    &format!("post-process {path}"),
                    matches!(reload, Ok(n) if n == pages_in),
                    format!(
                        "{} KB to {} KB in {ms:.1} ms, pages {pages_in} then {reload:?}",
                        bytes.len() / 1024,
                        out.len() / 1024
                    ),
                );
            }
            Err(e) => report(&format!("post-process {path}"), false, e.to_string()),
        }

        // A damaged copy: cut the last 20 percent of the file (the xref table goes).
        let cut = &bytes[..bytes.len() * 4 / 5];
        let damaged = QPdf::read_from_memory(cut).and_then(|p| {
            let warn = p.more_warnings();
            let out = p.writer().write_to_memory()?;
            Ok((warn, out.len()))
        });
        report(
            &format!("recover damaged copy of {path}"),
            damaged.is_ok(),
            format!("{damaged:?}"),
        );
    }
}
