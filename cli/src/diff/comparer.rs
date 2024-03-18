use crate::diff::assets::{diff_pkg_assets, diff_pkg_entries, diff_pkg_entry_exports};
use crate::diff::results::{BreakType, BrokenEntry, BrokenExport, DiffResults};
use crate::pkg::contents::PkgContents;
use crate::pkg::entries::{PkgEntry, PkgEntryType};
use crate::pkg::Pkg;
use anyhow::Result;
use std::collections::HashMap;

pub fn count_breaking_changes_between(previous_pkg: Pkg, current_pkg: Pkg) -> Result<DiffResults> {
    let mut diff_report = DiffResults::new();

    count_breaking_changes_between_contents(
        &mut diff_report,
        &previous_pkg.contents,
        &current_pkg.contents,
    )?;

    count_breaking_changes_between_entries(
        PkgEntryType::Main,
        &mut diff_report,
        &previous_pkg.entries.main,
        &current_pkg.entries.main,
    )?;

    count_breaking_changes_between_entries(
        PkgEntryType::Browser,
        &mut diff_report,
        &previous_pkg.entries.browser,
        &current_pkg.entries.browser,
    )?;

    count_breaking_changes_between_entries(
        PkgEntryType::Exports,
        &mut diff_report,
        &previous_pkg.entries.exports,
        &current_pkg.entries.exports,
    )?;

    Ok(diff_report)
}

fn count_breaking_changes_between_contents(
    diff_results: &mut DiffResults,
    previous_contents: &PkgContents,
    current_contents: &PkgContents,
) -> Result<()> {
    let missing_assets = diff_pkg_assets(&previous_contents, &current_contents)?;
    diff_results.removed_assets = missing_assets;

    Ok(())
}

fn count_breaking_changes_between_entries(
    entry_type: PkgEntryType,
    diff_results: &mut DiffResults,
    previous_entries: &HashMap<String, PkgEntry>,
    current_entries: &HashMap<String, PkgEntry>,
) -> Result<()> {
    let (missing_entries, matching_entries) =
        diff_pkg_entries(&previous_entries, &current_entries)?;

    for missing_entry_name in missing_entries {
        diff_results.broken_entries.push(BrokenEntry {
            is_missing: true,
            kind: entry_type.clone(),
            name: missing_entry_name.to_owned(),
            broken_exports: Vec::new(),
        });
    }

    for matching_entry in matching_entries {
        let (entry_name, entries) = matching_entry;
        let (previous_entry, current_entry) = entries;

        let broken_exports =
            count_breaking_changes_between_entry_exports(previous_entry, current_entry)?;

        diff_results.broken_entries.push(BrokenEntry {
            is_missing: false,
            kind: entry_type.clone(),
            name: entry_name.to_owned(),
            broken_exports,
        });
    }

    Ok(())
}

fn count_breaking_changes_between_entry_exports(
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
        todo!()
    }

    for missing_export_name in missing_named_exports {
        broken_exports.push((
            format!("Named export '{missing_export_name}'"),
            BreakType::RemovedOrRenamed,
        ));
    }

    for _named_export in matching_named_exports {
        todo!()
    }

    Ok(broken_exports)
}
