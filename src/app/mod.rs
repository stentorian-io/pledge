//! CLI orchestration and process outcomes for package analysis.

pub mod cli;
pub mod report;

use crate::app::cli::Cli;
use crate::core::findings;
use crate::core::findings::dedup_findings;
use crate::ecosystems::node::acquire::AcquireResult;
use crate::ecosystems::node::compare::assets;
use crate::ecosystems::node::{
    acquire, compare as package_compare, dependencies, discovery as discover,
};
use crate::languages::javascript::compare::structure;
use crate::languages::javascript::{compare, model as js_model};
use crate::languages::typescript::compiler;
use crate::languages::typescript::compiler::toolchain;

async fn enrich_after_preparation(
    exports: &mut js_model::ExportMap,
    root: &std::path::Path,
    preparation: Result<bool, dependencies::DependencyError>,
) -> Result<compiler::EnrichResult, compiler::LspError> {
    match preparation {
        Ok(_) => compiler::enrich_prepared_exports(exports, root).await,
        Err(error) => Ok(compiler::EnrichResult::Degraded(
            compiler::LspError::from(error).to_string(),
        )),
    }
}

/// Outcome of the interlint analysis run.
pub enum RunOutcome {
    /// No breaking changes found — exit 0
    Clean,
    /// Breaking changes found — exit 1
    Breaking,
    /// Analysis completed with incomplete evidence — exit 3
    Degraded,
}

pub async fn run(cli: Cli) -> anyhow::Result<RunOutcome> {
    // Handle --clear-cache before any pipeline work
    if cli.clear_cache {
        toolchain::clear_cache()?;
        println!("Cache cleared.");
        return Ok(RunOutcome::Clean);
    }

    // Validate package directory
    if !cli.path.join("package.json").exists() {
        anyhow::bail!("No package.json found. Run interlint from your package root directory.");
    }

    // Acquire packages with spinner
    let spinner = report::create_spinner("Acquiring packages...");
    let acquire_result = acquire::acquire_packages(&cli.path).await?;
    spinner.finish_and_clear();

    match acquire_result {
        // First-publish case (PKG-03): no previous version exists
        AcquireResult::FirstPublish { current_root, .. } => {
            let spinner = report::create_spinner("Analyzing exports...");
            let preparation = dependencies::prepare_dependencies(&current_root).await;
            let (exports, mut warnings) = discover::discover_exports(&current_root)?;
            if let Err(error) = preparation {
                warnings.push(discover::resolver::ResolveWarning {
                    message: compiler::LspError::from(error).to_string(),
                    incomplete: true,
                    type_expansion_only: false,
                });
            }
            spinner.finish_and_clear();

            let all_warnings: Vec<_> = warnings.into_iter().collect();

            let mut findings = compare::first_publish_findings(&exports);
            // Deduplicate findings from multiple analysis passes.
            dedup_findings(&mut findings);
            findings.sort_by(|a, b| a.severity.cmp(&b.severity));
            // ROB-07: Second dedup pass after sort — collapses cross-pass duplicates
            // that the sort may have brought into adjacency.
            dedup_findings(&mut findings);
            let degraded = all_warnings.iter().any(|warning| warning.incomplete);
            report::print_report(&findings, &all_warnings, degraded);

            if degraded {
                Ok(RunOutcome::Degraded)
            } else if findings::has_breaking_changes(&findings) {
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
            let (old_preparation, new_preparation) = tokio::join!(
                dependencies::prepare_dependencies(&previous_root),
                dependencies::prepare_dependencies(&current_root),
            );
            let (mut new_exports, mut new_warnings) = discover::discover_exports(&current_root)?;
            let (mut old_exports, mut old_warnings) = discover::discover_exports(&previous_root)?;
            spinner.finish_and_clear();

            // Step 2: evaluate both public type surfaces concurrently.
            let spinner = report::create_spinner("Resolving types...");
            let enrich_result = tokio::try_join!(
                enrich_after_preparation(&mut old_exports, &previous_root, old_preparation),
                enrich_after_preparation(&mut new_exports, &current_root, new_preparation),
            );
            spinner.finish_and_clear();

            let (old_enrich, new_enrich) = match enrich_result {
                Ok(pair) => pair,
                Err(e) => {
                    // If LSP fails entirely, treat as degraded
                    let msg = format!("LSP enrichment failed: {e}");
                    (
                        compiler::EnrichResult::Degraded(msg.clone()),
                        compiler::EnrichResult::Degraded(msg),
                    )
                }
            };

            old_enrich.reconcile_discovery_warnings(&mut old_warnings);
            new_enrich.reconcile_discovery_warnings(&mut new_warnings);

            // Determine degradation status
            let degraded = matches!(&old_enrich, compiler::EnrichResult::Degraded(_))
                || matches!(&new_enrich, compiler::EnrichResult::Degraded(_))
                || old_warnings
                    .iter()
                    .chain(&new_warnings)
                    .any(|warning| warning.incomplete);

            if degraded {
                // Print prescriptive warning
                if let compiler::EnrichResult::Degraded(msg) = &old_enrich {
                    eprintln!("warning: {msg}");
                } else if let compiler::EnrichResult::Degraded(msg) = &new_enrich {
                    eprintln!("warning: {msg}");
                }
                eprintln!(
                    "warning: Analysis is incomplete. Available findings are reported, but compatibility could not be established."
                );
            }

            // Step 3: Compare (type-aware when shapes available, fallback to presence-only)
            //
            // INF-04: pass `lsp_was_available = !degraded` so the
            // diff pipeline can choose between Major-verified and
            // Minor-limitation-annotated emission for typeof-import refs.
            let all_warnings: Vec<_> = old_warnings.into_iter().chain(new_warnings).collect();
            let mut findings =
                compare::compare_exports(&old_exports, &new_exports, &current_root, !degraded);

            // Step 3b: Metadata comparison (META-01..07)
            let metadata_findings =
                package_compare::metadata::compare_metadata(&previous_root, &current_root);
            findings.extend(metadata_findings);

            // Compare package exports, imports, and module loading.
            let exports_map_findings =
                package_compare::compare_exports_map_diff(&previous_root, &current_root);
            findings.extend(exports_map_findings);

            // Step 3d: Structural diff — class/enum/generic comparison (CLS-01..09, ENUM-01..05, GEN-01..06)
            let structural_findings = structure::compare_structural_diff(
                &previous_root,
                &current_root,
                &old_exports,
                &new_exports,
            );
            findings.extend(structural_findings);

            // Step 3e: Asset & schema comparison (AST-01..06)
            let asset_findings = assets::compare_asset_diff(&previous_root, &current_root);
            findings.extend(asset_findings);

            // Deduplicate findings from multiple analysis passes.
            dedup_findings(&mut findings);

            // Sort merged findings by severity, with breaking changes first.
            findings.sort_by(|a, b| a.severity.cmp(&b.severity));
            // ROB-07: Second dedup pass after sort — collapses cross-pass duplicates
            // that the sort may have brought into adjacency.
            dedup_findings(&mut findings);

            // Comparison passes can also exhaust a bounded analysis budget.
            let degraded = degraded || findings.iter().any(|f| f.rule == "analysis-incomplete");

            // Step 4: Report
            report::print_report(&findings, &all_warnings, degraded);

            if degraded {
                Ok(RunOutcome::Degraded)
            } else if findings::has_breaking_changes(&findings) {
                Ok(RunOutcome::Breaking)
            } else {
                Ok(RunOutcome::Clean)
            }
        }
    }
}
