use crate::diff::assets::{diff_pkg_assets, diff_pkg_entries, diff_pkg_entry_exports};
use crate::diff::results::{BreakType, BrokenEntryResult, BrokenExport, DiffResults};
use crate::pkg::contents::PkgContents;
use crate::pkg::entries::{PkgEntry, PkgEntryType};
use crate::pkg::Pkg;
use anyhow::{Context, Result};
use std::collections::HashMap;

pub fn get_diff_between(previous_pkg: Pkg, current_pkg: Pkg) -> Result<DiffResults> {
    let mut diff_report = DiffResults::new();

    analyze_changes_between_contents(
        &mut diff_report,
        &previous_pkg.contents,
        &current_pkg.contents,
    )
    .with_context(|| "Failed to count breaking changes between previous/current contents.")?;

    analyze_changes_between_entries(
        PkgEntryType::Main,
        &mut diff_report,
        &previous_pkg.entries.main,
        &current_pkg.entries.main,
    )
    .with_context(|| "Failed to count breaking changes between previous/current main entries.")?;

    analyze_changes_between_entries(
        PkgEntryType::Browser,
        &mut diff_report,
        &previous_pkg.entries.browser,
        &current_pkg.entries.browser,
    )
    .with_context(|| {
        "Failed to count breaking changes between previous/current browser entries."
    })?;

    analyze_changes_between_entries(
        PkgEntryType::Exports,
        &mut diff_report,
        &previous_pkg.entries.exports,
        &current_pkg.entries.exports,
    )
    .with_context(|| {
        "Failed to count breaking changes between previous/current exports entries."
    })?;

    Ok(diff_report)
}

fn analyze_changes_between_contents(
    diff_results: &mut DiffResults,
    previous_contents: &PkgContents,
    current_contents: &PkgContents,
) -> Result<()> {
    diff_results.removed_assets = diff_pkg_assets(&previous_contents, &current_contents)?;
    Ok(())
}

fn analyze_changes_between_entries(
    entry_type: PkgEntryType,
    diff_results: &mut DiffResults,
    previous_entries: &HashMap<String, PkgEntry>,
    current_entries: &HashMap<String, PkgEntry>,
) -> Result<()> {
    let (missing_entries, matching_entries) = diff_pkg_entries(&previous_entries, &current_entries)
        .with_context(|| "Failed to analyze diff between previous & current entries.")?;

    for missing_entry_name in missing_entries {
        diff_results.broken_entries.push(BrokenEntryResult {
            is_missing: true,
            kind: entry_type.clone(),
            name: missing_entry_name.to_owned(),
            broken_exports: Vec::new(),
        });
    }

    for matching_entry in matching_entries {
        let (entry_name, entries) = matching_entry;
        let (previous_entry, current_entry) = entries;

        let broken_exports = analyze_changes_between_entry_exports(previous_entry, current_entry)
            .with_context(|| {
            format!("Failed to analyze export diff between previous & current entry: {entry_name}")
        })?;

        diff_results.broken_entries.push(BrokenEntryResult {
            is_missing: false,
            kind: entry_type.clone(),
            name: entry_name.to_owned(),
            broken_exports,
        });
    }

    Ok(())
}

fn analyze_changes_between_entry_exports(
    previous_entry: &PkgEntry,
    current_entry: &PkgEntry,
) -> Result<Vec<BrokenExport>> {
    let mut broken_exports: Vec<BrokenExport> = Vec::new();

    let (
        is_default_export_missing,
        matching_default_export,
        missing_named_exports,
        matching_named_exports,
    ) = diff_pkg_entry_exports(previous_entry, current_entry)?;

    if is_default_export_missing {
        broken_exports.push((String::from("Default export"), BreakType::Removed));
    } else if matching_default_export.is_some() {
        todo!("handle matching default export breaking diff analysis")
    }

    for missing_export_name in missing_named_exports {
        broken_exports.push((
            format!("Named export '{missing_export_name}'"),
            BreakType::RemovedOrRenamed,
        ));
    }

    for _named_export in matching_named_exports {
        todo!("handle matching named export breaking diff analysis")
    }

    Ok(broken_exports)
}
