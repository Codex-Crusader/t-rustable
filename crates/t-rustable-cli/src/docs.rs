//! The document commands: the checks before any file, the jobs, and `merge`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;
use serde_json::{Map, Value};
use t_rustable_core::command::Command;
use t_rustable_core::{PageId, SaveOptions};

use crate::pages::{Range, merge_input};
use crate::report::{self, FileResult, Problem, Style};
use crate::run::{Env, Job, ModRef, Plan, Runner, Step, read_text};

fn usage(message: impl Into<String>) -> Problem {
    Problem::new("invalid-argument", message)
}

/// `<mod-id>` or `<mod-id>:<action>`. The action goes into the JSON arguments as
/// `"action"`, the convention of the batch jobs.
pub fn step(target: &str, args: Option<Value>) -> Result<Step, Problem> {
    let (id, action) = match target.split_once(':') {
        Some((id, action)) => (id, Some(action)),
        None => (target, None),
    };
    if id.is_empty() || action == Some("") {
        return Err(usage(format!(
            "{target:?} is not <mod-id> or <mod-id>:<action>."
        )));
    }
    let args = match (action, args) {
        (None, None) => String::new(),
        (None, Some(v)) => v.to_string(),
        (Some(action), args) => {
            let mut map = match args {
                None => Map::new(),
                Some(Value::Object(m)) => m,
                Some(_) => {
                    return Err(usage(
                        "The arguments must be a JSON object when you name an action.",
                    ));
                }
            };
            match map.get("action") {
                Some(Value::String(a)) if a == action => {}
                Some(_) => {
                    return Err(usage(format!(
                        "The arguments name another action than {action:?}."
                    )));
                }
                None => {}
            }
            map.insert("action".into(), action.into());
            Value::Object(map).to_string()
        }
    };
    Ok(Step {
        mod_id: id.into(),
        args,
    })
}

