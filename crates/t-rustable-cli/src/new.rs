//! `t-rustable mods new`: a new mod folder from the template in `mods/template/`.

use std::path::{Path, PathBuf};

use t_rustable_sdk::manifest::Manifest;

/// The template files, built into the program.
const FILES: [(&str, &str); 8] = [
    (
        ".cargo/config.toml",
        include_str!("../../../mods/template/.cargo/config.toml"),
    ),
    (
        ".gitignore",
        include_str!("../../../mods/template/.gitignore"),
    ),
    (
        "Cargo.toml",
        include_str!("../../../mods/template/Cargo.toml"),
    ),
    (
        "README.md",
        include_str!("../../../mods/template/README.md"),
    ),
    (
        "build.ps1",
        include_str!("../../../mods/template/build.ps1"),
    ),
    ("build.sh", include_str!("../../../mods/template/build.sh")),
    (
        "manifest.json",
        include_str!("../../../mods/template/manifest.json"),
    ),
    (
        "src/lib.rs",
        include_str!("../../../mods/template/src/lib.rs"),
    ),
];

const TEMPLATE_CRATE: &str = "rotate-page";
const TEMPLATE_ENTRY: &str = "rotate_page.wasm";
const TEMPLATE_SDK: &str = r#"t-rustable-sdk = { path = "../../crates/t-rustable-sdk" }"#;
const SDK_GIT: &str = "https://github.com/Codex-Crusader/t-rustable";

pub struct NewMod {
    pub name: String,
    pub id: Option<String>,
    pub dir: Option<PathBuf>,
    /// A local SDK folder instead of the git repository.
    pub sdk: Option<PathBuf>,
}

/// The crate name from the display name: lowercase letters and digits with dashes.
pub fn slug(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    (out.starts_with(|c: char| c.is_ascii_lowercase()) && out.len() <= 48).then_some(out)
}

/// Writes the new mod. Returns its folder.
pub fn run(args: NewMod) -> Result<PathBuf, String> {
    let slug = slug(&args.name).ok_or_else(|| {
        format!(
            "{:?} cannot be a mod name. Start with a letter and use at most 48 letters and digits.",
            args.name
        )
    })?;
    let id = args.id.unwrap_or_else(|| format!("com.example.{slug}"));
    let dir = args.dir.unwrap_or_else(|| PathBuf::from(&slug));
    if dir.exists() && std::fs::read_dir(&dir).map_or(true, |mut d| d.next().is_some()) {
        return Err(format!("{} exists and is not empty.", dir.display()));
    }
    let entry = format!("{}.wasm", slug.replace('-', "_"));
    let sdk = match &args.sdk {
        Some(path) => {
            let path = std::path::absolute(path).map_err(|e| e.to_string())?;
            format!(
                "t-rustable-sdk = {{ path = {:?} }}",
                path.to_string_lossy().replace('\\', "/")
            )
        }
        None => format!("t-rustable-sdk = {{ git = {SDK_GIT:?} }}"),
    };

    let mut manifest: serde_json::Value =
        serde_json::from_str(FILES[6].1).map_err(|e| e.to_string())?;
    manifest["id"] = id.clone().into();
    manifest["name"] = args.name.trim().into();
    manifest["entry"] = entry.clone().into();
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())? + "\n";
    Manifest::parse(&manifest_json).map_err(|problems| {
        let text: Vec<_> = problems.into_iter().map(|p| p.0).collect();
        text.join(" ")
    })?;

    for (path, text) in FILES {
        let text = match path {
            "manifest.json" => manifest_json.clone(),
            "Cargo.toml" => text
                .replace(
                    &format!("name = \"{TEMPLATE_CRATE}\""),
                    &format!("name = \"{slug}\""),
                )
                .replace(TEMPLATE_SDK, &sdk),
            _ => text.replace(TEMPLATE_ENTRY, &entry),
        };
        write(&dir.join(path), &text)?;
    }
    Ok(dir)
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("My Stamp").as_deref(), Some("my-stamp"));
        assert_eq!(slug("  Page #2 tool ").as_deref(), Some("page-2-tool"));
        assert_eq!(slug("2fast"), None);
        assert_eq!(slug("!!!"), None);
    }

    #[test]
    fn the_template_has_the_texts_that_new_replaces() {
        let cargo = FILES.iter().find(|f| f.0 == "Cargo.toml").unwrap().1;
        assert!(cargo.contains(&format!("name = \"{TEMPLATE_CRATE}\"")));
        assert!(cargo.contains(TEMPLATE_SDK));
        assert!(FILES[6].0 == "manifest.json" && FILES[6].1.contains(TEMPLATE_ENTRY));
    }
}
