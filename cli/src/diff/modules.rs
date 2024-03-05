use crate::ecma::parser::parse_pkg_entry;
use crate::ecma::walker::get_exports_in_module;
use crate::pkg::entries::PkgEntry;
use anyhow::Result;

pub fn diff_modules(previous_entry: &PkgEntry, current_entry: &PkgEntry) -> Result<u32> {
    let mut red_flag_count: u32 = 0;

    let previous_module = parse_pkg_entry(previous_entry)?;
    let current_module = parse_pkg_entry(current_entry)?;

    let (previous_default_export, previous_named_exports) =
        get_exports_in_module(previous_entry.dir_path(), previous_module)?;

    let (current_default_export, current_named_exports) =
        get_exports_in_module(current_entry.dir_path(), current_module)?;

    if previous_default_export.is_some() && current_default_export.is_none() {
        red_flag_count += 1;

        println!(
            "BREAKING CHANGE: Default export in '{}' was removed.",
            previous_entry.name
        );
    }

    for (previous_export_name, _) in previous_named_exports.iter() {
        if !current_named_exports.contains_key(previous_export_name) {
            red_flag_count += 1;

            println!(
                "BREAKING CHANGE: Named export '{}' in '{}' was removed or renamed.",
                previous_export_name, previous_entry.name
            );
        }
    }

    Ok(red_flag_count)
}
