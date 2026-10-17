//! Permissions come from component imports (ADR 0007). This module is the table.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// A permission that a mod declares in `manifest.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Permission {
    #[serde(rename = "document.read")]
    DocumentRead,
    #[serde(rename = "document.edit")]
    DocumentEdit,
    #[serde(rename = "ui")]
    Ui,
    #[serde(rename = "files.pick")]
    FilesPick,
}

impl Permission {
    pub const ALL: [Permission; 4] = [
        Self::DocumentRead,
        Self::DocumentEdit,
        Self::Ui,
        Self::FilesPick,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::DocumentRead => "document.read",
            Self::DocumentEdit => "document.edit",
            Self::Ui => "ui",
            Self::FilesPick => "files.pick",
        }
    }

    /// Text for the permission screen.
    pub fn description(self) -> &'static str {
        match self {
            Self::DocumentRead => "Read the text, pages, and objects of open documents.",
            Self::DocumentEdit => "Change open documents. Each change can be undone.",
            Self::Ui => "Add actions, toolbar items, panels, and overlays.",
            Self::FilesPick => "Read files that you pick in a file dialog.",
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What one component import needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportRule {
    /// No permission needed (`types`, `host`).
    Free,
    Needs(Permission),
    /// Not part of the API, or an unsupported version. Install fails.
    Refused,
}

/// Rule for an import name such as `t-rustable:api/document@0.1.0`.
/// Any `0.1.x` version is accepted.
pub fn rule_for_import(import: &str) -> ImportRule {
    let Some(rest) = import.strip_prefix("t-rustable:api/") else {
        return ImportRule::Refused;
    };
    let Some((interface, version)) = rest.split_once('@') else {
        return ImportRule::Refused;
    };
    if !version_supported(version) {
        return ImportRule::Refused;
    }
    match interface {
        "types" | "host" => ImportRule::Free,
        "document" => ImportRule::Needs(Permission::DocumentRead),
        "edit" => ImportRule::Needs(Permission::DocumentEdit),
        "ui" => ImportRule::Needs(Permission::Ui),
        "files" => ImportRule::Needs(Permission::FilesPick),
        _ => ImportRule::Refused,
    }
}

fn version_supported(version: &str) -> bool {
    let mut parts = version.split('.');
    matches!(
        (
            parts.next(),
            parts.next(),
            parts.next().map(|p| p.parse::<u32>().is_ok()),
            parts.next()
        ),
        (Some("0"), Some("1"), Some(true), None)
    )
}

/// A difference between the imports of a component and its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    RefusedImport(String),
    /// The component imports it, but the manifest does not declare it.
    Undeclared(Permission),
    /// The manifest declares it, but the component does not import it.
    Unused(Permission),
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RefusedImport(name) => {
                write!(f, "The mod imports {name}, which mods cannot use.")
            }
            Self::Undeclared(p) => write!(
                f,
                "The mod uses permission {p}, but the manifest does not declare it."
            ),
            Self::Unused(p) => write!(
                f,
                "The manifest declares permission {p}, but the mod does not use it."
            ),
        }
    }
}

/// Compares component imports with the declared permissions. Empty means they match.
pub fn check_imports<'a>(
    imports: impl IntoIterator<Item = &'a str>,
    declared: &[Permission],
) -> Vec<Mismatch> {
    let mut needed = BTreeSet::new();
    let mut problems = Vec::new();
    for import in imports {
        match rule_for_import(import) {
            ImportRule::Free => {}
            ImportRule::Needs(p) => {
                needed.insert(p);
            }
            ImportRule::Refused => problems.push(Mismatch::RefusedImport(import.to_string())),
        }
    }
    let declared: BTreeSet<Permission> = declared.iter().copied().collect();
    problems.extend(
        needed
            .difference(&declared)
            .map(|p| Mismatch::Undeclared(*p)),
    );
    problems.extend(declared.difference(&needed).map(|p| Mismatch::Unused(*p)));
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(
            rule_for_import("t-rustable:api/types@0.1.0"),
            ImportRule::Free
        );
        assert_eq!(
            rule_for_import("t-rustable:api/host@0.1.3"),
            ImportRule::Free
        );
        assert_eq!(
            rule_for_import("t-rustable:api/document@0.1.0"),
            ImportRule::Needs(Permission::DocumentRead)
        );
        assert_eq!(
            rule_for_import("t-rustable:api/edit@0.1.0"),
            ImportRule::Needs(Permission::DocumentEdit)
        );
        assert_eq!(
            rule_for_import("t-rustable:api/ui@0.1.0"),
            ImportRule::Needs(Permission::Ui)
        );
        assert_eq!(
            rule_for_import("t-rustable:api/files@0.1.0"),
            ImportRule::Needs(Permission::FilesPick)
        );
    }

    #[test]
    fn refuses_other_imports() {
        for name in [
            "wasi:filesystem/types@0.2.0",
            "wasi:sockets/tcp@0.2.0",
            "t-rustable:api/document@0.2.0",
            "t-rustable:api/document@1.0.0",
            "t-rustable:api/secret@0.1.0",
            "t-rustable:api/document",
        ] {
            assert_eq!(rule_for_import(name), ImportRule::Refused, "{name}");
        }
    }

    #[test]
    fn compares_both_directions() {
        let imports = [
            "t-rustable:api/types@0.1.0",
            "t-rustable:api/document@0.1.0",
            "t-rustable:api/ui@0.1.0",
        ];
        assert!(check_imports(imports, &[Permission::DocumentRead, Permission::Ui]).is_empty());
        assert_eq!(
            check_imports(imports, &[Permission::DocumentRead]),
            vec![Mismatch::Undeclared(Permission::Ui)]
        );
        assert_eq!(
            check_imports(
                imports,
                &[
                    Permission::DocumentRead,
                    Permission::Ui,
                    Permission::FilesPick
                ]
            ),
            vec![Mismatch::Unused(Permission::FilesPick)]
        );
        assert_eq!(
            check_imports(["wasi:cli/stdout@0.2.12"], &[]),
            vec![Mismatch::RefusedImport("wasi:cli/stdout@0.2.12".into())]
        );
    }
}
