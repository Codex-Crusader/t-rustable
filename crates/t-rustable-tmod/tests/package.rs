//! One crafted package for each rule of ADR 0009 and ADR 0019.

use std::collections::BTreeMap;
use std::io::Write as _;

use t_rustable_tmod::{LIMITS, Limits, PackageError, read, read_with, write};
use zip::write::SimpleFileOptions;

const MANIFEST: &str = r#"{
  "id": "com.example.test",
  "name": "Test",
  "version": "1.0.0",
  "apiVersion": "0.1",
  "author": "Someone",
  "description": "A test mod.",
  "permissions": ["document.read", "ui"],
  "entry": "mod.wasm",
  "worlds": ["interactive"],
  "panels": [{ "id": "main", "title": "Main", "entry": "ui/panel.html" }]
}"#;

fn good_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("manifest.json", MANIFEST.as_bytes().to_vec()),
        ("mod.wasm", b"\0asm component bytes".to_vec()),
        (
            "ui/panel.html",
            b"<!doctype html><script src=panel.js></script>".to_vec(),
        ),
        ("ui/panel.js", b"parent.postMessage('hi', '*')".to_vec()),
        ("LICENSE", b"MIT".to_vec()),
    ]
}

/// A zip written without our checks.
fn raw_zip(files: &[(&str, Vec<u8>)], options: SimpleFileOptions) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        if name.ends_with('/') {
            zip.add_directory(*name, options).unwrap();
        } else {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
    }
    zip.finish().unwrap().into_inner()
}

fn deflated() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

fn with(extra: &[(&'static str, Vec<u8>)]) -> Vec<u8> {
    let mut files = good_files();
    files.extend_from_slice(extra);
    raw_zip(&files, deflated())
}

fn code(result: Result<t_rustable_tmod::Package, PackageError>) -> &'static str {
    match result {
        Ok(_) => "accepted",
        Err(e) => e.code,
    }
}

/// Replaces each copy of `from` with `to` (same length), in the local and the central headers.
fn patch(mut bytes: Vec<u8>, from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let mut i = 0;
    let mut hits = 0;
    while i + from.len() <= bytes.len() {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
            hits += 1;
        }
        i += 1;
    }
    assert!(hits > 0, "nothing to patch");
    bytes
}

#[test]
fn a_good_package_is_read() {
    let bytes = raw_zip(&good_files(), deflated());
    let p = read(&bytes).unwrap();
    assert_eq!(p.manifest.id, "com.example.test");
    assert_eq!(p.files.len(), 5);
    assert_eq!(p.component(), b"\0asm component bytes");
    assert_eq!(p.sha256.len(), 64);
}

#[test]
fn folder_entries_are_allowed() {
    let mut files = vec![("ui/", Vec::new())];
    files.extend(good_files());
    assert_eq!(code(read(&raw_zip(&files, deflated()))), "accepted");
}

#[test]
fn stored_entries_are_allowed() {
    let bytes = raw_zip(
        &good_files(),
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    );
    assert_eq!(code(read(&bytes)), "accepted");
}

#[test]
fn unsafe_names_are_refused() {
    for name in [
        "../evil.js",
        "/abs.js",
        "ui\\x.js",
        "C:/x.js",
        ".hidden.js",
        "ui/CON.js",
        "caf\u{e9}.js",
    ] {
        assert_eq!(
            code(read(&with(&[(name, b"x".to_vec())]))),
            "unsafe-name",
            "{name}"
        );
    }
}

#[test]
fn names_that_differ_only_in_case_are_refused() {
    assert_eq!(
        code(read(&with(&[("ui/Panel.js", b"x".to_vec())]))),
        "case-collision"
    );
    assert_eq!(
        code(read(&with(&[("UI/other.js", b"x".to_vec())]))),
        "case-collision"
    );
}

#[test]
fn duplicate_names_are_refused() {
    let bytes = with(&[("ui/zzzzzzz.js", b"x".to_vec())]);
    let bytes = patch(bytes, b"ui/zzzzzzz.js", b"ui/panel.js\0\0");
    // The patch leaves NUL bytes in the name, so it is refused by the name rule or as a duplicate.
    assert!(matches!(
        code(read(&bytes)),
        "unsafe-name" | "duplicate-name"
    ));
    let bytes = with(&[("ui/pznel.js", b"x".to_vec())]);
    let bytes = patch(bytes, b"ui/pznel.js", b"ui/panel.js");
    assert_eq!(code(read(&bytes)), "duplicate-name");
}

#[test]
fn a_file_with_the_name_of_a_folder_is_refused() {
    assert_eq!(
        code(read(&with(&[("ui", b"x".to_vec())]))),
        "duplicate-name"
    );
}

