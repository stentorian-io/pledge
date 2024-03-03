use crate::ast::{get_items_in_module, parse_esm_module};
use crate::pkg::entries::PkgEntry;
use crate::pkg::Pkg;
use anyhow::{bail, Result};
use std::collections::HashMap;

pub fn diff_between(previous: Pkg, current: Pkg) -> Result<()> {
    let diff_results = vec![
        diff_pkg_assets(&previous, &current)?,
        diff_pkg_entries(&previous.entries.main, &current.entries.main)?,
    ];

    let issue_count = diff_results.into_iter().fold(0, |sum, count| sum + count);

    if issue_count.eq(&1) {
        bail!("Found {issue_count} breaking change.");
    } else if issue_count.gt(&1) {
        bail!("Found {issue_count} breaking changes.");
    } else {
        println!("No breaking changes found!");
    }

    Ok(())
}

fn diff_pkg_assets(previous: &Pkg, current: &Pkg) -> Result<u32> {
    let mut red_flag_count: u32 = 0;

    let previous_assets = previous.contents.asset_list()?;
    let current_assets = current.contents.asset_list()?;

    for missing_asset in previous_assets.difference(&current_assets) {
        red_flag_count += 1;

        println!(
            "BREAKING CHANGE: Asset {} is missing.",
            missing_asset.to_str().unwrap()
        );
    }

    Ok(red_flag_count)
}

fn diff_pkg_entries(
    previous_entries: &HashMap<String, PkgEntry>,
    current_entries: &HashMap<String, PkgEntry>,
) -> Result<u32> {
    let mut red_flag_count: u32 = 0;

    for (previous_entry_name, previous_entry) in previous_entries.iter() {
        let matching_current_entry = current_entries.get(previous_entry_name);

        if matching_current_entry.is_none() {
            red_flag_count += 1;

            println!(
                "BREAKING CHANGE: Entry {} was removed.",
                previous_entry_name
            );

            continue;
        }

        let matching_current_entry = matching_current_entry.unwrap();
        let module_diff_red_flag_count = diff_modules(previous_entry, matching_current_entry)?;

        red_flag_count += module_diff_red_flag_count;
    }

    Ok(red_flag_count)
}

fn diff_modules(previous: &PkgEntry, current: &PkgEntry) -> Result<u32> {
    let previous_module = parse_esm_module(previous)?;
    let current_module = parse_esm_module(current)?;

    let (
        _previous_module_declarations,
        _previous_module_declarations_with_export,
        _previous_module_exports_facade,
        _previous_module_exports_named,
        _previous_module_default_export_declaration,
        _previous_module_default_export_expression,
    ) = get_items_in_module(&previous_module);

    let (
        _current_module_declarations,
        _current_module_declarations_with_export,
        _current_module_exports_facade,
        _current_module_exports_named,
        _current_module_default_export_declaration,
        _current_module_default_export_expression,
    ) = get_items_in_module(&current_module);

    Ok(0)
}
