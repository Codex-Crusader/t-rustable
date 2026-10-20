//! The local log file. The app has no crash reporter and no telemetry (ADR 0011).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Log files over this size start again empty.
const MAX_BYTES: u64 = 4 << 20;

pub struct Log {
    file: Mutex<Option<File>>,
}

impl Log {
    pub fn open(dir: &Path) -> Self {
        let path = dir.join("t-rustable.log");
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
            let _ = std::fs::rename(&path, dir.join("t-rustable.old.log"));
        }
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        Self {
            file: Mutex::new(file),
        }
    }

    pub fn write(&self, level: &str, source: &str, message: &str) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = format!("{secs} {level} [{source}] {}\n", message.replace('\n', " "));
        if let Some(f) = self.file.lock().unwrap().as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
    }
}
