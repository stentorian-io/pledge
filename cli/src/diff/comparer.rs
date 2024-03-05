use crate::diff::assets::{diff_pkg_assets, diff_pkg_entries};
use crate::pkg::Pkg;

pub fn count_breaking_changes_between(previous_pkg: Pkg, current_pkg: Pkg) -> anyhow::Result<u32> {
    let previous_contents = previous_pkg.contents;
    let previous_main_entry = previous_pkg.entries.main;

    let current_contents = current_pkg.contents;
    let current_main_entry = current_pkg.entries.main;

    let diff_results = vec![
        diff_pkg_assets(&previous_contents, &current_contents)?,
        diff_pkg_entries(&previous_main_entry, &current_main_entry)?,
    ];

    Ok(diff_results.into_iter().sum::<u32>())
}
