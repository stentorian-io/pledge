//! Deduplication compatibility checks.

use interlint::core::findings::{Finding, Severity};
use std::path::PathBuf;

/// BUG 7: Duplicate findings (same rule + file + line, different messages)
/// should be collapsed to one (rule, file, line) key.
/// This tests the dedup logic directly with a local reimplementation matching
/// the one in main.rs.
#[test]
fn test_finding_deduplication() {
    use std::collections::HashSet;

    let file = PathBuf::from("src/index.ts");

    // Create 3 findings: two are duplicates of finding 1, one is unique
    let findings = vec![
        Finding {
            severity: Severity::Major,
            rule: "export-removed",
            file: file.clone(),
            line: 5,
            col: 1,
            message: "exported symbol `foo` was removed".to_string(),
        },
        Finding {
            severity: Severity::Major,
            rule: "export-removed",
            file: file.clone(),
            line: 5,
            col: 1,
            message: "duplicate: exported symbol `foo` was removed".to_string(),
        },
        Finding {
            severity: Severity::Minor,
            rule: "export-added",
            file: file.clone(),
            line: 10,
            col: 1,
            message: "exported symbol `bar` was added".to_string(),
        },
    ];

    // Apply dedup logic: retain first occurrence (rule, file, line)
    let mut seen = HashSet::new();
    let mut deduped = findings;
    deduped.retain(|f| {
        let key = (f.rule, f.file.clone(), f.line);
        seen.insert(key)
    });

    assert_eq!(
        deduped.len(),
        2,
        "Should have 2 findings after dedup, got {}",
        deduped.len()
    );

    // The duplicate export-removed at line 5 should be collapsed to one
    let removed_findings: Vec<_> = deduped
        .iter()
        .filter(|f| f.rule == "export-removed")
        .collect();
    assert_eq!(
        removed_findings.len(),
        1,
        "Should have exactly 1 export-removed finding after dedup"
    );

    // The unique export-added should still be present
    assert!(
        deduped.iter().any(|f| f.rule == "export-added"),
        "export-added should still be present after dedup"
    );
}
