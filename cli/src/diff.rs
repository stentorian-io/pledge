use crate::pkg::Pkg;
use anyhow::{bail, Result};

pub fn diff(a: Pkg, b: Pkg) -> Result<()> {
    let diff_results = vec![diff_pkg_files(&a, &b)];
    let issue_count = diff_results.into_iter().fold(0, |sum, count| sum + count);

    if issue_count.eq(&1) {
        bail!("Found {issue_count} breaking change.");
    } else if issue_count.gt(&1) {
        bail!("Found {issue_count} breaking changes.");
    }

    Ok(())
}

fn diff_pkg_files(a: &Pkg, b: &Pkg) -> u32 {
    let mut red_flag_count: u32 = 0;

    let a_files = &a.contents.resolved_files;
    let b_files = &b.contents.resolved_files;

    for missing_file_path in a_files.difference(b_files) {
        red_flag_count += 1;

        println!(
            "BREAKING CHANGE: File {} was removed in current version.",
            missing_file_path.to_str().unwrap()
        );
    }

    red_flag_count
}