#[test]
fn files_of_other_types_are_refused() {
    for name in ["run.exe", "ui/extra.wasm", "notes.txt", "ui/README.md"] {
        assert_eq!(
            code(read(&with(&[(name, b"x".to_vec())]))),
            "file-not-allowed",
            "{name}"
        );
    }
}

#[test]
fn missing_files_are_refused() {
    let without = |skip: &str| {
        let files: Vec<_> = good_files()
            .into_iter()
            .filter(|(n, _)| *n != skip)
            .collect();
        code(read(&raw_zip(&files, deflated())))
    };
    assert_eq!(without("manifest.json"), "missing-file");
    assert_eq!(without("mod.wasm"), "missing-file");
    assert_eq!(without("ui/panel.html"), "missing-file");
}

#[test]
fn bad_manifests_are_refused() {
    let mut files = good_files();
    files[0].1 = b"{ not json".to_vec();
    assert_eq!(code(read(&raw_zip(&files, deflated()))), "invalid-manifest");
    files[0].1 = MANIFEST.replace("\"1.0.0\"", "\"one\"").into_bytes();
    assert_eq!(code(read(&raw_zip(&files, deflated()))), "invalid-manifest");
    files[0].1 = vec![0xff, 0xfe];
    assert_eq!(code(read(&raw_zip(&files, deflated()))), "invalid-manifest");
}

#[test]
fn reserved_ids_are_refused() {
    for id in ["t-rustable.shapes", "test.panel"] {
        let mut files = good_files();
        files[0].1 = MANIFEST.replace("com.example.test", id).into_bytes();
        assert_eq!(
            code(read(&raw_zip(&files, deflated()))),
            "reserved-id",
            "{id}"
        );
    }
}

#[test]
fn a_zip_bomb_is_refused() {
    let zeros = vec![0u8; 4 * 1024 * 1024];
    assert_eq!(
        code(read(&with(&[("ui/big.json", zeros)]))),
        "package-too-large"
    );
}

#[test]
fn size_limits_count_real_bytes() {
    let small = Limits {
        file: 1000,
        ..LIMITS
    };
    assert_eq!(
        code(read_with(&with(&[("ui/a.json", vec![b'a'; 1001])]), &small)),
        "package-too-large"
    );
    let total = Limits {
        total: 2000,
        ..LIMITS
    };
    assert_eq!(
        code(read_with(
            &with(&[
                ("ui/a.json", vec![b'a'; 900]),
                ("ui/b.json", vec![b'b'; 900])
            ]),
            &total
        )),
        "package-too-large"
    );
    let few = Limits {
        entries: 3,
        ..LIMITS
    };
    assert_eq!(code(read_with(&with(&[]), &few)), "package-too-large");
    let tiny = Limits {
        package: 100,
        ..LIMITS
    };
    assert_eq!(code(read_with(&with(&[]), &tiny)), "package-too-large");
}

#[test]
fn a_header_that_lies_about_the_size_is_caught() {
    // A stored file of 3000 bytes whose headers say 30. The reader must count real bytes.
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut files = good_files();
    files.push(("ui/a.json", vec![b'a'; 3000]));
    let mut bytes = raw_zip(&files, opts);
    let name = b"ui/a.json";
    let mut at = 0;
    let mut patched = 0;
    while let Some(i) = bytes[at..].windows(name.len()).position(|w| w == name) {
        let pos = at + i;
        // Uncompressed size: local header offset 22 (name at 30), central offset 24 (name at 46).
        for (name_offset, size_offset, sig) in [
            (30usize, 22usize, [0x50, 0x4b, 3, 4]),
            (46, 24, [0x50, 0x4b, 1, 2]),
        ] {
            if pos >= name_offset && bytes[pos - name_offset..pos - name_offset + 4] == sig {
                bytes[pos - name_offset + size_offset..pos - name_offset + size_offset + 4]
                    .copy_from_slice(&30u32.to_le_bytes());
                patched += 1;
            }
        }
        at = pos + 1;
    }
    assert_eq!(patched, 2);
    let limits = Limits {
        file: 2000,
        ..LIMITS
    };
    assert_ne!(code(read_with(&bytes, &limits)), "accepted");
}

#[test]
fn encrypted_entries_are_refused() {
    // Set bit 0 (encrypted) of the flags in the local and the central header of mod.wasm.
    let mut bytes = with(&[]);
    let name = b"mod.wasm";
    let mut at = 0;
    while let Some(i) = bytes[at..].windows(name.len()).position(|w| w == name) {
        let pos = at + i;
        // Local header: name at offset 30, flags at 6. Central header: name at 46, flags at 8.
        for (name_offset, flag_offset, sig) in [
            (30usize, 6usize, [0x50, 0x4b, 3, 4]),
            (46, 8, [0x50, 0x4b, 1, 2]),
        ] {
            if pos >= name_offset && bytes[pos - name_offset..pos - name_offset + 4] == sig {
                bytes[pos - name_offset + flag_offset] |= 1;
            }
        }
        at = pos + 1;
    }
    assert_eq!(code(read(&bytes)), "unsupported-entry");
}

