use crate::ast::{get_items_in_module, parse_esm_module};
use crate::pkg::Pkg;
use anyhow::{bail, Result};
use std::path::PathBuf;

pub fn diff_between(previous: Pkg, current: Pkg) -> Result<()> {
    let diff_results = vec![
        diff_pkg_contents(&previous, &current),
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

fn diff_pkg_contents(previous: &Pkg, current: &Pkg) -> u32 {
    let mut red_flag_count: u32 = 0;

    let previous_files = &previous.contents.resolved_files;
    let current_files = &current.contents.resolved_files;

    for missing_file_path in previous_files.difference(current_files) {
        red_flag_count += 1;

        println!(
            "BREAKING CHANGE: File {} was removed.",
            missing_file_path.to_str().unwrap()
        );
    }

    red_flag_count
}

fn diff_pkg_entries(previous: &PathBuf, current: &PathBuf) -> Result<u32> {
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
