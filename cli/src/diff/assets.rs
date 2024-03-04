use crate::diff::modules::diff_modules;
use crate::pkg::entries::PkgEntry;
use crate::pkg::Pkg;
use std::collections::HashMap;

pub fn diff_pkg_assets(previous: &Pkg, current: &Pkg) -> anyhow::Result<u32> {
    let mut red_flag_count: u32 = 0;

    let previous_assets = previous.contents.asset_list()?;
    let current_assets = current.contents.asset_list()?;

    for missing_asset in previous_assets.difference(&current_assets) {
        red_flag_count += 1;

        println!(
            "BREAKING CHANGE: Asset '{}' is missing.",
            missing_asset.to_str().unwrap()
        );
    }

    Ok(red_flag_count)
}

pub fn diff_pkg_entries(
    previous_entries: &HashMap<String, PkgEntry>,
    current_entries: &HashMap<String, PkgEntry>,
) -> anyhow::Result<u32> {
    let mut red_flag_count: u32 = 0;

    for (previous_entry_name, previous_entry) in previous_entries.iter() {
        let matching_current_entry = current_entries.get(previous_entry_name);

        if matching_current_entry.is_none() {
            red_flag_count += 1;

            println!(
                "BREAKING CHANGE: Entry '{}' was removed.",
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
