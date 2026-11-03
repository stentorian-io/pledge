//! Default kind changes compatibility checks.

use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn ref_04_suppressed_when_ref_08_fires() {
    use pledge::languages::javascript::model::{DeclarationKind, ExportKind, ExportSymbol};
    use pledge::languages::typescript::compare::refinements::compare_ref_changes;

    let file = test_file();
    // Build a function -> class kind change that triggers REF-08 (default-export-changed)
    // REF-04 (declaration-kind-changed) should be suppressed.
    let old_sym = ExportSymbol {
        name: "default".to_string(),
        kind: ExportKind::Default,
        file: file.clone(),
        line: 1,
        col: 0,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        decl_kinds: vec![DeclarationKind::Function],
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
        name: "default".to_string(),
        kind: ExportKind::Default,
        file: file.clone(),
        line: 1,
        col: 0,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        decl_kinds: vec![DeclarationKind::Class],
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
    let findings = compare_ref_changes(&old_sym, &new_sym, "default", &file, 1, 0, &[], &[]);
    // REF-04 should not appear when REF-08 fires
    let has_ref_04 = findings
        .iter()
        .any(|f| f.rule == "declaration-kind-changed");
    let has_ref_08 = findings.iter().any(|f| f.rule == "default-export-changed");
    assert!(
        has_ref_08,
        "SEM-05: REF-08 default-export-changed should fire. Got: {:?}",
        findings
    );
    assert!(
        !has_ref_04,
        "SEM-05: REF-04 declaration-kind-changed should be suppressed. Got: {:?}",
        findings
    );
}
