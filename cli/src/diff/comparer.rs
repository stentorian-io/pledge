use crate::diff::assets::{diff_pkg_assets, diff_pkg_entries};
use crate::pkg::Pkg;

pub fn count_breaking_changes_between(previous_pkg: Pkg, current_pkg: Pkg) -> anyhow::Result<u32> {
    let diff_results = vec![
        diff_pkg_assets(&previous_pkg.contents, &current_pkg.contents)?,
        diff_pkg_entries(&previous_pkg.entries.main, &current_pkg.entries.main)?,
        diff_pkg_entries(&previous_pkg.entries.browser, &current_pkg.entries.browser)?,
        diff_pkg_entries(&previous_pkg.entries.exports, &current_pkg.entries.exports)?,
    ];

    Ok(diff_results.into_iter().sum::<u32>())
}
