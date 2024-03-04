use crate::diff::assets::{diff_pkg_assets, diff_pkg_entries};
use crate::pkg::Pkg;

pub fn count_breaking_changes_between(previous: Pkg, current: Pkg) -> anyhow::Result<u32> {
    let diff_results = vec![
        diff_pkg_assets(&previous, &current)?,
        diff_pkg_entries(&previous.entries.main, &current.entries.main)?,
    ];

    Ok(diff_results.into_iter().fold(0, |sum, count| sum + count))
}
