//! Generators compatibility checks.

use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn generator_removed_is_major_severity() {
    use interlint::languages::javascript::compare::behavior::compare_behavior;
    use interlint::languages::javascript::model::{ExportKind, ExportSymbol};

    let file = test_file();
    let old_sym = ExportSymbol {
        name: "gen".to_string(),
        kind: ExportKind::Named,
        file: file.clone(),
        line: 1,
        col: 0,
        is_async: false,
        is_generator: true,
        is_reexport: false,
        decl_kinds: Vec::new(),
        overloads: vec![],
        release_tag: None,
        is_type_only: false,
        type_signature: None,
        type_guard_type: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    };
    let new_sym = ExportSymbol {
        name: "gen".to_string(),
        kind: ExportKind::Named,
        file: file.clone(),
        line: 1,
        col: 0,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        decl_kinds: Vec::new(),
        overloads: vec![],
        release_tag: None,
        is_type_only: false,
        type_signature: None,
        type_guard_type: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    };
    let findings = compare_behavior(&old_sym, &new_sym, "gen", &file, 1, 0);
    assert!(
        findings.iter().any(|f| f.rule == "generator-removed"),
        "SEM-04: expected generator-removed. Got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "generator-removed")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::core::findings::Severity::Major,
        "SEM-04: generator-removed should be Major. Got: {:?}",
        f.severity
    );
}
