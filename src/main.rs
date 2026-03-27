use clap::Parser;
use interlint::acquire::{self, AcquireResult};
use interlint::cli::Cli;
use interlint::{compare, discover, report};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match run(cli).await {
        Ok(true) => std::process::exit(1),
        Ok(false) => std::process::exit(0),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<bool> {
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
            Ok(false)
        }

        // Normal flow: compare current against previous version
        AcquireResult::BothVersions {
            current_root,
            previous_root,
            ..
        } => {
            let spinner = report::create_spinner("Analyzing exports...");
            let (new_exports, new_warnings) = discover::discover_exports(&current_root)?;
            let (old_exports, old_warnings) = discover::discover_exports(&previous_root)?;
            spinner.finish_and_clear();

            let all_warnings: Vec<_> = old_warnings.into_iter().chain(new_warnings).collect();

            let findings = compare::compare_exports(&old_exports, &new_exports);
            report::print_report(&findings, &all_warnings);

            Ok(compare::has_breaking_changes(&findings))
        }
    }
}
