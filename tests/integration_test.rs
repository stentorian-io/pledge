use interlint::compare::{compare_exports, has_breaking_changes};
use interlint::discover::discover_exports;
use interlint::types::Severity;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_discover_simple_js_exports() {
    let (exports, _warnings) = discover_exports(&fixture_path("simple-pkg")).unwrap();
    assert!(
        exports.keys().any(|k| k.name == "greet"),
        "should find named export 'greet'"
    );
    assert!(
        exports.keys().any(|k| k.name == "farewell"),
        "should find named export 'farewell'"
    );
    assert!(
        exports.keys().any(|k| k.name == "default"),
        "should find default export"
    );
    assert_eq!(exports.len(), 3, "should find exactly 3 exports");
}

#[test]
fn test_discover_typescript_exports() {
    let (exports, _warnings) = discover_exports(&fixture_path("ts-pkg")).unwrap();
    assert!(
        exports.keys().any(|k| k.name == "VERSION"),
        "should find const export 'VERSION'"
    );
    assert!(
        exports.keys().any(|k| k.name == "Config"),
        "should find interface export 'Config'"
    );
    assert!(
        exports.keys().any(|k| k.name == "configure"),
        "should find function export 'configure'"
    );
    assert!(
        exports.keys().any(|k| k.name == "default"),
        "should find default export 'App'"
    );
}

#[test]
fn test_discover_reexports() {
    let (exports, _warnings) = discover_exports(&fixture_path("reexport-pkg")).unwrap();
    assert!(
        exports.keys().any(|k| k.name == "greet"),
        "should find re-exported 'greet'"
    );
    assert!(
        exports.keys().any(|k| k.name == "helper"),
        "should find star re-exported 'helper'"
    );
}

#[test]
fn test_compare_detects_removed_export() {
    let (old, _) = discover_exports(&fixture_path("simple-pkg")).unwrap();
    let (new, _) = discover_exports(&fixture_path("simple-pkg-v2")).unwrap();
    let findings = compare_exports(&old, &new);
    let removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "export-removed")
        .collect();
    assert_eq!(removed.len(), 1, "should detect 1 removed export");
    assert_eq!(removed[0].severity, Severity::Major);
    assert!(
        removed[0].message.contains("farewell"),
        "message should mention 'farewell', got: {}",
        removed[0].message
    );
}

#[test]
fn test_compare_detects_added_export() {
    let (old, _) = discover_exports(&fixture_path("simple-pkg")).unwrap();
    let (new, _) = discover_exports(&fixture_path("simple-pkg-v2")).unwrap();
    let findings = compare_exports(&old, &new);
    let added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "export-added")
        .collect();
    assert_eq!(added.len(), 1, "should detect 1 added export");
    assert_eq!(added[0].severity, Severity::Minor);
    assert!(
        added[0].message.contains("newFunc"),
        "message should mention 'newFunc', got: {}",
        added[0].message
    );
}

#[test]
fn test_compare_identical_no_findings() {
    let (old, _) = discover_exports(&fixture_path("simple-pkg")).unwrap();
    let (new, _) = discover_exports(&fixture_path("simple-pkg")).unwrap();
    let findings = compare_exports(&old, &new);
    assert!(
        findings.is_empty(),
        "identical exports should produce no findings"
    );
}

#[test]
fn test_has_breaking_changes() {
    let (old, _) = discover_exports(&fixture_path("simple-pkg")).unwrap();
    let (new, _) = discover_exports(&fixture_path("simple-pkg-v2")).unwrap();
    let findings = compare_exports(&old, &new);
    assert!(
        has_breaking_changes(&findings),
        "should detect breaking changes when export removed"
    );
}

#[test]
fn test_findings_sorted_by_severity() {
    let (old, _) = discover_exports(&fixture_path("simple-pkg")).unwrap();
    let (new, _) = discover_exports(&fixture_path("simple-pkg-v2")).unwrap();
    let findings = compare_exports(&old, &new);
    // major findings should come before minor
    let first_major_idx = findings.iter().position(|f| f.severity == Severity::Major);
    let last_minor_idx = findings.iter().rposition(|f| f.severity == Severity::Minor);
    if let (Some(maj), Some(min)) = (first_major_idx, last_minor_idx) {
        assert!(maj < min, "major findings should be sorted before minor");
    }
}

#[test]
fn test_finding_line_numbers_are_1_indexed() {
    let (exports, _) = discover_exports(&fixture_path("simple-pkg")).unwrap();
    for (key, sym) in &exports {
        let name = &key.name;
        assert!(
            sym.line >= 1,
            "line for '{}' should be >= 1 (1-indexed), got {}",
            name,
            sym.line
        );
        assert!(
            sym.col >= 1,
            "col for '{}' should be >= 1 (1-indexed), got {}",
            name,
            sym.col
        );
    }
}
