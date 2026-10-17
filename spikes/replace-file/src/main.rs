//! M3 spike: the last step of the save pipeline is a rename over the target.
//! What happens when another program has the target open?
//! On Windows, the share mode of the other program decides. A PDF viewer often opens
//! a file without FILE_SHARE_DELETE.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

fn report(name: &str, ok: bool, detail: impl AsRef<str>) {
    let mark = if ok { "PASS" } else { "INFO" };
    println!("[{mark}] {name}: {}", detail.as_ref());
}

fn attempt(dir: &Path, name: &str, open: impl FnOnce(&Path) -> std::io::Result<File>) {
    let target = dir.join("target.pdf");
    let temp = dir.join("target.pdf.tmp");
    fs::write(&target, b"old").unwrap();
    let mut t = File::create(&temp).unwrap();
    t.write_all(b"new").unwrap();
    t.sync_all().unwrap();
    drop(t);
    let holder = open(&target).unwrap();
    let result = fs::rename(&temp, &target);
    let content = fs::read(&target).unwrap_or_default();
    drop(holder);
    report(
        name,
        result.is_ok() && content == b"new",
        format!(
            "rename: {result:?}; target now {:?}",
            String::from_utf8_lossy(&content)
        ),
    );
    let _ = fs::remove_file(&temp);
    let _ = fs::remove_file(&target);
}

fn main() {
    let dir = std::env::temp_dir().join("t-rustable-replace-spike");
    fs::create_dir_all(&dir).unwrap();

    let other = dir.join("other.txt");
    fs::write(&other, b"x").unwrap();
    attempt(&dir, "target not open", |_| File::open(&other));
    attempt(&dir, "target open by Rust std (default share mode)", |p| {
        File::open(p)
    });

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const SHARE_READ: u32 = 1;
        const SHARE_WRITE: u32 = 2;
        const SHARE_DELETE: u32 = 4;
        attempt(&dir, "target open with share read only", |p| {
            OpenOptions::new().read(true).share_mode(SHARE_READ).open(p)
        });
        attempt(&dir, "target open with share read and write", |p| {
            OpenOptions::new()
                .read(true)
                .share_mode(SHARE_READ | SHARE_WRITE)
                .open(p)
        });
        attempt(
            &dir,
            "target open with share read, write, and delete",
            |p| {
                OpenOptions::new()
                    .read(true)
                    .share_mode(SHARE_READ | SHARE_WRITE | SHARE_DELETE)
                    .open(p)
            },
        );
    }
    #[cfg(not(windows))]
    let _ = OpenOptions::new();
    let _ = fs::remove_file(&other);
    let _ = fs::remove_dir(&dir);
}