pub fn parse_args(text: Option<String>, file: Option<PathBuf>) -> Result<Option<Value>, Problem> {
    let text = match (text, file) {
        (Some(t), _) => t,
        (None, Some(f)) => read_text(&f)?,
        (None, None) => return Ok(None),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| usage(format!("The arguments are not valid JSON: {e}")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    recipe: u32,
    steps: Vec<RecipeStep>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeStep {
    run: String,
    #[serde(default)]
    args: Option<Value>,
}

/// Reads a recipe: `{"recipe": 1, "steps": [{"run": "<mod-id>:<action>", "args": {}}]}`.
pub fn recipe(path: &Path) -> Result<Vec<Step>, Problem> {
    let text = read_text(path)?;
    let bad = |m: String| {
        Problem::new("invalid-recipe", format!("{}: {m}", path.display()))
            .hint("See docs/cli.md for the recipe format.")
    };
    let r: Recipe = serde_json::from_str(&text).map_err(|e| bad(e.to_string()))?;
    if r.recipe != 1 {
        return Err(bad(format!(
            "Recipe format {} is not known. Use 1.",
            r.recipe
        )));
    }
    if r.steps.is_empty() {
        return Err(bad("The recipe has no steps.".into()));
    }
    r.steps
        .into_iter()
        .enumerate()
        .map(|(k, s)| {
            step(&s.run, s.args).map_err(|e| bad(format!("step {}: {}", k + 1, e.message)))
        })
        .collect()
}

/// Where the files of `run` and `recipe` go.
pub struct Outputs {
    pub output: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub in_place: bool,
}

/// One key for all paths of one file: other case, short names, junctions, hard links.
#[derive(PartialEq, Eq, Hash)]
enum FileKey {
    File(same_file::Handle),
    Path(String),
}

fn key(p: &Path) -> FileKey {
    if let Ok(h) = same_file::Handle::from_path(p) {
        return FileKey::File(h);
    }
    let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let s = p.to_string_lossy().into_owned();
    FileKey::Path(if cfg!(windows) { s.to_lowercase() } else { s })
}

fn same_file(a: &Path, b: &Path) -> bool {
    same_file::is_same_file(a, b).unwrap_or(false)
}

fn create_dir(dir: &Path) -> Result<(), Problem> {
    std::fs::create_dir_all(dir)
        .map_err(|e| Problem::new("io-error", format!("{}: {e}", dir.display())))
}

/// Refuses two jobs with the same output, and an output that is an input file.
fn check_outputs(jobs: &[Job], inputs: &[PathBuf], in_place: bool) -> Result<(), Problem> {
    let mut seen = HashSet::new();
    for o in jobs.iter().filter_map(|j| j.output.as_deref()) {
        if !seen.insert(key(o)) {
            return Err(usage(format!(
                "Two input files have the same output {}. Use files with different names.",
                o.display()
            )));
        }
        // A file that does not exist yet cannot be an input.
        if !in_place && o.exists() && inputs.iter().any(|i| same_file(i, o)) {
            return Err(usage(format!(
                "The output {} is an input file. A command changes its input files only with --in-place.",
                o.display()
            )));
        }
    }
    Ok(())
}

pub fn edit_jobs(files: &[PathBuf], out: &Outputs) -> Result<Vec<Job>, Problem> {
    if out.output.is_some() && files.len() > 1 {
        return Err(usage(
            "-o works with one input file. Use --out-dir for more files.",
        ));
    }
    if let Some(dir) = &out.out_dir {
        create_dir(dir)?;
    }
    let jobs: Vec<Job> = files
        .iter()
        .map(|f| Job {
            input: f.clone(),
            output: if out.in_place {
                Some(f.clone())
            } else if let Some(o) = &out.output {
                Some(o.clone())
            } else {
                out.out_dir
                    .as_ref()
                    .map(|d| d.join(f.file_name().unwrap_or(f.as_os_str())))
            },
        })
        .collect();
    check_outputs(&jobs, files, out.in_place)?;
    Ok(jobs)
}

/// The exported files are `<out-dir>/<name>-<page>.png` or `<out-dir>/<name>.txt`.
pub fn export_jobs(files: &[PathBuf], out_dir: Option<&Path>) -> Result<Vec<Job>, Problem> {
    if let Some(dir) = out_dir {
        create_dir(dir)?;
    }
    let jobs: Vec<Job> = files
        .iter()
        .map(|f| Job {
            input: f.clone(),
            output: out_dir.map(|d| d.join(f.file_stem().unwrap_or(f.as_os_str()))),
        })
        .collect();
    check_outputs(&jobs, files, false)?;
    Ok(jobs)
}

pub fn resolve_mods(steps: &[Step]) -> Result<Vec<ModRef>, Problem> {
    let folders = t_rustable_core::paths::Folders::standard();
    let resources =
        t_rustable_core::paths::Resources::find().map_err(|e| Problem::new("internal", e))?;
    let mut mods: Vec<ModRef> = Vec::new();
    for s in steps {
        if !mods.iter().any(|m| m.id == s.mod_id) {
            mods.push(crate::mods::resolve(&s.mod_id, &folders, &resources)?);
        }
    }
    Ok(mods)
}

/// Runs the jobs in this process, or on `--jobs` worker processes.
pub fn execute(plan: Plan, jobs: Vec<Job>, workers: usize, style: Style) -> ExitCode {
    let print = |r: &FileResult| {
        if !style.json {
            crate::print_result(r, style);
        }
    };
    let results = if workers > 1 && jobs.len() > 1 {
        crate::workers::run_all(&plan, jobs, workers, print)
    } else {
        Runner::new(plan).map(|mut runner| {
            jobs.iter()
                .map(|j| {
                    let r = runner.run(j);
                    print(&r);
                    r
                })
                .collect()
        })
    };
    match results {
        Ok(results) => report::finish(&results, style),
        Err(e) => report::usage(&e, style.json),
    }
}

/// `merge`: the pages of all inputs, in order, in one new file. The first input is the
/// base: its document parts (outline, form, metadata) stay. Pages of the other inputs
/// are copies.
pub fn merge(inputs: &[String], output: Option<&Path>, style: Style) -> ExitCode {
    let parsed: Result<Vec<(PathBuf, Option<Range>)>, Problem> =
        inputs.iter().map(|i| merge_input(i)).collect();
    let parsed = match parsed {
        Ok(p) => p,
        Err(e) => return report::usage(&e, style.json),
    };
    if let Some(o) = output
        && o.exists()
        && parsed.iter().any(|(i, _)| same_file(i, o))
    {
        let e = usage(format!(
            "The output {} is an input file. Write the merged file to a new file.",
            o.display()
        ));
        return report::usage(&e, style.json);
    }
    let env = match Env::start() {
        Ok(env) => env,
        Err(e) => return report::usage(&e, style.json),
    };
    let base = parsed[0].0.clone();
    let result = match merge_into(&env, &parsed, output) {
        Ok(pages) => FileResult {
            input: base,
            ok: true,
            outputs: output.map(Path::to_path_buf).into_iter().collect(),
            result: Some(serde_json::json!({ "pages": pages })),
            ..FileResult::default()
        },
        Err(e) => FileResult::failed(base, e),
    };
    if !style.json {
        crate::print_result(&result, style);
    }
    report::finish(&[result], style)
}

fn merge_into(
    env: &Env,
    inputs: &[(PathBuf, Option<Range>)],
    output: Option<&Path>,
) -> Result<usize, Problem> {
    let (base_path, base_range) = &inputs[0];
    let mut session = env.open(base_path)?;
    let in_file = |p: &Path, e: Problem| Problem {
        message: format!("{}: {}", p.display(), e.message),
        ..e
    };
    let base_ids = session.page_ids();
    let mut commands = Vec::new();
    if let Some(range) = base_range {
        let keep = range
            .indices(base_ids.len())
            .map_err(|e| in_file(base_path, e))?;
        let mut seen = HashSet::new();
        if !keep.iter().all(|i| seen.insert(*i)) {
            return Err(usage(
                "The first file can use each page only once. To use a page again, add the file again as a later input.",
            ));
        }
        for (i, id) in base_ids.iter().enumerate() {
            if !seen.contains(&i) {
                commands.push(Command::DeletePage { page: *id });
            }
        }
        if !keep.is_sorted() {
            for i in &keep {
                commands.push(Command::MovePage {
                    page: base_ids[*i],
                    before: None,
                });
            }
        }
    }
    for (path, range) in &inputs[1..] {
        let bytes = env.read(path)?;
        let summary = t_rustable_core::probe(&env.engine, bytes.clone(), None).map_err(|e| {
            let e = Problem::from(e);
            if e.code == "password-required" {
                Problem::new("encrypted-source", "The file is encrypted.")
            } else {
                e
            }
        });
        let summary = summary.map_err(|e| in_file(path, e))?;
        if summary.encrypted {
            return Err(in_file(
                path,
                Problem::new(
                    "encrypted-source",
                    "The file has a password or restrictions. Merge does not copy pages from it, because the copy would not keep the restrictions.",
                ),
            ));
        }
        let pages = match range {
            Some(r) => r
                .indices(summary.page_count)
                .map_err(|e| in_file(path, e))?,
            None => (0..summary.page_count).collect(),
        };
        let source = session.add_asset(bytes);
        commands.push(Command::InsertPdfPages {
            ids: pages.iter().map(|_| PageId::random()).collect(),
            before: None,
            source,
            pages,
        });
    }
    let mut tx = session.begin("Merge", "cli");
    for c in commands {
        session.add(&mut tx, c)?;
    }
    session.commit(tx)?;
    let pages = session.page_ids().len();
    if let Some(o) = output {
        session.save(o, SaveOptions::default())?;
    }
    session.close()?;
    Ok(pages)
}
