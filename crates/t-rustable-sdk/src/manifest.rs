//! `manifest.json` of a mod. The JSON Schema is in `docs/mod-api/manifest.schema.json`.
//! These checks match the schema and add the rules that a schema cannot express.

use crate::permissions::Permission;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub author: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    pub permissions: Vec<Permission>,
    pub entry: String,
    pub worlds: Vec<World>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panels: Vec<Panel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum World {
    Interactive,
    Batch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    pub id: String,
    pub title: String,
    /// HTML file in the package.
    pub entry: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Action {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// For example "Ctrl+Shift+R". The user decides the final shortcut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// True if the action starts a pointer tool. The mod then gets pointer and key
    /// events until the user selects another tool. False for a command that runs once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tool: bool,
}

/// The API versions that this SDK accepts in `apiVersion`.
pub const SUPPORTED_API_VERSIONS: [&str; 1] = ["0.1"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError(pub String);

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

impl Manifest {
    /// Parses and checks a manifest. Returns every problem, not only the first.
    pub fn parse(json: &str) -> Result<Manifest, Vec<ManifestError>> {
        let manifest: Manifest = serde_json::from_str(json)
            .map_err(|e| vec![ManifestError(format!("manifest.json is not valid: {e}."))])?;
        let problems = manifest.check();
        if problems.is_empty() {
            Ok(manifest)
        } else {
            Err(problems)
        }
    }

    pub fn check(&self) -> Vec<ManifestError> {
        let mut problems = Vec::new();
        let mut fail = |text: String| problems.push(ManifestError(text));

        if !is_mod_id(&self.id) {
            fail(format!(
                "id {:?} is not valid. Use 3 to 64 lowercase letters, digits, dots, and dashes, for example \"com.example.rotate\".",
                self.id
            ));
        }
        for (field, value) in [
            ("name", &self.name),
            ("author", &self.author),
            ("description", &self.description),
        ] {
            if value.trim().is_empty() {
                fail(format!("{field} must not be empty."));
            }
        }
        if !is_semver(&self.version) {
            fail(format!(
                "version {:?} is not a semantic version such as \"1.0.0\".",
                self.version
            ));
        }
        if !SUPPORTED_API_VERSIONS.contains(&self.api_version.as_str()) {
            fail(format!(
                "apiVersion {:?} is not supported. Use \"0.1\".",
                self.api_version
            ));
        }
        if !is_package_path(&self.entry, ".wasm") {
            fail(format!(
                "entry {:?} must be a relative path to a .wasm file in the package.",
                self.entry
            ));
        }
        if self.worlds.is_empty() {
            fail("worlds must list \"interactive\", \"batch\", or both.".into());
        }
        if self.worlds.iter().collect::<BTreeSet<_>>().len() != self.worlds.len() {
            fail("worlds has a duplicate entry.".into());
        }
        if self.permissions.iter().collect::<BTreeSet<_>>().len() != self.permissions.len() {
            fail("permissions has a duplicate entry.".into());
        }

        let interactive = self.worlds.contains(&World::Interactive);
        if !interactive {
            for p in [Permission::Ui, Permission::FilesPick] {
                if self.permissions.contains(&p) {
                    fail(format!("permission {p} needs the interactive world."));
                }
            }
            if !self.panels.is_empty() || !self.actions.is_empty() {
                fail("panels and actions need the interactive world.".into());
            }
        }
        if !self.panels.is_empty() && !self.permissions.contains(&Permission::Ui) {
            fail("panels need the permission ui.".into());
        }

        let mut ids = BTreeSet::new();
        for panel in &self.panels {
            if !is_local_id(&panel.id) || !ids.insert(&panel.id) {
                fail(format!(
                    "panel id {:?} is not valid or not unique.",
                    panel.id
                ));
            }
            if !is_package_path(&panel.entry, ".html") {
                fail(format!(
                    "panel entry {:?} must be a relative path to an .html file in the package.",
                    panel.entry
                ));
            }
        }
        let mut ids = BTreeSet::new();
        for action in &self.actions {
            if !is_local_id(&action.id) || !ids.insert(&action.id) {
                fail(format!(
                    "action id {:?} is not valid or not unique.",
                    action.id
                ));
            }
            if action.title.trim().is_empty() {
                fail(format!("action {:?} needs a title.", action.id));
            }
        }
        problems
    }
}

/// Lowercase letters and digits, separated by single dots or dashes. 3 to 64 characters.
fn is_mod_id(id: &str) -> bool {
    (3..=64).contains(&id.len())
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.split(['.', '-']).all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// IDs of panels and actions inside one mod: lowercase letters, digits, and dashes.
fn is_local_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// `MAJOR.MINOR.PATCH` with an optional `-prerelease`. No build metadata.
fn is_semver(version: &str) -> bool {
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    let number_ok = |n: &&str| {
        !n.is_empty()
            && n.bytes().all(|b| b.is_ascii_digit())
            && (n.len() == 1 || !n.starts_with('0'))
    };
    numbers.len() == 3
        && numbers.iter().all(number_ok)
        && pre.is_none_or(|p| {
            !p.is_empty()
                && p.split('.').all(|s| {
                    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
        })
}

/// A relative path inside the package, with forward slashes and no `..`.
fn is_package_path(path: &str, extension: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && path.ends_with(extension)
        && !path.starts_with('/')
        && !path.contains(['\\', ':'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
        "id": "com.example.rotate",
        "name": "Rotate",
        "version": "1.0.0",
        "apiVersion": "0.1",
        "author": "Example",
        "description": "Rotates pages.",
        "permissions": ["document.read", "document.edit", "ui"],
        "entry": "mod.wasm",
        "worlds": ["interactive"],
        "actions": [{ "id": "rotate-page", "title": "Rotate page" }]
    }"#;

    fn errors(json: &str) -> Vec<String> {
        Manifest::parse(json)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.0)
            .collect()
    }

    #[test]
    fn good_manifest() {
        let m = Manifest::parse(GOOD).unwrap();
        assert_eq!(
            m.permissions,
            [
                Permission::DocumentRead,
                Permission::DocumentEdit,
                Permission::Ui
            ]
        );
        assert_eq!(m.worlds, [World::Interactive]);
    }

    #[test]
    fn tool_actions() {
        assert!(!Manifest::parse(GOOD).unwrap().actions[0].tool);
        let tool = GOOD.replace("\"Rotate page\" }", "\"Rotate page\", \"tool\": true }");
        assert!(Manifest::parse(&tool).unwrap().actions[0].tool);
    }

    #[test]
    fn unknown_field_and_permission() {
        assert_eq!(
            errors(&GOOD.replace("\"author\"", "\"extra\": 1, \"author\"")).len(),
            1
        );
        assert_eq!(errors(&GOOD.replace("\"ui\"", "\"network\"")).len(), 1);
    }

    #[test]
    fn bad_values() {
        let bad = GOOD
            .replace("com.example.rotate", "Com..Rotate")
            .replace("1.0.0", "1.0")
            .replace("\"0.1\"", "\"0.2\"")
            .replace("mod.wasm", "../mod.wasm");
        assert_eq!(errors(&bad).len(), 4, "{:?}", errors(&bad));
    }

    #[test]
    fn batch_world_rules() {
        let batch = GOOD.replace("\"interactive\"", "\"batch\"");
        let found = errors(&batch);
        assert!(
            found
                .iter()
                .any(|e| e.contains("permission ui needs the interactive world")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|e| e.contains("panels and actions")),
            "{found:?}"
        );
    }

    #[test]
    fn id_rules() {
        assert!(is_mod_id("com.example.rotate-pages"));
        assert!(is_mod_id("abc"));
        for bad in [
            "ab", "1abc", "com..x", "com.-x", "Com.x", "com.x_y", "com.x.",
        ] {
            assert!(!is_mod_id(bad), "{bad}");
        }
    }

    #[test]
    fn semver_rules() {
        for good in ["0.1.0", "1.2.3", "1.0.0-beta.1"] {
            assert!(is_semver(good), "{good}");
        }
        for bad in ["1.0", "01.0.0", "1.0.0-", "1.0.0+build", "a.b.c"] {
            assert!(!is_semver(bad), "{bad}");
        }
    }

    #[test]
    fn package_paths() {
        assert!(is_package_path("ui/panel.html", ".html"));
        for bad in [
            "/abs.html",
            "../x.html",
            "ui\\x.html",
            "C:/x.html",
            "ui//x.html",
            "x.htm",
        ] {
            assert!(!is_package_path(bad, ".html"), "{bad}");
        }
    }
}
