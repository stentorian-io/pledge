use std::collections::HashSet;

use clap::Parser;
use interlint::acquire::{self, AcquireResult};
use interlint::cli::Cli;
use interlint::{compare, discover, lsp, report};

/// Remove duplicate findings by a 5-tuple key: (rule, file, line, col, message).
///
/// ROB-07: The key was widened from (rule, file, line) to include `col` and `message`.
/// This prevents two findings at the same location but with different messages from being
/// silently collapsed — e.g., overload-removed and param-type-narrowed at the same line.
/// Called twice in each pipeline path: once after merging passes, and once after sorting,
/// to collapse any cross-pass duplicates that the sort may have brought together.
fn dedup_findings(findings: &mut Vec<interlint::types::Finding>) {
    let mut seen = HashSet::new();
    findings.retain(|f| {
        let key = (f.rule, f.file.clone(), f.line, f.col, f.message.clone());
        seen.insert(key)
    });
}

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
    tracing_subscriber::fmt::init();
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

#[cfg(test)]
mod rob_07_tests {
    use interlint::types::{Finding, Severity};
    use std::path::PathBuf;

    fn make_finding(rule: &'static str, line: u32, col: u32, message: &str) -> Finding {
        Finding {
            severity: Severity::Major,
            rule,
            file: PathBuf::from("a.ts"),
            line,
            col,
            message: message.into(),
        }
    }

    #[test]
    fn rob_07_different_message_same_line_preserved() {
        let mut findings = vec![
            make_finding("r", 1, 1, "first"),
            make_finding("r", 1, 1, "second"),
        ];
        super::dedup_findings(&mut findings);
        assert_eq!(
            findings.len(),
            2,
            "ROB-07: same rule/file/line/col but different message must both be retained"
        );
    }

    #[test]
    fn rob_07_identical_fields_deduped() {
        let mut findings = vec![
            make_finding("r", 1, 1, "same"),
            make_finding("r", 1, 1, "same"),
        ];
        super::dedup_findings(&mut findings);
        assert_eq!(
            findings.len(),
            1,
            "ROB-07: identical 5-tuple must be deduped to 1"
        );
    }

    #[test]
    fn rob_07_different_col_preserved() {
        let mut findings = vec![make_finding("r", 1, 1, "m"), make_finding("r", 1, 5, "m")];
        super::dedup_findings(&mut findings);
        assert_eq!(
            findings.len(),
            2,
            "ROB-07: different col must be retained as distinct findings"
        );
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

            let all_warnings: Vec<_> = warnings.into_iter().collect();

            let mut findings = compare::first_publish_findings(&exports);
            // Deduplicate findings from multiple analysis passes (BUG 7)
            dedup_findings(&mut findings);
            findings.sort_by(|a, b| a.severity.cmp(&b.severity));
            // ROB-07: Second dedup pass after sort — collapses cross-pass duplicates
            // that the sort may have brought into adjacency.
            dedup_findings(&mut findings);
            report::print_report(&findings, &all_warnings, false);

            if compare::has_breaking_changes(&findings) {
                Ok(RunOutcome::Breaking)
            } else {
                Ok(RunOutcome::Clean)
            }
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
            let mut findings = compare::compare_exports(&old_exports, &new_exports, &current_root);

            // Step 3b: Metadata comparison (META-01..07)
            let metadata_findings =
                compare::metadata::compare_metadata(&previous_root, &current_root);
            findings.extend(metadata_findings);

            // Step 3c: Exports map diff comparison (EXP-01..13) — per D-03, orchestrated via compare/mod.rs
            let exports_map_findings =
                compare::compare_exports_map_diff(&previous_root, &current_root);
            findings.extend(exports_map_findings);

            // Step 3d: Structural diff — class/enum/generic comparison (CLS-01..09, ENUM-01..05, GEN-01..06)
            let structural_findings = compare::compare_structural_diff(
                &previous_root,
                &current_root,
                &old_exports,
                &new_exports,
            );
            findings.extend(structural_findings);

            // Step 3e: Asset & schema comparison (AST-01..06)
            let asset_findings = compare::compare_asset_diff(&previous_root, &current_root);
            findings.extend(asset_findings);

            // Deduplicate findings from multiple analysis passes (BUG 7)
            dedup_findings(&mut findings);

            // Re-sort after merging and dedup (Major first, then Minor, then Patch — D-01)
            findings.sort_by(|a, b| a.severity.cmp(&b.severity));
            // ROB-07: Second dedup pass after sort — collapses cross-pass duplicates
            // that the sort may have brought into adjacency.
            dedup_findings(&mut findings);

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
