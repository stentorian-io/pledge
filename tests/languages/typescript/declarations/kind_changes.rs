//! Kind changes compatibility checks.

use pledge::core::findings::Finding;
use pledge::languages::javascript::model::{DeclarationKind, ExportKind, ExportSymbol};
use pledge::languages::typescript::compare::refinements as ref_diff;
use pledge::languages::typescript::compare::rules::RULE_DECLARATION_KIND_CHANGED;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

fn make_sym(name: &str, decl_kind: Option<DeclarationKind>) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: decl_kind.map(|k| vec![k]).unwrap_or_default(),
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    }
}

/// BUG 6: Changing from `DeclarationKind::Class` to `DeclarationKind::Function`
/// should fire `declaration-kind-changed`.
#[test]
fn test_declaration_kind_class_to_function() {
    let old_sym = make_sym("Widget", Some(DeclarationKind::Class));
    let new_sym = make_sym("Widget", Some(DeclarationKind::Function));

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "Widget", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
        "Expected declaration-kind-changed (Class→Function); got: {:?}",
        rule_ids(&findings)
    );
}

/// BUG 6: Changing from `DeclarationKind::Enum` to `DeclarationKind::Variable`
/// should fire `declaration-kind-changed`.
#[test]
fn test_declaration_kind_enum_to_variable() {
    let old_sym = make_sym("Status", Some(DeclarationKind::Enum));
    let new_sym = make_sym("Status", Some(DeclarationKind::Variable));

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "Status", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
        "Expected declaration-kind-changed (Enum→Variable); got: {:?}",
        rule_ids(&findings)
    );
}

fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}
