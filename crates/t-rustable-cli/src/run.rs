//! Runs one task on each input file. A worker process (`--jobs`) runs the same code.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use t_rustable_core::fonts::Fonts;
use t_rustable_core::paths::Resources;
use t_rustable_core::session::OutlineEntry;
use t_rustable_core::{OpenOptions, SaveOptions, Session};
use t_rustable_engine::{Engine, Priority};
use t_rustable_plugin_host::services::MemorySettings;
use t_rustable_plugin_host::{Limits, ModHost, Services};

use crate::bridge::{CliFiles, CliUi, Notes};
use crate::pages::Range;
use crate::report::{FileResult, Problem};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Png,
    Txt,
}

/// One mod call. `args` is the JSON text for the batch job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub mod_id: String,
    pub args: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Task {
    Info,
    Export {
        format: Format,
        dpi: u32,
        pages: Option<String>,
    },
    /// Runs the steps on one session, then saves once.
    Edit {
        steps: Vec<Step>,
    },
}

/// A mod that the task needs, and its folder.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModRef {
    pub id: String,
    pub dir: PathBuf,
}

/// The same for all files of one command.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub task: Task,
    pub mods: Vec<ModRef>,
    pub verbose: bool,
}

/// One input file. `output` is the file to save, or the path start of the exported
/// files. None for a dry run and for `info`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
}

/// The engine and a private journal folder. The journals of the command line never go
/// to the app's sessions folder, so the app does not offer to recover them.
pub struct Env {
    pub resources: Resources,
    pub engine: Engine,
    sessions: tempfile::TempDir,
    password: Option<String>,
}

impl Env {
    pub fn start() -> Result<Self, Problem> {
        let resources = Resources::find().map_err(|e| Problem::new("internal", e))?;
        let engine = Engine::start(&resources.pdfium)
            .map_err(|e| Problem::new("internal", format!("PDFium did not start: {e}")))?;
        let sessions = tempfile::Builder::new()
            .prefix("t-rustable-cli-")
            .tempdir()
            .map_err(|e| Problem::new("io-error", format!("No temporary folder: {e}")))?;
        Ok(Self {
            resources,
            engine,
            sessions,
            password: std::env::var("T_RUSTABLE_PASSWORD").ok(),
        })
    }

    pub fn read(&self, input: &Path) -> Result<Arc<[u8]>, Problem> {
        std::fs::read(input)
            .map(Arc::from)
            .map_err(|e| Problem::new("io-error", format!("{}: {e}", input.display())))
    }

    pub fn open(&self, input: &Path) -> Result<Session, Problem> {
        let options = OpenOptions {
            sessions_dir: self.sessions.path(),
            password: self.password.as_deref(),
            source_name: input.file_name().map(|n| n.to_string_lossy().into_owned()),
        };
        Ok(Session::open(&self.engine, self.read(input)?, options)?)
    }
}

pub struct Runner {
    env: Env,
    plan: Plan,
    host: Option<ModHost>,
    notes: Arc<Notes>,
}

impl Runner {
    /// Starts the engine and loads the mods of the plan. An error here stops the command.
    pub fn new(plan: Plan) -> Result<Self, Problem> {
        let env = Env::start()?;
        let notes = Arc::new(Notes::new(plan.verbose));
        let host = if plan.mods.is_empty() {
            None
        } else {
            let fonts = Fonts::load(&env.resources.fonts)?;
            let services = Services {
                ui: Arc::new(CliUi(notes.clone())),
                files: Arc::new(CliFiles(notes.clone())),
                settings: Arc::new(MemorySettings::default()),
                fonts: Arc::new(fonts),
            };
            let mut host = ModHost::new(services, Limits::default())?;
            for m in &plan.mods {
                host.load_dir(&m.dir).map_err(|e| {
                    Problem::from(e).hint(format!("The mod is in {}.", m.dir.display()))
                })?;
            }
            Some(host)
        };
        Ok(Self {
            env,
            plan,
            host,
            notes,
        })
    }

    pub fn run(&mut self, job: &Job) -> FileResult {
        let mut r = match self.work(job) {
            Ok(r) => r,
            Err(e) => FileResult::failed(job.input.clone(), e),
        };
        r.notes.extend(self.notes.take());
        r
    }

    fn work(&mut self, job: &Job) -> Result<FileResult, Problem> {
        let session = self.env.open(&job.input)?;
        let doc = Arc::new(Mutex::new(session));
        let result = match &self.plan.task {
            Task::Info => info(&mut doc.lock().unwrap(), &job.input).map(|v| (v, Vec::new())),
            Task::Export { format, dpi, pages } => {
                let pages = pages.as_deref().map(str::parse::<Range>).transpose()?;
                export(&mut doc.lock().unwrap(), job, *format, *dpi, pages.as_ref())
            }
            Task::Edit { steps } => {
                let host = self.host.as_mut().expect("the plan loads the mods");
                edit(host, &doc, steps, job)
            }
        };
        if let Ok(m) = Arc::try_unwrap(doc) {
            let _ = m.into_inner().map(Session::close);
        }
        let (value, outputs) = result?;
        Ok(FileResult {
            input: job.input.clone(),
            ok: true,
            outputs,
            result: (!value.is_null()).then_some(value),
            ..FileResult::default()
        })
    }
}

