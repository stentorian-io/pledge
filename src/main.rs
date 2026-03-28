use clap::Parser;
use interlint::acquire::{self, AcquireResult};
use interlint::cli::Cli;
use interlint::{compare, discover, lsp, report};

/// Outcome of the interlint analysis run.
enum RunOutcome {
    /// No breaking changes found — exit 0
    Clean,
    /// Breaking changes found — exit 1
    Breaking,
    /// Analysis completed but LSP was unavailable — exit 3
    Degraded,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match run(cli).await {
        Ok(RunOutcome::Clean) => std::process::exit(0),
        Ok(RunOutcome::Breaking) => std::process::exit(1),
        Ok(RunOutcome::Degraded) => std::process::exit(3),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<RunOutcome> {
    // Handle --clear-cache before any pipeline work (D-21)
    if cli.clear_cache {
        lsp::managed::clear_cache()?;
        println!("Cache cleared.");
        return Ok(RunOutcome::Clean);
    }

    // Validate package directory (D-08)
    if !cli.path.join("package.json").exists() {
        anyhow::bail!("No package.json found. Run interlint from your package root directory.");
    }

    // Acquire packages with spinner (D-05)
    let spinner = report::create_spinner("Acquiring packages...");
    let acquire_result = acquire::acquire_packages(&cli.path).await?;
    spinner.finish_and_clear();

    match acquire_result {
        // First-publish case (PKG-03): no previous version exists
        AcquireResult::FirstPublish { current_root, .. } => {
            let spinner = report::create_spinner("Analyzing exports...");
            let (exports, warnings) = discover::discover_exports(&current_root)?;
            spinner.finish_and_clear();

            for w in &warnings {
                eprintln!("warning: {}", w.message);
            }
            report::print_first_publish_report(&exports);

            // No breaking changes on first publish
            Ok(RunOutcome::Clean)
        }

        // Normal flow: compare current against previous version
        AcquireResult::BothVersions {
            current_root,
            previous_root,
            ..
        } => {
            // Step 1: Discover exports
            let spinner = report::create_spinner("Analyzing exports...");
            let (mut new_exports, new_warnings) = discover::discover_exports(&current_root)?;
            let (mut old_exports, old_warnings) = discover::discover_exports(&previous_root)?;
            spinner.finish_and_clear();

            // Step 2: LSP enrichment (D-07: two servers in parallel)
            let spinner = report::create_spinner("Resolving types...");
            let enrich_result = tokio::try_join!(
                lsp::enrich_exports(&mut old_exports, &previous_root),
                lsp::enrich_exports(&mut new_exports, &current_root),
            );
            spinner.finish_and_clear();

            let (old_enrich, new_enrich) = match enrich_result {
                Ok(pair) => pair,
                Err(e) => {
                    // If LSP fails entirely, treat as degraded
                    let msg = format!("LSP enrichment failed: {e}");
                    (
                        lsp::EnrichResult::Degraded(msg.clone()),
                        lsp::EnrichResult::Degraded(msg),
                    )
                }
            };

            // Determine degradation status
            let degraded = matches!(&old_enrich, lsp::EnrichResult::Degraded(_))
                || matches!(&new_enrich, lsp::EnrichResult::Degraded(_));

            if degraded {
                // Print prescriptive warning (D-16)
                if let lsp::EnrichResult::Degraded(msg) = &old_enrich {
                    eprintln!("warning: {msg}");
                } else if let lsp::EnrichResult::Degraded(msg) = &new_enrich {
                    eprintln!("warning: {msg}");
                }
                eprintln!(
                    "warning: Continuing with structural analysis only. Type-level changes will not be detected."
                );
            }

            // Step 3: Compare (type-aware when shapes available, fallback to presence-only)
            let all_warnings: Vec<_> = old_warnings.into_iter().chain(new_warnings).collect();
            let findings = compare::compare_exports(&old_exports, &new_exports);

            // Step 4: Report
            report::print_report(&findings, &all_warnings, degraded);

            if degraded {
                Ok(RunOutcome::Degraded)
            } else if compare::has_breaking_changes(&findings) {
                Ok(RunOutcome::Breaking)
            } else {
                Ok(RunOutcome::Clean)
            }
        }
    }
}
