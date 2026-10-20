//! App folders (ARCHITECTURE section 12) and bundled resources.

use std::path::{Path, PathBuf};

pub struct Folders {
    pub config: PathBuf,
    pub sessions: PathBuf,
    pub log: PathBuf,
}

impl Folders {
    /// The standard folders of the platform. `T_RUSTABLE_HOME` puts all of them in one
    /// folder, for tests and the self-test.
    pub fn standard() -> Self {
        if let Some(home) = std::env::var_os("T_RUSTABLE_HOME") {
            return Self::under(Path::new(&home));
        }
        if cfg!(windows) {
            let roaming = env_dir("APPDATA").unwrap_or_else(|| PathBuf::from("."));
            let local = env_dir("LOCALAPPDATA").unwrap_or_else(|| roaming.clone());
            Self {
                config: roaming.join("T-Rustable").join("config"),
                sessions: local.join("T-Rustable").join("sessions"),
                log: local.join("T-Rustable").join("log"),
            }
        } else {
            let home = env_dir("HOME").unwrap_or_else(|| PathBuf::from("."));
            let config = env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
            let state = env_dir("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state"));
            Self {
                config: config.join("t-rustable"),
                sessions: state.join("t-rustable").join("sessions"),
                log: state.join("t-rustable").join("log"),
            }
        }
    }

    pub fn under(root: &Path) -> Self {
        Self {
            config: root.join("config"),
            sessions: root.join("sessions"),
            log: root.join("log"),
        }
    }

    pub fn create(&self) -> std::io::Result<()> {
        for dir in [&self.config, &self.sessions, &self.log] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// An absolute path from a variable, else None.
fn env_dir(name: &str) -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os(name)?);
    p.is_absolute().then_some(p)
}

/// Bundled files: PDFium, the fonts, and the built-in mods. A release bundle puts them
/// next to the program. A developer build uses the repository folders.
pub struct Resources {
    pub pdfium: PathBuf,
    pub fonts: PathBuf,
    pub mods: PathBuf,
}

impl Resources {
    pub fn find() -> Result<Self, String> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pick = |bundled: &str, dev: PathBuf| match &exe_dir {
            Some(dir) if dir.join(bundled).exists() => dir.join(bundled),
            _ => dev,
        };
        let pdfium = match std::env::var_os("T_RUSTABLE_PDFIUM") {
            Some(p) => PathBuf::from(p),
            None => match &exe_dir {
                Some(dir) if dir.join(t_rustable_engine::pdfium_library_name()).is_file() => {
                    dir.join(t_rustable_engine::pdfium_library_name())
                }
                _ => t_rustable_engine::dev_pdfium_path()?,
            },
        };
        let mods = match std::env::var_os("T_RUSTABLE_BUILTIN_MODS") {
            Some(p) => PathBuf::from(p),
            None => pick("mods", repo.join("target/mods")),
        };
        if !mods.is_dir() {
            return Err(format!(
                "The built-in mods are missing at {}. Run: cargo xtask build-mods",
                mods.display()
            ));
        }
        Ok(Self {
            pdfium,
            fonts: pick("fonts", repo.join("fonts")),
            mods,
        })
    }
}
