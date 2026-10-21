//! File name rules for packages and panel URLs (ADR 0009, ADR 0019).

/// Longest path in a package, in bytes.
pub const MAX_PATH: usize = 200;
/// Most folder levels in a path, the file included.
pub const MAX_DEPTH: usize = 8;

/// File types that panels can use.
pub const UI_TYPES: [&str; 7] = ["html", "css", "js", "svg", "png", "woff2", "json"];

/// Top-level files that a package can have for people to read.
pub const DOC_FILES: [&str; 4] = ["README.md", "LICENSE", "LICENSE.txt", "LICENSE.md"];

const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];

/// Checks one package path. Returns the problem in words.
pub fn check_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("A file has an empty name.".into());
    }
    if path.len() > MAX_PATH {
        return Err(format!("{path:?} is longer than {MAX_PATH} characters."));
    }
    if let Some(c) = path
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/')))
    {
        return Err(format!(
            "{path:?} has the character {c:?}. Use only ASCII letters, digits, \".\", \"-\", \"_\", and \"/\"."
        ));
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > MAX_DEPTH {
        return Err(format!("{path:?} has more than {MAX_DEPTH} levels."));
    }
    for part in parts {
        if part.is_empty() {
            return Err(format!("{path:?} has an empty part or starts with \"/\"."));
        }
        if part.starts_with('.') || part.ends_with('.') {
            return Err(format!(
                "{path:?} has a part that starts or ends with \".\"."
            ));
        }
        if is_reserved(part) {
            return Err(format!(
                "{path:?} uses the name {part:?}, which Windows reserves."
            ));
        }
    }
    Ok(())
}

/// Windows device names, also with an extension, for example `con.txt` or `COM1.js`.
fn is_reserved(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return true;
    }
    let b = stem.as_bytes();
    b.len() == 4
        && (stem.starts_with("com") || stem.starts_with("lpt"))
        && (b'1'..=b'9').contains(&b[3])
}

/// The extension of a path, without the dot.
pub fn extension(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    name.rsplit_once('.').map(|(_, ext)| ext)
}

/// True for a file type that a panel can use.
pub fn is_ui_file(path: &str) -> bool {
    extension(path).is_some_and(|e| UI_TYPES.contains(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_paths() {
        for p in ["manifest.json", "ui/panel.html", "a_b-c.d/e.js", "LICENSE"] {
            assert_eq!(check_path(p), Ok(()), "{p}");
        }
    }

    #[test]
    fn bad_paths() {
        for p in [
            "",
            "/abs.js",
            "../up.js",
            "a/../b.js",
            "a//b.js",
            "a\\b.js",
            "C:/x.js",
            ".hidden",
            "ui/.DS_Store",
            "trail.",
            "sp ace.js",
            "caf\u{e9}.js",
            "CON",
            "ui/nul.html",
            "Com1.js",
            "lpt9",
            "a/b/c/d/e/f/g/h/i.js",
        ] {
            assert!(check_path(p).is_err(), "{p:?} was accepted");
        }
        assert!(check_path(&"a".repeat(MAX_PATH + 1)).is_err());
    }

    #[test]
    fn names_that_only_look_reserved_are_allowed() {
        for p in [
            "console.js",
            "com10.js",
            "lpt0.js",
            "auxiliary.css",
            "nul-x.js",
        ] {
            assert_eq!(check_path(p), Ok(()), "{p}");
        }
    }

    #[test]
    fn ui_types() {
        assert!(is_ui_file("ui/a.woff2"));
        assert!(!is_ui_file("ui/a.wasm"));
        assert!(!is_ui_file("LICENSE"));
        assert_eq!(extension("ui.x/file"), None);
    }
}
