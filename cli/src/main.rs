use anyhow::{bail, Result};
use breakpoint::diff::comparer;
use breakpoint::pkg::registry;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> Result<ExitCode> {
    let start_timestamp = Instant::now();
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        bail!("Expected only one argument: the package path.");
    }

    let working_dir = args.last().unwrap();
    let working_dir = PathBuf::from(working_dir);

    let pkg_current = registry::load_from_dir(working_dir)?;
    let pkg_previous = registry::fetch_from_server(&pkg_current)?;

    let diff_result = comparer::count_breaking_changes_between(pkg_previous, pkg_current)?;

    diff_result.print_asset_issues();
    diff_result.print_entry_issues();

    Ok(diff_result.print_conclusion(start_timestamp))
}
