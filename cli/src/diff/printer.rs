use crate::diff::results::DiffResults;
use crate::pkg::entries::PkgEntryType;
use std::process::ExitCode;
use std::time::Instant;

const TERM_STYLE_BOLD: &str = "\x1b[1m";
const TERM_STYLE_RED: &str = "\x1b[31m";
const TERM_STYLE_RESET: &str = "\x1b[0m";

pub fn print_asset_issues(diff_results: &DiffResults) {
    if !diff_results.removed_assets.is_empty() {
        print_breaking_change_tally_header(
            &diff_results.removed_assets.len(),
            "to assets:".into(),
            true,
        );

        for missing_asset_path in diff_results.removed_assets.iter() {
            println!("  - {} was removed.", missing_asset_path.display())
        }
    }
}

pub fn print_entry_issues(diff_results: &DiffResults) {
    for entry in diff_results.broken_entries.iter() {
        let entry_issue_count = entry.issue_count();

        if entry_issue_count.eq(&0) {
            continue;
        }

        match &entry.kind {
            PkgEntryType::Main => print_breaking_change_tally_header(
                &entry_issue_count,
                format!("to {} entry:", entry.kind),
                true,
            ),
            entry_kind => print_breaking_change_tally_header(
                &entry_issue_count,
                format!("to {} entry {}:", entry_kind, entry.name),
                true,
            ),
        }

        if entry.is_missing {
            println!("  - was removed.",)
        } else {
            for (export_name, break_type) in entry.broken_exports.iter() {
                println!("  - {export_name} was {break_type}.",)
            }
        }
    }
}

pub fn print_exit(diff_results: &DiffResults, start_timestamp: Instant) -> ExitCode {
    let issue_count = diff_results.issue_count();
    let elapsed_time = start_timestamp.elapsed().as_secs_f32();
    let is_error = issue_count.gt(&0);

    print_breaking_change_tally_header(&issue_count, format!("in {elapsed_time:.2}s."), is_error);

    if is_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_breaking_change_tally_header(issue_count: &usize, suffix: String, is_error: bool) {
    let prefix = if issue_count.eq(&0) {
        format!("Found {issue_count} breaking change")
    } else {
        format!("Found {issue_count} breaking changes")
    };

    let prefix = if is_error {
        format!("{TERM_STYLE_RED}{prefix}")
    } else {
        prefix
    };

    println!("{TERM_STYLE_BOLD}\n{prefix} {suffix}{TERM_STYLE_RESET}");
}
