//! The document commands, through the real program. Each test has its own
//! `T_RUSTABLE_HOME`. The built-in mods come from `cargo xtask build-mods`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use t_rustable_engine::testing::{blank_pdf, encrypt, text_pdf};

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let mods = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/mods");
        assert!(mods.is_dir(), "Run: cargo xtask build-mods");
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let p = self.path(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p.to_str().unwrap().to_string()
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_t-rustable"));
        c.args(args)
            .env("T_RUSTABLE_HOME", self.path("home"))
            .env_remove("T_RUSTABLE_PASSWORD");
        for (k, v) in env {
            c.env(k, v);
        }
        c.output().unwrap()
    }

    /// Runs with `--json` and returns the exit code and the report.
    fn json(&self, args: &[&str]) -> (i32, Value) {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        let out = self.run(&all);
        let report =
            serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(&out)));
        (out.status.code().unwrap(), report)
    }

    fn info(&self, file: &str) -> Value {
        let (code, r) = self.json(&["info", file]);
        assert_eq!(code, 0, "{r}");
        r["files"][0]["result"].clone()
    }

    /// The text of each page, through `export --format txt`.
    fn texts(&self, file: &str) -> Vec<String> {
        let dir = self.path("texts");
        let _ = std::fs::remove_dir_all(&dir);
        let (code, r) = self.json(&[
            "export",
            file,
            "--format",
            "txt",
            "--out-dir",
            dir.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{r}");
        let out = r["files"][0]["outputs"][0].as_str().unwrap();
        let text = std::fs::read_to_string(out).unwrap();
        let mut pages: Vec<String> = text.split('\u{c}').map(String::from).collect();
        assert_eq!(pages.pop().as_deref(), Some(""));
        pages
    }
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn page_size(info: &Value) -> (f64, f64) {
    let s = &info["pageSizes"][0];
    (s["width"].as_f64().unwrap(), s["height"].as_f64().unwrap())
}

#[test]
fn info_reports_each_file() {
    let h = Home::new();
    let a = h.file("a.pdf", &text_pdf(&["A1", "A2", "A3"]));
    let bad = h.file("bad.pdf", b"not a pdf");
    let locked = h.file("locked.pdf", &encrypt(&text_pdf(&["L"]), ""));
    let (code, r) = h.json(&["info", &a, &bad, &locked]);
    assert_eq!(code, 1, "{r}");
    assert_eq!(r["ok"], false);
    let files = r["files"].as_array().unwrap();
    assert_eq!(files[0]["result"]["pages"], 3);
    assert_eq!(page_size(&files[0]["result"]), (200.0, 200.0));
    assert_eq!(files[0]["result"]["encrypted"], false);
    assert_eq!(files[1]["error"]["code"], "damaged-file");
    // Only an owner password: the file opens, and info tells that it is encrypted.
    assert_eq!(files[2]["result"]["encrypted"], true);

    // A user password comes only from the environment.
    let secret = h.file("secret.pdf", &encrypt(&text_pdf(&["S"]), "pw"));
    let (code, r) = h.json(&["info", &secret]);
    assert_eq!(code, 1);
    assert_eq!(r["files"][0]["error"]["code"], "password-required");
    assert!(
        r["files"][0]["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("T_RUSTABLE_PASSWORD")
    );
    let out = h.run_with(&["info", &secret], &[("T_RUSTABLE_PASSWORD", "pw")]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("Pages: 1"), "{}", text(&out));
}

#[test]
fn a_run_removes_only_the_journals_of_stopped_runs() {
    let h = Home::new();
    let a = h.file("a.pdf", &text_pdf(&["A"]));
    let tmp = h.path("tmp");
    let tmp_env = tmp.to_str().unwrap();
    let journal = |name: &str| {
        let dir = tmp.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("original.pdf"), b"%PDF").unwrap();
        std::fs::File::create(dir.join("lock")).unwrap()
    };
    // A killed run leaves its folder, and nothing holds the lock.
    drop(journal("t-rustable-cli-stopped"));
    let busy = journal("t-rustable-cli-busy");
    busy.lock().unwrap();
    let env = [("TMP", tmp_env), ("TEMP", tmp_env), ("TMPDIR", tmp_env)];
    let out = h.run_with(&["info", &a], &env);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!tmp.join("t-rustable-cli-stopped").exists());
    assert!(tmp.join("t-rustable-cli-busy/original.pdf").exists());
}

#[test]
fn merge_takes_page_ranges_in_order() {
    let h = Home::new();
    let a = h.file("a.pdf", &text_pdf(&["A1", "A2", "A3"]));
    let b = h.file("b.pdf", &text_pdf(&["B1", "B2", "B3"]));
    let m = h.path("m.pdf");
    let m = m.to_str().unwrap();
    let (code, r) = h.json(&[
        "merge",
        &format!("{a}:3,1"),
        &format!("{b}:2-"),
        &a,
        "-o",
        m,
    ]);
    assert_eq!(code, 0, "{r}");
    assert_eq!(r["files"][0]["result"]["pages"], 7);
    assert_eq!(h.texts(m), ["A3", "A1", "B2", "B3", "A1", "A2", "A3"]);

    // Refused before any work: the output is an input, or a range is not valid.
    let (code, _) = h.json(&["merge", &a, &b, "-o", &a]);
    assert_eq!(code, 2);
    let (code, _) = h.json(&["merge", &format!("{a}:0"), "-o", m]);
    assert_eq!(code, 2);
    // A page that does not exist, a repeated page of the base, and an encrypted source.
    let (code, r) = h.json(&["merge", &a, &format!("{b}:9"), "-o", m]);
    assert_eq!(
        (code, &r["files"][0]["error"]["code"]),
        (1, &"not-found".into())
    );
    let (code, _) = h.json(&["merge", &format!("{a}:1,1"), "-o", m]);
    assert_eq!(code, 1);
    let locked = h.file("locked.pdf", &encrypt(&text_pdf(&["L"]), ""));
    let (code, r) = h.json(&["merge", &a, &locked, "-o", m]);
    assert_eq!(code, 1);
    assert_eq!(r["files"][0]["error"]["code"], "encrypted-source");

    // A dry run writes nothing.
    let dry = h.path("dry.pdf");
    let (code, _) = h.json(&["merge", &a, &b, "--dry-run", "-o", dry.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(!dry.exists());
}

#[test]
fn export_writes_png_and_text() {
    let h = Home::new();
    let a = h.file("a.pdf", &text_pdf(&["One", "Two", "Three"]));
    let out = h.path("out");
    let (code, r) = h.json(&[
        "export",
        &a,
        "--format",
        "png",
        "--dpi",
        "36",
        "--pages",
        "2-",
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{r}");
    let png = std::fs::read(out.join("a-2.png")).unwrap();
    // 200 points at 36 pixels per inch: 100 pixels.
    assert_eq!(&png[16..24], &[0, 0, 0, 100, 0, 0, 0, 100]);
    assert!(out.join("a-3.png").is_file());
    assert!(!out.join("a-1.png").exists());

    // Golden text: a form feed ends each page.
    h.json(&[
        "export",
        &a,
        "--format",
        "txt",
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert_eq!(
        std::fs::read_to_string(out.join("a.txt")).unwrap(),
        "One\u{c}Two\u{c}Three\u{c}"
    );

    // Two inputs with the same name would write the same files.
    let a2 = h.file("other/a.pdf", &text_pdf(&["X"]));
    let (code, _) = h.json(&[
        "export",
        &a,
        &a2,
        "--format",
        "txt",
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 2);
    // At 600 pixels per inch, page 2 is over the pixel limit of the engine. Page 1 was
    // written first, and the failed file leaves no output.
    let big = h.file(
        "big.pdf",
        &blank_pdf(&[(100.0, 100.0), (14_000.0, 14_000.0)]),
    );
    let big_out = h.path("big");
    let (code, r) = h.json(&[
        "export",
        &big,
        "--format",
        "png",
        "--dpi",
        "600",
        "--out-dir",
        big_out.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{r}");
    assert_eq!(std::fs::read_dir(&big_out).unwrap().count(), 0);
    let (code, r) = h.json(&["export", &a, "--format", "png", "--pages", "5", "--dry-run"]);
    assert_eq!(
        (code, &r["files"][0]["error"]["code"]),
        (1, &"not-found".into())
    );
}

#[test]
fn run_saves_only_where_told() {
    let h = Home::new();
    let original = blank_pdf(&[(100.0, 200.0), (100.0, 200.0)]);
    let a = h.file("a.pdf", &original);
    let rotated = h.path("rotated.pdf");
    let rotate = [
        "run",
        "t-rustable.page-tools:rotate",
        "--args",
        r#"{"degrees": 90}"#,
    ];
    let with = |more: &[&str]| -> Vec<String> {
        rotate.iter().chain(more).map(|s| s.to_string()).collect()
    };
    let call = |args: Vec<String>| {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        h.json(&args)
    };

    let (code, r) = call(with(&[&a, "-o", rotated.to_str().unwrap()]));
    assert_eq!(code, 0, "{r}");
    assert_eq!(r["files"][0]["result"]["pages"], 2);
    assert_eq!(
        page_size(&h.info(rotated.to_str().unwrap())),
        (200.0, 100.0)
    );
    assert_eq!(std::fs::read(&a).unwrap(), original);

    // Never the input without --in-place.
    let (code, _) = call(with(&[&a, "-o", &a]));
    assert_eq!(code, 2);
    let (code, _) = call(with(&[&a, "--out-dir", h.dir.path().to_str().unwrap()]));
    assert_eq!(code, 2);
    assert_eq!(std::fs::read(&a).unwrap(), original);

    // A dry run does the work and writes nothing.
    let (code, r) = call(with(&[&a, "--dry-run"]));
    assert_eq!(code, 0, "{r}");
    assert_eq!(r["dryRun"], true);
    assert_eq!(std::fs::read(&a).unwrap(), original);

    // Another path to the same file is the same file.
    let link = h.path("link.pdf");
    std::fs::hard_link(&a, &link).unwrap();
    let link = link.to_str().unwrap();
    let (code, _) = call(with(&[&a, "-o", link]));
    assert_eq!(code, 2);
    let (code, _) = call(with(&[&a, link, "--in-place"]));
    assert_eq!(code, 2);
    assert_eq!(std::fs::read(&a).unwrap(), original);

    let (code, _) = call(with(&[&a, "--in-place"]));
    assert_eq!(code, 0);
    assert_eq!(page_size(&h.info(&a)), (200.0, 100.0));

    // -o is for one input, and a mod error fails the file.
    let b = h.file("b.pdf", &original);
    let (code, _) = call(with(&[&a, &b, "-o", rotated.to_str().unwrap()]));
    assert_eq!(code, 2);
    let (code, r) = h.json(&["run", "t-rustable.page-tools:delete", &b, "--dry-run"]);
    assert_eq!(
        (code, &r["files"][0]["error"]["code"]),
        (1, &"invalid-argument".into())
    );
}

#[test]
fn a_save_keeps_the_encryption() {
    let h = Home::new();
    let locked = h.file("locked.pdf", &encrypt(&blank_pdf(&[(100.0, 200.0)]), ""));
    let out = h.path("out.pdf");
    let (code, r) = h.json(&[
        "run",
        "t-rustable.page-tools:rotate",
        &locked,
        "--args",
        r#"{"degrees": 90}"#,
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{r}");
    let info = h.info(out.to_str().unwrap());
    assert_eq!(info["encrypted"], true);
    assert_eq!(page_size(&info), (200.0, 100.0));
}

#[test]
fn mods_that_cannot_run_stop_the_command() {
    let h = Home::new();
    let a = h.file("a.pdf", &text_pdf(&["A"]));
    for (target, code) in [
        ("t-rustable.shapes", "no-batch-job"),
        ("com.example.missing", "not-found"),
        ("t-rustable.page-tools:", "invalid-argument"),
    ] {
        let (exit, r) = h.json(&["run", target, &a, "--dry-run"]);
        assert_eq!((exit, &r["error"]["code"]), (2, &code.into()), "{target}");
    }
    // A mod that the user turned off in the app.
    let config = h.path("home/config/mods.json");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, r#"{"disabled": ["t-rustable.page-tools"]}"#).unwrap();
    let (exit, r) = h.json(&["run", "t-rustable.page-tools:rotate", &a, "--dry-run"]);
    assert_eq!((exit, &r["error"]["code"]), (2, &"mod-disabled".into()));
}

#[test]
fn recipe_on_workers_keeps_the_input_order() {
    let h = Home::new();
    let recipe = h.file(
        "r.json",
        br#"{"recipe": 1, "steps": [
            {"run": "t-rustable.page-tools:rotate", "args": {"degrees": 90}},
            {"run": "t-rustable.page-tools:delete", "args": {"pages": [1]}}
        ]}"#,
    );
    let mut inputs = Vec::new();
    for i in 0..4 {
        let bytes = if i == 2 {
            b"damaged".to_vec()
        } else {
            blank_pdf(&[(100.0, 200.0); 3])
        };
        inputs.push(h.file(&format!("in/f{i}.pdf"), &bytes));
    }
    let out = h.path("out");
    let mut args = vec!["recipe", &recipe];
    args.extend(inputs.iter().map(String::as_str));
    args.extend(["--out-dir", out.to_str().unwrap(), "--jobs", "2"]);
    let (code, r) = h.json(&args);
    assert_eq!(code, 1, "{r}");
    let files = r["files"].as_array().unwrap();
    for (i, f) in files.iter().enumerate() {
        assert!(f["input"].as_str().unwrap().ends_with(&format!("f{i}.pdf")));
        assert_eq!(f["ok"], i != 2, "{f}");
    }
    assert_eq!(files[0]["result"][1]["pages"], 2);
    let info = h.info(out.join("f3.pdf").to_str().unwrap());
    assert_eq!(
        (info["pages"].clone(), page_size(&info)),
        (2.into(), (200.0, 100.0))
    );
    assert!(!out.join("f2.pdf").exists());

    // A recipe that does not parse stops the command.
    let bad = h.file(
        "bad.json",
        br#"{"recipe": 1, "steps": [{"run": "x", "arg": {}}]}"#,
    );
    let (code, r) = h.json(&["recipe", &bad, &inputs[0], "--dry-run"]);
    assert_eq!((code, &r["error"]["code"]), (2, &"invalid-recipe".into()));
}
