//! The `t-rustable` command. Milestone 5 has the commands for mod authors. The document
//! commands come in Milestone 6.

mod new;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "t-rustable", version = t_rustable_core::VERSION, about = "T-Rustable command line")]
struct Cli {
    #[command(subcommand)]
    command: Top,
}

#[derive(Subcommand)]
enum Top {
    /// Make and pack mods.
    #[command(subcommand)]
    Mods(Mods),
}

#[derive(Subcommand)]
enum Mods {
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
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
