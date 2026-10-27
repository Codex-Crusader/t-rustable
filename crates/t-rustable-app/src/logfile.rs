//! The local log file. The app has no crash reporter and no telemetry (ADR 0011).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use t_rustable_plugin_host::services::{MAX_TEXT, log_enabled, printable};

/// A log file over this size moves to `t-rustable.old.log`, and a new file starts.
const MAX_BYTES: u64 = 4 << 20;

pub struct Log {
    dir: PathBuf,
    /// The open file and its size.
    file: Mutex<Option<(File, u64)>>,
}

impl Log {
    /// Opens the log file, writes all `log` records into it, and writes panics into it.
    pub fn install(dir: &Path) {
        let log = Self {
            dir: dir.to_path_buf(),
            file: Mutex::new(open(dir, false)),
        };
        if log::set_boxed_logger(Box::new(log)).is_ok() {
            log::set_max_level(if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            });
        }
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error!(target: "panic", "{info}");
            default(info);
        }));
    }

    fn write(&self, level: log::Level, source: &str, message: &str) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Mods and file names put text into messages. Keep each record on one line.
        let line = format!(
            "{secs} {} [{}] {}\n",
            level.as_str().to_ascii_lowercase(),
            printable(source, 128),
            printable(message, 4 * MAX_TEXT)
        );
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if file
            .as_ref()
            .is_some_and(|(_, size)| size + line.len() as u64 > MAX_BYTES)
        {
            *file = None;
            *file = open(&self.dir, true);
        }
        if let Some((f, size)) = file.as_mut()
            && f.write_all(line.as_bytes()).is_ok()
        {
            *size += line.len() as u64;
        }
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
    }
}

impl log::Log for Log {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        log_enabled(metadata)
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.write(record.level(), record.target(), &record.args().to_string());
        }
    }

    fn flush(&self) {}
}

/// Opens the log file for appends. With `rotate`, or if the file is over the size
/// limit, the file moves away first.
fn open(dir: &Path, rotate: bool) -> Option<(File, u64)> {
    let path = dir.join("t-rustable.log");
    if rotate || std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join("t-rustable.old.log"));
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    Some((file, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_and_keeps_each_record_on_one_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = Log {
            dir: dir.path().to_path_buf(),
            file: Mutex::new(open(dir.path(), false)),
        };
        log.write(log::Level::Warn, "a.mod", "one\ntwo\r\x1b[2Jthree");
        let text = std::fs::read_to_string(dir.path().join("t-rustable.log")).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(
            text.ends_with(" warn [a.mod] one two  [2Jthree\n"),
            "{text:?}"
        );

        let big = "x".repeat(4 * MAX_TEXT);
        for _ in 0..(MAX_BYTES as usize / big.len() + 2) {
            log.write(log::Level::Info, "a.mod", &big);
        }
        let size = |name: &str| std::fs::metadata(dir.path().join(name)).unwrap().len();
        assert!(size("t-rustable.log") <= MAX_BYTES);
        assert!(size("t-rustable.old.log") <= MAX_BYTES);
    }
}
