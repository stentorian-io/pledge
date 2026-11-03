//! Overload removal compatibility checks.

use pledge::languages::typescript::types::shape::{ParamShape, TypeShape};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn unmatched_overload_fires_removed() {
    use pledge::languages::javascript::model::{ExportKind, ExportSymbol};
    use pledge::languages::typescript::compare::refinements::compare_ref_changes;

    let file = test_file();
    // old: 2 overloads at arity 1; new: 1 overload at arity 1
    // After consumed-tracking, one should fire overload-removed
    let overload_string = TypeShape::Function {
        contract: Default::default(),
        params: vec![make_param(
            "x",
            false,
            TypeShape::Opaque("string".to_string()),
        )],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };
    let overload_number = TypeShape::Function {
        contract: Default::default(),
        params: vec![make_param(
            "x",
            false,
            TypeShape::Opaque("number".to_string()),
        )],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };

    let old_sym = ExportSymbol {
        name: "myFn".to_string(),
        kind: ExportKind::Named,
        file: file.clone(),
        line: 1,
        col: 0,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        decl_kinds: Vec::new(),
        overloads: vec![overload_string.clone(), overload_number],
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
        name: "myFn".to_string(),
        kind: ExportKind::Named,
        file: file.clone(),
        line: 1,
        col: 0,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        decl_kinds: Vec::new(),
        overloads: vec![overload_string],
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
    let findings = compare_ref_changes(&old_sym, &new_sym, "myFn", &file, 1, 0, &[], &[]);
    assert!(
        findings.iter().any(|f| f.rule == "overload-removed"),
        "SEM-07: expected overload-removed. Got: {:?}",
        findings
    );
}

fn make_param(name: &str, optional: bool, shape: TypeShape) -> ParamShape {
    ParamShape {
        name: name.to_string(),
        optional,
        rest: false,
        shape,
    }
}
