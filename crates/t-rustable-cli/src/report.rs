//! Results of each file, the output in text or JSON, and the exit codes.

use std::path::PathBuf;
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// All files are done.
pub const OK: u8 = 0;
/// At least one file failed. The other files are done.
pub const FAILED: u8 = 1;
/// The command cannot start: bad arguments, a bad recipe, or a mod that cannot run.
pub const USAGE: u8 = 2;

/// Writes log records to stderr: warnings and errors, and with `--verbose` all records.
struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            // A failed mod call can carry names from the mod binary. Keep the terminal safe.
            use t_rustable_plugin_host::services::{MAX_TEXT, printable};
            let level = record.level().as_str().to_ascii_lowercase();
            let message = printable(&record.args().to_string(), 4 * MAX_TEXT);
            eprintln!("{level} [{}] {message}", printable(record.target(), 128));
        }
    }

    fn flush(&self) {}
}

/// Sets the log level. The first call also installs the logger.
pub fn init_logger(verbose: bool) {
    let _ = log::set_logger(&Stderr);
    log::set_max_level(if verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    });
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Problem {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)?;
        if let Some(h) = &self.hint {
            write!(f, " {h}")?;
        }
        Ok(())
    }
}

impl From<t_rustable_core::CoreError> for Problem {
    fn from(e: t_rustable_core::CoreError) -> Self {
        use t_rustable_core::error::code;
        let hint = match e.code {
            code::PASSWORD_REQUIRED => {
                Some("Put the password in the T_RUSTABLE_PASSWORD environment variable.".into())
            }
            code::SIGNED_DOCUMENT => Some(
                "A save breaks the digital signatures. The command line does not do that. Use the app."
                    .into(),
            ),
            _ => e.hint,
        };
        Self {
            code: e.code.into(),
            message: e.message,
            hint,
        }
    }
}

impl From<t_rustable_plugin_host::HostError> for Problem {
    fn from(e: t_rustable_plugin_host::HostError) -> Self {
        Self::new(e.code, e.message)
    }
}

impl From<t_rustable_plugin_host::api::types::ApiError> for Problem {
    fn from(e: t_rustable_plugin_host::api::types::ApiError) -> Self {
        Self {
            code: e.code,
            message: e.message,
            hint: e.hint,
        }
    }
}

/// The result of one input file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FileResult {
    pub input: PathBuf,
    pub ok: bool,
    /// Files that were written. Empty for a dry run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Problem>,
    /// What the command found: the document information, or the replies of the mods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Messages of the mods, and the questions that the command line could not ask.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl FileResult {
    pub fn failed(input: PathBuf, error: Problem) -> Self {
        Self {
            input,
            error: Some(error),
            ..Self::default()
        }
    }
}

/// How to print the results.
#[derive(Clone, Copy)]
pub struct Style {
    pub json: bool,
    pub dry_run: bool,
    /// Print the document information of `info`.
    pub info: bool,
}

/// Prints one result in text mode, as soon as it is known.
pub fn print_text(r: &FileResult, style: Style) {
    let input = r.input.display();
    match &r.error {
        None if style.dry_run => println!("{input}: OK (dry run, nothing written)"),
        None if r.outputs.is_empty() => println!("{input}: OK"),
        None => {
            let outputs: Vec<String> = r.outputs.iter().map(|p| p.display().to_string()).collect();
            if outputs.len() <= 3 {
                println!("{input}: wrote {}", outputs.join(", "));
            } else {
                println!(
                    "{input}: wrote {} files, the first is {}",
                    outputs.len(),
                    outputs[0]
                );
            }
        }
        Some(e) => eprintln!("{input}: {e}"),
    }
    for n in &r.notes {
        eprintln!("{input}: {n}");
    }
}

/// Prints the JSON report for `--json`, and returns the exit code.
pub fn finish(results: &[FileResult], style: Style) -> ExitCode {
    let ok = results.iter().all(|r| r.ok);
    if style.json {
        let report = serde_json::json!({ "ok": ok, "dryRun": style.dry_run, "files": results });
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else if results.len() > 1 {
        let failed = results.iter().filter(|r| !r.ok).count();
        if failed > 0 {
            eprintln!("{failed} of {} files failed.", results.len());
        }
    }
    ExitCode::from(if ok { OK } else { FAILED })
}

/// Prints a problem that stops the command before any file, and returns exit code 2.
pub fn usage(e: &Problem, json: bool) -> ExitCode {
    if json {
        let report = serde_json::json!({ "ok": false, "error": e, "files": [] });
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        eprintln!("error: {e}");
    }
    ExitCode::from(USAGE)
}
