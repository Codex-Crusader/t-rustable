//! The `trpanel` scheme: the panel files of loaded mods (ARCHITECTURE section 15).
//! A path is `/<mod-id>/<file path>`. The scheme serves only some file types, and only
//! files inside the folder of the mod.

use std::path::{Path, PathBuf};

/// The CSP of each panel response. A sandboxed panel has an opaque origin, so 'self'
/// does not match on WebKitGTK. The scheme is listed instead (M1 spike 2).
pub const CSP: &str = "default-src 'none'; script-src trpanel: http://trpanel.localhost; \
    style-src trpanel: http://trpanel.localhost; img-src trpanel: http://trpanel.localhost data:; \
    font-src trpanel: http://trpanel.localhost; connect-src 'none'; form-action 'none'; base-uri 'none'";

/// A virtual file in each mod folder with the theme variables of the app.
pub const THEME_FILE: &str = "t-rustable-theme.css";

/// The largest theme text that the UI can set.
pub const MAX_THEME: usize = 64 * 1024;

const TYPES: [(&str, &str); 7] = [
    ("html", "text/html; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("woff2", "font/woff2"),
    ("json", "application/json"),
];

/// Splits a request path into the mod ID and the file path. None if the path has
/// characters other than ASCII letters, digits, `.`, `-`, `_`, and `/`, an empty or
/// dot segment, or a file type that is not allowed.
pub fn parse(path: &str) -> Option<(&str, &str)> {
    let (mod_id, file) = path.strip_prefix('/')?.split_once('/')?;
    let ok = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'/'))
            && s.split('/')
                .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
    };
    (ok(mod_id) && !mod_id.contains('/') && ok(file) && mime(file).is_some())
        .then_some((mod_id, file))
}

pub fn mime(file: &str) -> Option<&'static str> {
    let ext = file.rsplit_once('.')?.1;
    TYPES.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

/// The file inside `dir`, after symbolic links. None if it is outside `dir` or missing.
pub fn resolve(dir: &Path, file: &str) -> Option<PathBuf> {
    let dir = dir.canonicalize().ok()?;
    let path = dir.join(file).canonicalize().ok()?;
    (path.starts_with(&dir) && path.is_file()).then_some(path)
}

/// Puts a link to the theme file at the start of the `<head>` of an HTML file, so the
/// panel styles can use the `--tr-*` variables.
pub fn inject_theme(html: &[u8], mod_id: &str) -> Vec<u8> {
    let link = format!(r#"<link rel="stylesheet" href="/{mod_id}/{THEME_FILE}">"#);
    let lower = html.to_ascii_lowercase();
    let at = lower
        .windows(5)
        .position(|w| w == b"<head")
        .and_then(|i| {
            lower[i..]
                .iter()
                .position(|&b| b == b'>')
                .map(|j| i + j + 1)
        })
        .unwrap_or(0);
    [&html[..at], link.as_bytes(), &html[at..]].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(
            parse("/test.panel/panel/index.html"),
            Some(("test.panel", "panel/index.html"))
        );
        for bad in [
            "/test.panel/../x.html",
            "/test.panel/panel//x.html",
            "/test.panel/./x.html",
            "/test.panel/x.exe",
            "/test.panel/x%2e%2e.html",
            "/test.panel/a\\b.html",
            "/test.panel",
            "test.panel/x.html",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn theme_link_goes_into_head() {
        let out = inject_theme(b"<!doctype html><HTML><Head lang=\"en\"><title>", "m.x");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(
            r#"<Head lang="en"><link rel="stylesheet" href="/m.x/t-rustable-theme.css"><title>"#
        ));
        let bare = String::from_utf8(inject_theme(b"<p>x</p>", "m.x")).unwrap();
        assert!(bare.starts_with("<link"));
    }

    #[test]
    fn files_stay_inside_the_mod_folder() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("mod");
        std::fs::create_dir_all(dir.join("panel")).unwrap();
        std::fs::write(dir.join("panel/a.html"), "a").unwrap();
        std::fs::write(root.path().join("secret.html"), "s").unwrap();
        assert!(resolve(&dir, "panel/a.html").is_some());
        assert!(resolve(&dir, "../secret.html").is_none());
        assert!(resolve(&dir, "panel/missing.html").is_none());
    }
}
