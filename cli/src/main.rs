use anyhow::{bail, Context, Result};
use breakpoint::diff::analyzer;
use breakpoint::diff::printer;
use breakpoint::pkg::registry;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> Result<ExitCode> {
    let start = Instant::now();
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        bail!("Expected only one argument: the package path.");
    }

    let working_dir = args.last().unwrap();
    let working_dir = PathBuf::from(working_dir);

    let pkg_current = registry::load_from_dir(working_dir)
        .with_context(|| "Failed to load current package from file system.")?;

    let pkg_previous = registry::fetch_from_server(&pkg_current)
        .with_context(|| "Failed to fetch previous package from registry server.")?;

    let diff_results = analyzer::get_diff_between(pkg_previous, pkg_current)
        .with_context(|| "Breaking diff analysis between previous & current versions failed.")?;

    printer::print_asset_issues(&diff_results);
    printer::print_entry_issues(&diff_results);

    Ok(printer::print_exit(&diff_results, start))
}