/// Writes a new file: a temporary file first, then a rename.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Problem> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            Problem::new("io-error", format!("{}: {e}", path.display()))
        })
}

fn round(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn count_outline(entries: &[OutlineEntry]) -> usize {
    entries.iter().map(|e| 1 + count_outline(&e.children)).sum()
}

fn info(s: &mut Session, input: &Path) -> Result<Value, Problem> {
    let ids = s.page_ids();
    // Sizes as the reader sees them: the crop box, turned with the page rotation.
    let mut sizes: Vec<(f64, f64, usize)> = Vec::new();
    let mut labels = false;
    for (i, id) in ids.iter().enumerate() {
        let page = s.page(*id)?;
        let b = page.info.crop_box;
        let (mut w, mut h) = (round(b.right - b.left), round(b.top - b.bottom));
        if page.info.quarter_turns % 2 == 1 {
            (w, h) = (h, w);
        }
        match sizes.iter_mut().find(|(sw, sh, _)| *sw == w && *sh == h) {
            Some(size) => size.2 += 1,
            None => sizes.push((w, h, 1)),
        }
        labels |= page.label != (i + 1).to_string();
    }
    let sizes: Vec<Value> = sizes
        .into_iter()
        .map(|(w, h, n)| json!({ "width": w, "height": h, "pages": n }))
        .collect();
    Ok(json!({
        "pages": ids.len(),
        "pageSizes": sizes,
        "pageLabels": labels,
        "encrypted": s.encrypted(),
        "signatures": s.signature_count(),
        "formFields": s.form_fields()?.len(),
        "outlineEntries": count_outline(&s.outline()?),
        "metadata": s.metadata()?,
        "fileSize": std::fs::metadata(input).map(|m| m.len()).unwrap_or(0),
    }))
}

fn export(
    s: &mut Session,
    job: &Job,
    format: Format,
    dpi: u32,
    pages: Option<&Range>,
) -> Result<(Value, Vec<PathBuf>), Problem> {
    let ids = s.page_ids();
    let indices = match pages {
        Some(r) => r.indices(ids.len())?,
        None => (0..ids.len()).collect(),
    };
    let mut outputs = Vec::new();
    match format {
        Format::Png => {
            let digits = ids.len().to_string().len();
            for i in indices {
                let bitmap = s
                    .render(ids[i], f64::from(dpi) / 72.0, None, Priority::Normal)
                    .map_err(|e| {
                        Problem::from(e).hint(format!("Page {} did not render.", i + 1))
                    })?;
                if let Some(prefix) = &job.output {
                    let mut name = prefix.as_os_str().to_owned();
                    name.push(format!("-{:0digits$}.png", i + 1));
                    let path = PathBuf::from(name);
                    write_file(&path, &bitmap.to_png())?;
                    outputs.push(path);
                }
            }
        }
        Format::Txt => {
            // A form feed ends each page.
            let mut text = String::new();
            for i in indices {
                text.push_str(&s.page_text(ids[i])?);
                text.push('\u{c}');
            }
            if let Some(prefix) = &job.output {
                let mut name = prefix.as_os_str().to_owned();
                name.push(".txt");
                let path = PathBuf::from(name);
                write_file(&path, text.as_bytes())?;
                outputs.push(path);
            }
        }
    }
    Ok((Value::Null, outputs))
}

fn edit(
    host: &mut ModHost,
    doc: &Arc<Mutex<Session>>,
    steps: &[Step],
    job: &Job,
) -> Result<(Value, Vec<PathBuf>), Problem> {
    let mut replies = Vec::new();
    for (k, step) in steps.iter().enumerate() {
        let at = |p: Problem| {
            if steps.len() > 1 {
                let message = format!("Step {} ({}): {}", k + 1, step.mod_id, p.message);
                Problem { message, ..p }
            } else {
                p
            }
        };
        let outcome = host
            .run_batch(
                &step.mod_id,
                doc,
                &step.args,
                Arc::new(AtomicBool::new(false)),
            )
            .map_err(|e| at(e.into()))?;
        let reply = outcome.value.map_err(|e| at(e.into()))?;
        replies.push(serde_json::from_str(&reply).unwrap_or(Value::String(reply)));
    }
    let mut outputs = Vec::new();
    if let Some(output) = &job.output {
        doc.lock().unwrap().save(output, SaveOptions::default())?;
        outputs.push(output.clone());
    }
    let value = if replies.len() == 1 {
        replies.pop().unwrap()
    } else {
        Value::Array(replies)
    };
    Ok((value, outputs))
}
