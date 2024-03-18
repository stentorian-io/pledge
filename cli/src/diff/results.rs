use crate::pkg::entries::PkgEntryType;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;
use strum_macros::Display;

type ExportName = String;
pub type BrokenExport = (ExportName, BreakType);

#[derive(Display, Debug)]
pub enum BreakType {
    #[strum(serialize = "removed")]
    Removed,
    #[strum(serialize = "removed or renamed")]
    RemovedOrRenamed,
}

pub struct BrokenEntry {
    pub kind: PkgEntryType,
    pub name: String,
    pub is_missing: bool,
    pub broken_exports: Vec<BrokenExport>,
}

impl BrokenEntry {
    pub fn issue_count(&self) -> usize {
        if self.is_missing {
            1
        } else {
            self.broken_exports.len()
        }
    }
}

pub struct DiffResults {
    pub removed_assets: Vec<PathBuf>,
    pub broken_entries: Vec<BrokenEntry>,
}

impl DiffResults {
    pub fn new() -> Self {
        Self {
            removed_assets: Vec::new(),
            broken_entries: Vec::new(),
        }
    }

    pub fn print_asset_issues(&self) {
        if !self.removed_assets.is_empty() {
            println!(
                "\n{} to assets:",
                Self::get_breaking_change_count_prefix(&self.removed_assets.len()),
            );

            for missing_asset_path in self.removed_assets.iter() {
                println!("  - {} was removed.", missing_asset_path.display())
            }
        }
    }

    pub fn print_entry_issues(&self) {
        for entry in self.broken_entries.iter() {
            let entry_issue_count = entry.issue_count();

            if entry_issue_count.eq(&0) {
                continue;
            }

            match &entry.kind {
                PkgEntryType::Main => println!(
                    "\n{} to {} entry:",
                    Self::get_breaking_change_count_prefix(&entry_issue_count),
                    entry.kind
                ),
                entry_kind => println!(
                    "\n{} to {} entry {}:",
                    Self::get_breaking_change_count_prefix(&entry_issue_count),
                    entry_kind,
                    entry.name
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

    pub fn print_conclusion(&self, start_timestamp: Instant) -> ExitCode {
        let issue_count = self.issue_count();
        let elapsed_time = start_timestamp.elapsed().as_secs_f32();

        println!(
            "\n{} in {elapsed_time:.2}s.",
            Self::get_breaking_change_count_prefix(&issue_count)
        );

        if issue_count.eq(&0) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }

    fn issue_count(&self) -> usize {
        let broken_entry_issue_count = self
            .broken_entries
            .iter()
            .map(|entry| entry.issue_count())
            .sum::<usize>();

        self.removed_assets.len() + broken_entry_issue_count
    }

    fn get_breaking_change_count_prefix(issue_count: &usize) -> String {
        if issue_count.eq(&0) {
            format!("Found {issue_count} breaking change")
        } else {
            format!("Found {issue_count} breaking changes")
        }
    }
}
