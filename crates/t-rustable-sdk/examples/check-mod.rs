//! Checks a built mod against its manifest.
//!
//! Usage: check-mod <manifest.json> <component.wit>
//! Make the .wit file with: wasm-tools component wit mod.wasm > mod.wit

use std::process::ExitCode;
use t_rustable_sdk::manifest::Manifest;
use t_rustable_sdk::permissions::check_imports;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, manifest_path, wit_path] = args.as_slice() else {
        eprintln!("Usage: check-mod <manifest.json> <component.wit>");
        return ExitCode::from(2);
    };
    let manifest =
        match Manifest::parse(&std::fs::read_to_string(manifest_path).expect("read manifest")) {
            Ok(m) => m,
            Err(problems) => {
                problems.iter().for_each(|p| eprintln!("manifest: {p}"));
                return ExitCode::FAILURE;
            }
        };
    let wit = std::fs::read_to_string(wit_path).expect("read wit");
    let imports: Vec<&str> = wit
        .lines()
        .filter_map(|line| line.trim().strip_prefix("import "))
        .map(|rest| rest.trim_end_matches(';').trim())
        .collect();
    let problems = check_imports(imports.iter().copied(), &manifest.permissions);
    if problems.is_empty() {
        println!(
            "{}: imports match the permissions {:?}.",
            manifest.id, manifest.permissions
        );
        ExitCode::SUCCESS
    } else {
        problems.iter().for_each(|p| eprintln!("{p}"));
        ExitCode::FAILURE
    }
}
