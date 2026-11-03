//! CLI orchestration and process outcomes for package analysis.

pub mod cli;
pub mod report;

use crate::app::cli::Cli;
use crate::core::findings::{self, dedup_findings, Finding};
use crate::ecosystems::node::compare::assets;
use crate::ecosystems::node::discovery::resolver::ResolveWarning;
use crate::ecosystems::node::{acquire, compare as package_compare, dependencies, discovery};
use crate::languages::javascript::compare::structure;
use crate::languages::javascript::{compare, model as js_model};
use crate::languages::typescript::compiler::{self, toolchain};
use anyhow::Context;
use std::path::Path;

/// Outcome of the pledge analysis run. Incomplete evidence takes precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Clean,
    Breaking,
    Degraded,
}

impl RunOutcome {
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Clean => 0,
            Self::Breaking => 1,
            Self::Degraded => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Breaking => "breaking",
            Self::Degraded => "incomplete",
        }
    }
}

async fn enrich_after_preparation(
    exports: &mut js_model::ExportMap,
    root: &Path,
    workspace: &Path,
    preparation: Result<bool, dependencies::DependencyError>,
) -> compiler::EnrichResult {
    match preparation {
        Ok(_) => compiler::enrich_prepared_exports_from_workspace(exports, root, workspace)
            .await
            .unwrap_or_else(|error| compiler::EnrichResult::Degraded(error.to_string())),
        Err(error) => compiler::EnrichResult::Degraded(error.to_string()),
    }
}

fn reconcile_warnings(
    enrichment: compiler::EnrichResult,
    warnings: &mut Vec<ResolveWarning>,
    side: &str,
) {
    enrichment.reconcile_discovery_warnings(warnings);
    if let compiler::EnrichResult::Degraded(message) = enrichment {
        warnings.push(ResolveWarning {
            message,
            incomplete: true,
            type_expansion_only: false,
        });
    }
    for warning in warnings {
        warning.message = format!("{side}: {}", warning.message);
    }
}

pub async fn run(cli: Cli) -> anyhow::Result<RunOutcome> {
    if cli.clear_cache {
        toolchain::clear_cache()?;
        report::print_cache_cleared(cli.format)?;
        return Ok(RunOutcome::Clean);
    }
    let workspace = cli
        .path
        .canonicalize()
        .with_context(|| format!("Cannot open package directory {}", cli.path.display()))?;
    let spinner = report::create_spinner(cli.format, "Acquiring packages...");
    let packages = acquire::acquire_packages(&workspace, cli.against.as_deref()).await?;
    spinner.finish_and_clear();
    let current_root = &packages.current_root;

    let (mut findings, mut warnings) = if let Some(previous_root) = &packages.previous_root {
        let spinner = report::create_spinner(cli.format, "Preparing dependencies and exports...");
        let (old_preparation, new_preparation) = tokio::join!(
            dependencies::prepare_dependencies(previous_root),
            dependencies::prepare_dependencies(current_root),
        );
        let (mut old_exports, mut old_warnings) = discovery::discover_exports(previous_root)?;
        let (mut new_exports, mut new_warnings) = discovery::discover_exports(current_root)?;
        spinner.finish_and_clear();

        let spinner = report::create_spinner(cli.format, "Resolving types...");
        let (old_enrichment, new_enrichment) = tokio::join!(
            enrich_after_preparation(&mut old_exports, previous_root, &workspace, old_preparation),
            enrich_after_preparation(&mut new_exports, current_root, &workspace, new_preparation),
        );
        spinner.finish_and_clear();
        reconcile_warnings(old_enrichment, &mut old_warnings, "baseline");
        reconcile_warnings(new_enrichment, &mut new_warnings, "current");
        let warnings: Vec<_> = old_warnings.into_iter().chain(new_warnings).collect();
        let complete = !warnings.iter().any(|warning| warning.incomplete);
        let mut findings =
            compare::compare_exports(&old_exports, &new_exports, current_root, complete);
        findings.extend(package_compare::metadata::compare_metadata(
            previous_root,
            current_root,
        ));
        findings.extend(package_compare::compare_exports_map_diff(
            previous_root,
            current_root,
        ));
        findings.extend(structure::compare_structural_diff(
            previous_root,
            current_root,
            &old_exports,
            &new_exports,
        ));
        findings.extend(assets::compare_asset_diff(previous_root, current_root));
        (findings, warnings)
    } else {
        let spinner = report::create_spinner(cli.format, "Analyzing first-publish exports...");
        let preparation = dependencies::prepare_dependencies(current_root).await;
        let (exports, mut warnings) = discovery::discover_exports(current_root)?;
        if let Err(error) = preparation {
            warnings.push(ResolveWarning {
                message: error.to_string(),
                incomplete: true,
                type_expansion_only: false,
            });
        }
        spinner.finish_and_clear();
        (compare::first_publish_findings(&exports), warnings)
    };

    // Stable ordering makes CI artifacts comparable despite hash-map traversal.
    for finding in &mut findings {
        for root in std::iter::once(current_root).chain(packages.previous_root.iter()) {
            if let Ok(relative) = finding.file.strip_prefix(root) {
                finding.file = relative.to_path_buf();
                break;
            }
        }
    }
    dedup_findings(&mut findings);
    findings.sort_by(|left, right| finding_order(left).cmp(&finding_order(right)));
    warnings.sort_by(|left, right| left.message.cmp(&right.message));
    warnings.dedup_by(|left, right| {
        left.message == right.message && left.incomplete == right.incomplete
    });
    let incomplete = warnings.iter().any(|warning| warning.incomplete)
        || findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete");
    let outcome = if incomplete {
        RunOutcome::Degraded
    } else if findings::has_breaking_changes(&findings) {
        RunOutcome::Breaking
    } else {
        RunOutcome::Clean
    };
    report::print_report(
        cli.format,
        cli.color,
        &packages.comparison,
        &findings,
        &warnings,
        outcome,
    )?;
    Ok(outcome)
}

fn finding_order(finding: &Finding) -> (&findings::Severity, &Path, u32, u32, &str, &str) {
    (
        &finding.severity,
        &finding.file,
        finding.line,
        finding.col,
        finding.rule,
        &finding.message,
    )
}
