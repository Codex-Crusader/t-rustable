//! The `t-rustable` command: document commands, batch runs with mods, and mod tools.
//! Exit codes: 0 all files done, 1 at least one file failed, 2 the command cannot start.

mod bridge;
mod docs;
mod mods;
mod new;
mod pages;
mod report;
mod run;
mod workers;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use report::{FileResult, Style};
use run::{Format, Plan, Task};

#[derive(Parser)]
#[command(name = "t-rustable", version = t_rustable_core::VERSION, about = "T-Rustable command line")]
struct Cli {
    /// Print one JSON report on stdout.
    #[arg(long, global = true)]
    json: bool,
    /// Print the log messages and progress of the mods on stderr.
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Top,
}

#[derive(Args)]
struct Jobs {
    /// Worker processes, each with its own engine.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u16).range(1..=64))]
    jobs: u16,
}

/// Exactly one of these.
#[derive(Args)]
#[group(required = true, multiple = false)]
struct OutputArgs {
    /// The output file. Only for one input file.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// The output folder. Each output has the name of its input.
    #[arg(long)]
    out_dir: Option<PathBuf>,
    /// Write the changes into the input files.
    #[arg(long)]
    in_place: bool,
    /// Do all steps, but write no file.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Subcommand)]
enum Top {
    /// Shows the pages, page sizes, and other facts of PDF files.
    Info {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[command(flatten)]
        jobs: Jobs,
    },
    /// Puts the pages of PDF files into one new file. Pick pages with file.pdf:1-3,5,8-
    Merge {
        #[arg(required = true, value_name = "FILE[:PAGES]")]
        inputs: Vec<String>,
        /// The new file.
        #[arg(short, long, required_unless_present = "dry_run")]
        output: Option<PathBuf>,
        /// Do the merge, but write no file.
        #[arg(long)]
        dry_run: bool,
    },
    /// Writes the pages as PNG images or the text as TXT files.
    Export {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long, value_enum)]
        format: Format,
        /// Pixels per inch of the PNG images.
        #[arg(long, default_value_t = 150, value_parser = clap::value_parser!(u32).range(36..=600))]
        dpi: u32,
        /// The pages, for example 1-3,5,8-. The default is all pages.
        #[arg(long)]
        pages: Option<String>,
        /// The output folder.
        #[arg(long, required_unless_present = "dry_run")]
        out_dir: Option<PathBuf>,
        /// Read and render the pages, but write no file.
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        jobs: Jobs,
    },
    /// Runs the batch job of a mod on each file.
    Run {
        /// The mod ID, and optionally the action: <mod-id>[:<action>].
        target: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// The arguments of the batch job, as JSON.
        #[arg(long, conflicts_with = "args_file")]
        args: Option<String>,
        /// A file with the arguments of the batch job, as JSON.
        #[arg(long)]
        args_file: Option<PathBuf>,
        #[command(flatten)]
        out: OutputArgs,
        #[command(flatten)]
        jobs: Jobs,
    },
    /// Runs the steps of a recipe file on each file, then saves each file once.
    Recipe {
        /// The recipe file (JSON).
        recipe: PathBuf,
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[command(flatten)]
        out: OutputArgs,
        #[command(flatten)]
        jobs: Jobs,
    },
    /// List, install, remove, check, make, and pack mods.
    #[command(subcommand)]
    Mods(Mods),
    /// One worker process of --jobs. Not for direct use.
    #[command(hide = true)]
    Worker,
}

#[derive(Subcommand)]
enum Mods {
    /// Lists the built-in and the installed mods.
    List,
    /// Checks a .tmod package, shows its permissions, and installs it.
    Install {
        package: PathBuf,
        /// Install without a question. Read the permissions first.
        #[arg(long)]
        yes: bool,
    },
    /// Removes an installed mod.
    Remove { id: String },
    /// Does the install checks on a .tmod package, or on a mod by its ID.
    Verify {
        #[arg(value_name = "PACKAGE_OR_ID")]
        target: String,
    },
    /// Writes a new mod folder from the template.
    New {
        /// The name that users see, for example "My stamp".
        name: String,
        /// The mod ID. The default is com.example.<name>.
        #[arg(long)]
        id: Option<String>,
        /// The folder to write. The default is the name in lowercase with dashes.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// A local t-rustable-sdk folder. The default is the SDK from the T-Rustable git repository.
        #[arg(long)]
        sdk: Option<PathBuf>,
    },
    /// Checks a built mod folder and writes its .tmod package.
    Pack {
        /// The mod folder, with manifest.json.
        folder: PathBuf,
        /// The package file. The default is <folder>/<id>-<version>.tmod.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// The WebAssembly module or component. The default is the cargo output in the folder.
        #[arg(long)]
        module: Option<PathBuf>,
    },
}

fn pack(folder: PathBuf, output: Option<PathBuf>, module: Option<PathBuf>) -> Result<(), String> {
    let (package, bytes) = t_rustable_tmod::build::pack(&folder, module.as_deref())
        .map_err(|e| format!("{} ({})", e.message, e.code))?;
    let m = &package.manifest;
    let output = output.unwrap_or_else(|| folder.join(format!("{}-{}.tmod", m.id, m.version)));
    std::fs::write(&output, &bytes).map_err(|e| format!("{}: {e}", output.display()))?;
    let permissions: Vec<String> = m.permissions.iter().map(ToString::to_string).collect();
    println!("Wrote {} ({} bytes).", output.display(), bytes.len());
    println!("Mod: {} {} ({})", m.name, m.version, m.id);
    println!(
        "Permissions: {}",
        if permissions.is_empty() {
            "none".into()
        } else {
            permissions.join(", ")
        }
    );
    println!("SHA-256: {}", package.sha256);
    Ok(())
}

