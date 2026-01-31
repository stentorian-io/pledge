use anyhow::{bail, Context, Result};
use breakpoint::api::{self, ApiClient};
use breakpoint::converter::pkg_to_public_interface;
use breakpoint::pkg::registry;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> Result<ExitCode> {
    let start = Instant::now();
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        bail!("Usage: breakpoint [--previous <path>] <current-path>");
    }

    let (previous_path, current_path) = if args.len() >= 3 && args[0] == "--previous" {
        // Local comparison mode: compare two local directories
        (Some(PathBuf::from(&args[1])), PathBuf::from(&args[2]))
    } else if args.len() == 1 {
        // Registry mode: fetch previous from npm registry
        (None, PathBuf::from(&args[0]))
    } else {
        bail!("Usage: breakpoint [--previous <path>] <current-path>");
    };

    let pkg_current = registry::load_from_dir(current_path)
        .with_context(|| "Failed to load current package from file system.")?;

    let pkg_previous = if let Some(ref prev_path) = previous_path {
        // Load previous from local directory
        println!("Loading previous version from: {}", prev_path.display());
        registry::load_from_dir(prev_path.clone())
            .with_context(|| "Failed to load previous package from file system.")?
    } else {
        // Fetch previous from registry
        registry::fetch_from_server(&pkg_current)
            .with_context(|| "Failed to fetch previous package from registry server.")?
    };

    // Convert packages to PublicInterface format and send to API
    run_api_analysis(&pkg_previous, &pkg_current, start)
}

/// Run analysis using the Interlint API.
fn run_api_analysis(
    pkg_previous: &breakpoint::pkg::Pkg,
    pkg_current: &breakpoint::pkg::Pkg,
    start: Instant,
) -> Result<ExitCode> {
    println!("Converting packages to format...");

    let interface_previous = pkg_to_public_interface(pkg_previous)
        .with_context(|| "Failed to convert previous package to public interface.")?;

    let interface_current = pkg_to_public_interface(pkg_current)
        .with_context(|| "Failed to convert current package to public interface.")?;

    println!(
        "Previous: {} exports across {} modules",
        interface_previous.export_count(),
        interface_previous.modules.len()
    );
    println!(
        "Current: {} exports across {} modules",
        interface_current.export_count(),
        interface_current.modules.len()
    );

    println!("\nSending to Interlint API for analysis...");

    let client = ApiClient::new().with_context(|| "Failed to create API client.")?;

    let response = client
        .compare(interface_previous, interface_current)
        .with_context(|| "API comparison failed.")?;

    // Print results
    api::print_breaking_changes(&response);

    let elapsed = start.elapsed();
    println!("\nAnalysis completed in {:.2?}", elapsed);

    if response.summary.has_breaking_changes {
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}