#[test]
fn links_are_refused() {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in good_files() {
        zip.start_file(name, deflated()).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.add_symlink("ui/link.js", "../../etc/passwd", deflated())
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert_eq!(code(read(&bytes)), "unsupported-entry");
}

#[test]
fn other_compression_methods_are_refused() {
    // Change the method of the stored manifest from 0 to 12 (bzip2) in both headers.
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut bytes = raw_zip(&good_files(), opts);
    for (sig, method_offset) in [([0x50u8, 0x4b, 3, 4], 8usize), ([0x50, 0x4b, 1, 2], 10)] {
        let mut at = 0;
        while let Some(i) = bytes[at..].windows(4).position(|w| w == sig) {
            let pos = at + i;
            bytes[pos + method_offset] = 12;
            at = pos + 4;
        }
    }
    assert_eq!(code(read(&bytes)), "unsupported-entry");
}

#[test]
fn a_huge_entry_count_is_refused_before_parsing() {
    let mut bytes = with(&[]);
    let n = bytes.len();
    // The end record is the last 22 bytes (no comment). Entry counts at 8 and 10.
    bytes[n - 22 + 8..n - 22 + 12].copy_from_slice(&[0xff, 0xf0, 0xff, 0xf0]);
    assert_eq!(code(read(&bytes)), "package-too-large");
}

#[test]
fn zip64_is_refused() {
    let mut bytes = with(&[]);
    let n = bytes.len();
    let mut locator = vec![0x50, 0x4b, 0x06, 0x07];
    locator.extend_from_slice(&[0; 16]);
    let end = bytes.split_off(n - 22);
    bytes.extend_from_slice(&locator);
    bytes.extend_from_slice(&end);
    assert_eq!(code(read(&bytes)), "unsupported-entry");
}

#[test]
fn junk_is_not_a_package() {
    assert_eq!(code(read(b"")), "not-a-package");
    assert_eq!(
        code(read(b"hello, this is not a zip file at all.")),
        "not-a-package"
    );
    assert_eq!(
        code(read(&[
            0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ])),
        "missing-file"
    );
}

#[test]
fn write_then_read_gives_the_same_files() {
    let files: BTreeMap<String, Vec<u8>> = good_files()
        .into_iter()
        .map(|(n, b)| (n.to_string(), b))
        .collect();
    let bytes = write(&files).unwrap();
    assert_eq!(read(&bytes).unwrap().files, files);
    assert_eq!(
        write(&files).unwrap(),
        bytes,
        "the output must be the same each time"
    );
}

#[test]
fn write_refuses_what_read_refuses() {
    let mut files: BTreeMap<String, Vec<u8>> = good_files()
        .into_iter()
        .map(|(n, b)| (n.to_string(), b))
        .collect();
    files.insert("notes.txt".into(), b"x".to_vec());
    assert_eq!(write(&files).unwrap_err().code, "file-not-allowed");
    files.remove("notes.txt");
    files.insert("ui/../x.js".into(), b"x".to_vec());
    assert_eq!(write(&files).unwrap_err().code, "unsafe-name");
}

#[test]
fn extract_writes_the_files() {
    let p = read(&with(&[])).unwrap();
    let dir = tempfile::tempdir().unwrap();
    p.extract(dir.path()).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("ui/panel.js")).unwrap(),
        b"parent.postMessage('hi', '*')"
    );
}

#[test]
fn collect_folder_takes_only_package_files() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    for (name, bytes) in good_files() {
        let path = d.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::fs::write(d.join("Cargo.toml"), "[package]").unwrap();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/lib.rs"), "").unwrap();
    std::fs::write(d.join("ui/.DS_Store"), "junk").unwrap();
    let (manifest, files) = t_rustable_tmod::collect_folder(d).unwrap();
    assert_eq!(manifest.id, "com.example.test");
    let names: Vec<_> = files.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "LICENSE",
            "manifest.json",
            "mod.wasm",
            "ui/panel.html",
            "ui/panel.js"
        ]
    );
}

#[test]
fn the_fuzz_seeds_from_another_zip_tool_are_read() {
    for seed in [
        &include_bytes!("../../../fuzz/corpus/tmod_read/deflate.tmod")[..],
        &include_bytes!("../../../fuzz/corpus/tmod_read/stored.tmod")[..],
    ] {
        assert_eq!(read(seed).unwrap().manifest.id, "com.example.seed");
    }
}
