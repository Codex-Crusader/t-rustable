//! The crash journal: a folder in the `.trex` format (ADR 0008).
//!
//! ```text
//! manifest.json      format version, app version, PDFium release, ID seed
//! original.pdf       the original file, unchanged
//! commands.jsonl     one entry per line
//! assets/<sha256>    images and fonts that commands use
//! ```

use crate::assets::{AssetId, Assets};
use crate::command::Command;
use crate::error::{CoreError, Result, code};
use crate::ids::Seed;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const FORMAT: u32 = 1;
/// The PDFium release that the engine uses. A replay with another release can differ.
pub const PDFIUM_RELEASE: &str = "chromium/8100";

const MANIFEST: &str = "manifest.json";
const ORIGINAL: &str = "original.pdf";
const COMMANDS: &str = "commands.jsonl";
const ASSETS: &str = "assets";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub app_version: String,
    pub pdfium: String,
    pub seed: Seed,
    pub original_sha256: AssetId,
    /// File name of the original, for the list of sessions to recover.
    pub source_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Entry {
    Commit {
        id: u64,
        label: String,
        source: String,
        commands: Vec<Command>,
    },
    Undo,
    Redo,
}

fn io(path: &Path, e: std::io::Error) -> CoreError {
    CoreError::io(&path.display().to_string(), e)
}

fn sync_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| io(dir, e))?;
    let _ = dir;
    Ok(())
}

fn write_synced(path: &Path, data: &[u8]) -> Result<()> {
    let mut f = File::create(path).map_err(|e| io(path, e))?;
    f.write_all(data)
        .and_then(|_| f.sync_all())
        .map_err(|e| io(path, e))
}

pub struct Journal {
    dir: PathBuf,
    lines: File,
    written: HashSet<AssetId>,
}

/// The content of a journal folder.
pub struct Recovered {
    pub manifest: Manifest,
    pub original: Arc<[u8]>,
    pub entries: Vec<Entry>,
    pub assets: Assets,
}

