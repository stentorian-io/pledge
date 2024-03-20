use crate::pkg::entries::PkgEntryType;
use std::path::PathBuf;
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

pub struct BrokenEntryResult {
    pub kind: PkgEntryType,
    pub name: String,
    pub is_missing: bool,
    pub broken_exports: Vec<BrokenExport>,
}

impl BrokenEntryResult {
    pub fn issue_count(&self) -> usize {
        if self.is_missing {
            1
        } else {
            self.broken_exports.len()
        }
    }
}

#[derive(Default)]
pub struct DiffResults {
    pub removed_assets: Vec<PathBuf>,
    pub broken_entries: Vec<BrokenEntryResult>,
}

impl DiffResults {
    pub fn issue_count(&self) -> usize {
        let broken_entry_issue_count = self
            .broken_entries
            .iter()
            .map(|entry| entry.issue_count())
            .sum::<usize>();

        self.removed_assets.len() + broken_entry_issue_count
    }
}
