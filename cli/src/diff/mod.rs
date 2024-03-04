pub mod assets;
pub mod modules;

use crate::diff::assets::{diff_pkg_assets, diff_pkg_entries};
use crate::pkg::Pkg;
use anyhow::Result;

pub fn diff_between(previous: Pkg, current: Pkg) -> Result<u32> {
    let diff_results = vec![
        diff_pkg_assets(&previous, &current)?,
        diff_pkg_entries(&previous.entries.main, &current.entries.main)?,
    ];

    Ok(diff_results.into_iter().fold(0, |sum, count| sum + count))
}