impl Journal {
    /// Makes a new journal folder in `sessions_dir`.
    pub fn create(sessions_dir: &Path, manifest: &Manifest, original: &[u8]) -> Result<Self> {
        fs::create_dir_all(sessions_dir).map_err(|e| io(sessions_dir, e))?;
        let dir = sessions_dir.join(crate::ids::PageId::random().to_string());
        fs::create_dir(&dir).map_err(|e| io(&dir, e))?;
        fs::create_dir(dir.join(ASSETS)).map_err(|e| io(&dir, e))?;
        write_synced(&dir.join(ORIGINAL), original)?;
        let json =
            serde_json::to_vec_pretty(manifest).map_err(|e| CoreError::internal(e.to_string()))?;
        // The manifest comes last: a folder without it is not a journal.
        write_synced(&dir.join(MANIFEST), &json)?;
        let path = dir.join(COMMANDS);
        let lines = File::create_new(&path).map_err(|e| io(&path, e))?;
        sync_dir(&dir)?;
        sync_dir(sessions_dir)?;
        Ok(Self {
            dir,
            lines,
            written: HashSet::new(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Reads a journal folder. A partial last line, from a crash, is ignored.
    pub fn read(dir: &Path) -> Result<Recovered> {
        let bad = |m: String| {
            CoreError::new(code::DAMAGED_FILE, "The session journal is damaged.").with_details(m)
        };
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(dir.join(MANIFEST)).map_err(|e| io(dir, e))?)
                .map_err(|e| bad(format!("manifest: {e}")))?;
        if manifest.format != FORMAT {
            return Err(bad(format!("format {} is not supported", manifest.format)));
        }
        let original: Arc<[u8]> = fs::read(dir.join(ORIGINAL)).map_err(|e| io(dir, e))?.into();
        if AssetId::of(&original) != manifest.original_sha256 {
            return Err(bad("original.pdf does not match its SHA-256".into()));
        }
        let text = fs::read(dir.join(COMMANDS)).map_err(|e| io(dir, e))?;
        let complete = text.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let mut entries = Vec::new();
        for (n, line) in text[..complete].split(|&b| b == b'\n').enumerate() {
            if line.is_empty() {
                continue;
            }
            entries.push(
                serde_json::from_slice(line).map_err(|e| bad(format!("line {}: {e}", n + 1)))?,
            );
        }
        let mut assets = Assets::default();
        let asset_dir = dir.join(ASSETS);
        for item in fs::read_dir(&asset_dir).map_err(|e| io(&asset_dir, e))? {
            let path = item.map_err(|e| io(&asset_dir, e))?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Ok(id) = name.parse::<AssetId>() else {
                continue;
            }; // a temporary file
            let data: Arc<[u8]> = fs::read(&path).map_err(|e| io(&path, e))?.into();
            if assets.add(data) != id {
                return Err(bad(format!("asset {name} does not match its SHA-256")));
            }
        }
        Ok(Recovered {
            manifest,
            original,
            entries,
            assets,
        })
    }

    /// Opens a journal folder to continue it. Removes a partial last line.
    pub fn reopen(dir: &Path, assets: &Assets) -> Result<Self> {
        let path = dir.join(COMMANDS);
        let text = fs::read(&path).map_err(|e| io(&path, e))?;
        let complete = text.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let lines = OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| io(&path, e))?;
        lines
            .set_len(complete as u64)
            .and_then(|_| lines.sync_all())
            .map_err(|e| io(&path, e))?;
        let mut lines = lines;
        use std::io::Seek;
        lines
            .seek(std::io::SeekFrom::End(0))
            .map_err(|e| io(&path, e))?;
        let written = assets.ids().collect();
        Ok(Self {
            dir: dir.to_path_buf(),
            lines,
            written,
        })
    }

    /// Writes the missing assets, then appends one line and flushes it to disk.
    pub fn append(&mut self, entry: &Entry, assets: &Assets) -> Result<()> {
        if let Entry::Commit { commands, .. } = entry {
            for id in commands.iter().filter_map(Command::assets) {
                if self.written.contains(&id) {
                    continue;
                }
                let data = assets
                    .get(id)
                    .ok_or_else(|| CoreError::internal(format!("asset {id} is missing")))?;
                let tmp = self.dir.join(ASSETS).join(format!("{id}.tmp"));
                write_synced(&tmp, data)?;
                let path = self.dir.join(ASSETS).join(id.to_string());
                fs::rename(&tmp, &path).map_err(|e| io(&path, e))?;
                sync_dir(&self.dir.join(ASSETS))?;
                self.written.insert(id);
            }
        }
        let mut line = serde_json::to_vec(entry).map_err(|e| CoreError::internal(e.to_string()))?;
        line.push(b'\n');
        let path = self.dir.join(COMMANDS);
        self.lines
            .write_all(&line)
            .and_then(|_| self.lines.sync_data())
            .map_err(|e| io(&path, e))
    }

    /// Deletes the journal, when the user closes the session.
    pub fn delete(self) -> Result<()> {
        drop(self.lines);
        fs::remove_dir_all(&self.dir).map_err(|e| io(&self.dir, e))
    }
}

/// A journal of a session that did not close, for the recover dialog.
#[derive(Clone, Debug)]
pub struct Unclosed {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

/// Lists the journals in `sessions_dir`. Folders without a valid manifest are skipped.
pub fn list_unclosed(sessions_dir: &Path) -> Result<Vec<Unclosed>> {
    let Ok(items) = fs::read_dir(sessions_dir) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in items.flatten() {
        let dir = item.path();
        let Ok(bytes) = fs::read(dir.join(MANIFEST)) else {
            continue;
        };
        if let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) {
            out.push(Unclosed { dir, manifest });
        }
    }
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PageId;

    fn manifest(original: &[u8]) -> Manifest {
        Manifest {
            format: FORMAT,
            app_version: crate::VERSION.into(),
            pdfium: PDFIUM_RELEASE.into(),
            seed: Seed([5; 32]),
            original_sha256: AssetId::of(original),
            source_name: Some("a.pdf".into()),
        }
    }

    fn commit(id: u64, page: PageId) -> Entry {
        Entry::Commit {
            id,
            label: "Rotate".into(),
            source: "user".into(),
            commands: vec![Command::RotatePage { page, degrees: 90 }],
        }
    }

    #[test]
    fn append_read_and_ignore_a_partial_last_line() {
        let root = tempfile::tempdir().unwrap();
        let mut assets = Assets::default();
        let font = assets.add(Arc::from(&b"font data"[..]));
        let mut j = Journal::create(root.path(), &manifest(b"%PDF"), b"%PDF").unwrap();
        let p = PageId(1);
        let text = Command::AddText {
            id: crate::ids::ObjectId(2),
            page: p,
            at: t_rustable_engine::Point { x: 0.0, y: 0.0 },
            text: "x".into(),
            font,
            size: 10.0,
            color: t_rustable_engine::Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        };
        let with_asset = Entry::Commit {
            id: 1,
            label: "Text".into(),
            source: "mod:add-text".into(),
            commands: vec![text],
        };
        j.append(&with_asset, &assets).unwrap();
        j.append(&Entry::Undo, &assets).unwrap();
        j.append(&commit(2, p), &assets).unwrap();
        let dir = j.dir().to_path_buf();
        drop(j);
        // A crash in the middle of a line.
        let mut f = OpenOptions::new()
            .append(true)
            .open(dir.join(COMMANDS))
            .unwrap();
        f.write_all(b"{\"kind\":\"com").unwrap();
        drop(f);

        let r = Journal::read(&dir).unwrap();
        assert_eq!(r.entries, [with_asset, Entry::Undo, commit(2, p)]);
        assert_eq!(&**r.assets.get(font).unwrap(), b"font data");
        assert_eq!(&*r.original, b"%PDF");

        // Reopen cuts the partial line, so new lines stay valid.
        let mut j = Journal::reopen(&dir, &r.assets).unwrap();
        j.append(&Entry::Redo, &r.assets).unwrap();
        assert_eq!(
            Journal::read(&dir).unwrap().entries.last(),
            Some(&Entry::Redo)
        );

        let list = list_unclosed(root.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].manifest.source_name.as_deref(), Some("a.pdf"));
        j.delete().unwrap();
        assert!(list_unclosed(root.path()).unwrap().is_empty());
    }

    #[test]
    fn damaged_journals_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let assets = Assets::default();
        let mut j = Journal::create(root.path(), &manifest(b"%PDF"), b"%PDF").unwrap();
        j.append(&commit(1, PageId(1)), &assets).unwrap();
        let dir = j.dir().to_path_buf();
        drop(j);

        fs::write(
            dir.join(ASSETS).join(AssetId::of(b"x").to_string()),
            b"not x",
        )
        .unwrap();
        assert_eq!(Journal::read(&dir).err().unwrap().code, code::DAMAGED_FILE);
        fs::remove_file(dir.join(ASSETS).join(AssetId::of(b"x").to_string())).unwrap();

        fs::write(dir.join(ORIGINAL), b"changed").unwrap();
        assert_eq!(Journal::read(&dir).err().unwrap().code, code::DAMAGED_FILE);
        fs::write(dir.join(ORIGINAL), b"%PDF").unwrap();

        // A bad complete line is damage, not a crash.
        fs::write(dir.join(COMMANDS), b"{bad}\n").unwrap();
        assert_eq!(Journal::read(&dir).err().unwrap().code, code::DAMAGED_FILE);
    }
}
