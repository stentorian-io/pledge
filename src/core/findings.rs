//! Findings, severities, and consolidation shared by all analyzers.

use std::collections::HashSet;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Major,
    Minor,
    Patch,
    Warning,
    Info,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub rule: &'static str,
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
    pub message: String,
}

/// Returns true if any finding has Major severity (per CLI-02).
pub fn has_breaking_changes(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Major)
}

/// Remove duplicate findings by a 5-tuple key: (rule, file, line, col, message).
///
/// ROB-07: The key was widened from (rule, file, line) to include `col` and `message`.
/// This prevents two findings at the same location but with different messages from being
/// silently collapsed — e.g., overload-removed and param-type-narrowed at the same line.
/// Called twice in each pipeline path: once after merging passes, and once after sorting,
/// to collapse any cross-pass duplicates that the sort may have brought together.
pub(crate) fn dedup_findings(findings: &mut Vec<Finding>) {
    let mut seen = HashSet::new();
    findings.retain(|f| {
        let key = (f.rule, f.file.clone(), f.line, f.col, f.message.clone());
        seen.insert(key)
    });
}
#[cfg(test)]
mod tests {
    use crate::core::findings::{Finding, Severity};
    use std::path::PathBuf;

    fn make_finding(rule: &'static str, line: u32, col: u32, message: &str) -> Finding {
        Finding {
            severity: Severity::Major,
            rule,
            file: PathBuf::from("a.ts"),
            line,
            col,
            message: message.into(),
        }
    }

    #[test]
    fn different_message_same_line_preserved() {
        let mut findings = vec![
            make_finding("r", 1, 1, "first"),
            make_finding("r", 1, 1, "second"),
        ];
        super::dedup_findings(&mut findings);
        assert_eq!(
            findings.len(),
            2,
            "ROB-07: same rule/file/line/col but different message must both be retained"
        );
    }

    #[test]
    fn identical_fields_deduped() {
        let mut findings = vec![
            make_finding("r", 1, 1, "same"),
            make_finding("r", 1, 1, "same"),
        ];
        super::dedup_findings(&mut findings);
        assert_eq!(
            findings.len(),
            1,
            "ROB-07: identical 5-tuple must be deduped to 1"
        );
    }

    #[test]
    fn different_col_preserved() {
        let mut findings = vec![make_finding("r", 1, 1, "m"), make_finding("r", 1, 5, "m")];
        super::dedup_findings(&mut findings);
        assert_eq!(
            findings.len(),
            2,
            "ROB-07: different col must be retained as distinct findings"
        );
    }
}