/// Prints one result in text mode.
fn print_result(r: &FileResult, style: Style) {
    match &r.result {
        Some(info) if r.ok && style.info => print_info(r, info),
        _ => report::print_text(r, style),
    }
}

fn print_info(r: &FileResult, info: &serde_json::Value) {
    println!("{}", r.input.display());
    let field = |name: &str| info.get(name).cloned().unwrap_or_default();
    println!("  Pages: {}", field("pages"));
    for s in field("pageSizes").as_array().into_iter().flatten() {
        println!(
            "  Page size: {} x {} pt ({} pages)",
            s["width"], s["height"], s["pages"]
        );
    }
    let yes = |v: serde_json::Value| if v == true { "yes" } else { "no" };
    println!("  Page labels: {}", yes(field("pageLabels")));
    println!("  Encrypted: {}", yes(field("encrypted")));
    println!("  Digital signatures: {}", field("signatures"));
    println!("  Form fields: {}", field("formFields"));
    println!("  Outline entries: {}", field("outlineEntries"));
    if let Some(meta) = field("metadata").as_object() {
        for (k, v) in meta {
            if let Some(v) = v.as_str() {
                println!("  {}{}: {v}", k[..1].to_uppercase(), &k[1..]);
            }
        }
    }
    println!("  File size: {} bytes", field("fileSize"));
}

fn document_command(command: Top, json: bool, verbose: bool) -> ExitCode {
    let style = |dry_run, info| Style {
        json,
        dry_run,
        info,
    };
    let start = |task: Task, steps: &[run::Step]| -> Result<Plan, report::Problem> {
        Ok(Plan {
            task,
            mods: docs::resolve_mods(steps)?,
            verbose,
        })
    };
    let edit = |steps: Vec<run::Step>, files: Vec<PathBuf>, out: OutputArgs, jobs: Jobs| {
        let outputs = docs::Outputs {
            output: out.output,
            out_dir: out.out_dir,
            in_place: out.in_place,
        };
        let prepared = start(
            Task::Edit {
                steps: steps.clone(),
            },
            &steps,
        )
        .and_then(|plan| {
            let list = if out.dry_run {
                files
                    .iter()
                    .map(|f| run::Job {
                        input: f.clone(),
                        output: None,
                    })
                    .collect()
            } else {
                docs::edit_jobs(&files, &outputs)?
            };
            Ok((plan, list))
        });
        match prepared {
            Ok((plan, list)) => {
                docs::execute(plan, list, jobs.jobs.into(), style(out.dry_run, false))
            }
            Err(e) => report::usage(&e, json),
        }
    };
    match command {
        Top::Info { files, jobs } => {
            let list = files
                .into_iter()
                .map(|f| run::Job {
                    input: f,
                    output: None,
                })
                .collect();
            match start(Task::Info, &[]) {
                Ok(plan) => docs::execute(plan, list, jobs.jobs.into(), style(false, true)),
                Err(e) => report::usage(&e, json),
            }
        }
        Top::Merge {
            inputs,
            output,
            dry_run,
        } => docs::merge(
            &inputs,
            output.filter(|_| !dry_run).as_deref(),
            style(dry_run, false),
        ),
        Top::Export {
            files,
            format,
            dpi,
            pages,
            out_dir,
            dry_run,
            jobs,
        } => {
            let prepared = pages
                .as_deref()
                .map(str::parse::<pages::Range>)
                .transpose()
                .and_then(|_| docs::export_jobs(&files, out_dir.as_deref().filter(|_| !dry_run)))
                .and_then(|list| {
                    let plan = start(Task::Export { format, dpi, pages }, &[])?;
                    Ok((plan, list))
                });
            match prepared {
                Ok((plan, list)) => {
                    docs::execute(plan, list, jobs.jobs.into(), style(dry_run, false))
                }
                Err(e) => report::usage(&e, json),
            }
        }
        Top::Run {
            target,
            files,
            args,
            args_file,
            out,
            jobs,
        } => match docs::parse_args(args, args_file).and_then(|a| docs::step(&target, a)) {
            Ok(step) => edit(vec![step], files, out, jobs),
            Err(e) => report::usage(&e, json),
        },
        Top::Recipe {
            recipe,
            files,
            out,
            jobs,
        } => match docs::recipe(&recipe) {
            Ok(steps) => edit(steps, files, out, jobs),
            Err(e) => report::usage(&e, json),
        },
        Top::Mods(_) | Top::Worker => unreachable!("handled in main"),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Top::Worker => return workers::worker_main(),
        Top::Mods(Mods::New { name, id, dir, sdk }) => new::run(new::NewMod { name, id, dir, sdk })
            .map(|dir| {
                println!("Wrote {}.", dir.display());
                println!("Build it with build.sh or build.ps1 in that folder.");
            }),
        Top::Mods(Mods::Pack {
            folder,
            output,
            module,
        }) => pack(folder, output, module),
        Top::Mods(Mods::List) => return mods::list(cli.json),
        Top::Mods(Mods::Install { package, yes }) => return mods::install(&package, yes, cli.json),
        Top::Mods(Mods::Remove { id }) => return mods::remove(&id, cli.json),
        Top::Mods(Mods::Verify { target }) => return mods::verify(&target, cli.json),
        command => return document_command(command, cli.json, cli.verbose),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_arguments_are_consistent() {
        Cli::command().debug_assert();
    }
}
