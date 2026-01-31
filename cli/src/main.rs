use anyhow::{bail, Context, Result};
use breakpoint::api::{self, ApiClient};
use breakpoint::converter::pkg_to_public_interface;
use breakpoint::pkg::registry;
use breakpoint::pkg::registry_client::RegistryClient;
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

    // Load current package using native pack command
    println!("Loading current package...");
    let packed_current = registry::load_from_dir_with_native_pack(current_path.clone())
        .with_context(|| "Failed to load and pack current package.")?;

    let pkg_name = packed_current.name.clone();
    let pkg_version = packed_current.version.clone();

    // Convert to Pkg for API compatibility
    let pkg_current = packed_current
        .to_pkg()
        .with_context(|| "Failed to convert packed package.")?;

    // Handle previous version
    let pkg_previous = if let Some(ref prev_path) = previous_path {
        // Local comparison mode: load previous from local directory using native pack
        println!("Loading previous version from: {}", prev_path.display());
        let packed_previous = registry::load_from_dir_with_native_pack(prev_path.clone())
            .with_context(|| "Failed to load and pack previous package.")?;
        Some(
            packed_previous
                .to_pkg()
                .with_context(|| "Failed to convert packed previous package.")?,
        )
    } else {
        // Registry mode: check if package exists, then fetch
        println!("Checking registry for previous version of '{}'...", pkg_name);

        let client = RegistryClient::from_pkg_dir(&current_path)
            .with_context(|| "Failed to create registry client.")?;

        match client.package_exists(&pkg_name) {
            Ok(true) => {
                // Package exists, fetch the latest version
                println!("Found package in registry, fetching latest version...");
                let pkg = registry::fetch_previous_version(&pkg_current)
                    .with_context(|| "Failed to fetch previous package from registry.")?;
                Some(pkg)
            }
            Ok(false) => {
                // Package doesn't exist - first publish
                println!();
                println!("⚠️  No previous version found for '{}'.", pkg_name);
                println!("   This appears to be the first publish of this package.");
                println!("   Skipping breaking change comparison.");
                println!();
                None
            }
            Err(e) => {
                // Network error or other issue
                eprintln!("Warning: Could not check registry: {}", e);
                eprintln!("Skipping breaking change comparison.");
                None
            }
        }
    };

    // Run analysis if we have a previous version to compare against
    match pkg_previous {
        Some(previous) => run_api_analysis(&previous, &pkg_current, start),
        None => {
            // First publish - just analyze current package
            run_first_publish_analysis(&pkg_current, &pkg_name, &pkg_version, start)
        }
    }
}

/// Run analysis for first-time publish (no previous version to compare).
fn run_first_publish_analysis(
    pkg_current: &breakpoint::pkg::Pkg,
    pkg_name: &str,
    pkg_version: &str,
    start: Instant,
) -> Result<ExitCode> {
    println!("Analyzing package: {} v{}", pkg_name, pkg_version);

    let interface_current = pkg_to_public_interface(pkg_current)
        .with_context(|| "Failed to convert current package to public interface.")?;

    println!(
        "Package has {} exports across {} modules",
        interface_current.export_count(),
        interface_current.modules.len()
    );

    let elapsed = start.elapsed();
    println!("\nAnalysis completed in {:.2?}", elapsed);
    println!("\n✅ Ready to publish {} v{}", pkg_name, pkg_version);

    Ok(ExitCode::SUCCESS)
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
