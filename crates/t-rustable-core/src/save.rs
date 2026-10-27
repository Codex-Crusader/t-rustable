//! The last steps of the save pipeline (ARCHITECTURE section 9): a temporary file in the
//! target folder, a flush, and a rename. The target changes only at the rename.

use crate::error::{CoreError, Result, code};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a rename tries again while another program has the target open (Windows).
const RENAME_RETRY: Duration = Duration::from_secs(1);

fn io(path: &Path, e: std::io::Error) -> CoreError {
    CoreError::io(&path.display().to_string(), e)
}

/// Reads a JSON file that the app keeps, for example settings. A missing file gives the
/// default. A damaged file also gives the default, but first it moves to
/// `<name>.damaged`, so the next save does not replace it without a trace.
pub fn read_kept<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return T::default(),
        Err(e) => {
            log::error!("{}: {e}", path.display());
            return T::default();
        }
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        let mut aside = path.as_os_str().to_owned();
        aside.push(".damaged");
        let aside = std::path::PathBuf::from(aside);
        match std::fs::rename(path, &aside) {
            Ok(()) => log::error!(
                "{} is damaged ({e}). It moved to {}.",
                path.display(),
                aside.display()
            ),
            Err(r) => log::error!("{} is damaged ({e}). It cannot move: {r}", path.display()),
        }
        T::default()
    })
}

pub fn write_atomic(target: &Path, data: &[u8]) -> Result<()> {
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| CoreError::invalid(format!("{} is not a file path.", target.display())))?;
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let tmp = dir.join(format!(".{name}.{}.tmp", crate::ids::PageId::random()));
    let written = File::create_new(&tmp).and_then(|mut f| {
        f.write_all(data)?;
        f.sync_all()
    });
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(&tmp, e));
    }
    let started = Instant::now();
    loop {
        match std::fs::rename(&tmp, target) {
            Ok(()) => break,
            Err(e)
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    && started.elapsed() < RENAME_RETRY =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    return Err(CoreError::new(
                        code::FILE_IN_USE,
                        format!("{} is in use by another program.", target.display()),
                    )
                    .with_hint("Close the file in the other program, then save again.")
                    .with_details(e.to_string()));
                }
                return Err(io(target, e));
            }
        }
    }
    #[cfg(unix)]
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| io(dir, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_kept_moves_a_damaged_file_away() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mods.json");
        let read = || read_kept::<std::collections::BTreeMap<String, u32>>(&path);
        assert!(read().is_empty());
        std::fs::write(&path, br#"{"a": 1}"#).unwrap();
        assert_eq!(read()["a"], 1);
        std::fs::write(&path, b"{ damaged").unwrap();
        assert!(read().is_empty());
        assert!(!path.exists());
        assert_eq!(
            std::fs::read(dir.path().join("mods.json.damaged")).unwrap(),
            b"{ damaged"
        );
    }

    #[test]
    fn replaces_the_target_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.pdf");
        std::fs::write(&target, b"old").unwrap();
        write_atomic(&target, b"new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_failed_save_leaves_the_target_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        // The target is a folder, so the rename fails.
        let target = dir.path().join("folder.pdf");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), b"x").unwrap();
        assert!(write_atomic(&target, b"new").is_err());
        assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"x");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temporary file"
        );
    }

    #[cfg(windows)]
    #[test]
    fn open_target_gives_file_in_use_on_windows() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("open.pdf");
        std::fs::write(&target, b"old").unwrap();
        // Share mode read and write, but not delete: like many PDF viewers.
        let _held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&target)
            .unwrap();
        let e = write_atomic(&target, b"new").unwrap_err();
        assert_eq!(e.code, code::FILE_IN_USE);
        drop(_held);
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temporary file"
        );
    }
}
