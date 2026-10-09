//! Namespace moves compatibility checks.

use interlint::core::findings::Severity;
use interlint::ecosystems::node::discovery::discover_exports;
use interlint::languages::javascript::compare::compare_exports;
use interlint::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, SymbolSpace,
};
use interlint::languages::typescript::compare::rules::RULE_NAMESPACE_MEMBER_MOVED;
use std::path::PathBuf;

fn make_sym(name: &str, file: PathBuf, line: u32, kinds: Vec<DeclarationKind>) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file,
        line,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: kinds,
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

fn write_pkg(dir: &std::path::Path, pkg: serde_json::Value) {
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();
}

/// Benign pkg_root placeholder — used for synthetic ExportMap tests that don't
/// need real source files (IDN-01 source-scan will find no alias trailer).
fn noop_root() -> &'static std::path::Path {
    std::path::Path::new(".")
}

#[test]
fn namespace_move_collapses_into_member_moved_findings() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "A.x"),
        make_sym(
            "A.x",
            PathBuf::from("ns.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "A.y"),
        make_sym(
            "A.y",
            PathBuf::from("ns.ts"),
            2,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "D.x"),
        make_sym(
            "D.x",
            PathBuf::from("ns.ts"),
            10,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "D.y"),
        make_sym(
            "D.y",
            PathBuf::from("ns.ts"),
            11,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
            .count(),
        2,
        "IDN-02 should emit 2 namespace-member-moved; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        0,
        "IDN-02 should suppress the base export-removed"
    );
    assert_eq!(
        findings.iter().filter(|f| f.rule == "export-added").count(),
        0,
        "IDN-02 should suppress the base export-added"
    );
    for f in findings
        .iter()
        .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
    {
        assert!(matches!(f.severity, Severity::Major));
        assert!(
            f.message.contains("A.") && f.message.contains("D."),
            "ns-move message must mention both paths; got {}",
            f.message
        );
    }
}

#[test]
fn partial_overlap_does_not_collapse() {
    // exact-subtree match — partial overlap must NOT collapse.
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "A.x"),
        make_sym(
            "A.x",
            PathBuf::from("ns.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "A.y"),
        make_sym(
            "A.y",
            PathBuf::from("ns.ts"),
            2,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "D.x"),
        make_sym(
            "D.x",
            PathBuf::from("ns.ts"),
            10,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
            .count(),
        0,
        "partial overlap must NOT collapse; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn warning6_through_full_pipeline() {
    let old_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        old_dir.path().join("ns.ts"),
        r#"
export namespace A {
  export const x = 1;
  export const y = 2;
}
"#,
    )
    .unwrap();
    write_pkg(
        old_dir.path(),
        serde_json::json!({
            "name": "ns-pkg", "version": "1.0.0", "main": "./ns.ts"
        }),
    );

    let new_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        new_dir.path().join("ns.ts"),
        r#"
export namespace D {
  export const x = 1;
  export const y = 2;
}
"#,
    )
    .unwrap();
    write_pkg(
        new_dir.path(),
        serde_json::json!({
            "name": "ns-pkg", "version": "1.0.0", "main": "./ns.ts"
        }),
    );

    let (old_exports, _) = discover_exports(old_dir.path()).unwrap();
    let (new_exports, _) = discover_exports(new_dir.path()).unwrap();

    // Sanity:  produced dotted-name ExportKeys.
    assert!(
        old_exports.keys().any(|k| k.name == "A.x"),
        " should emit dotted A.x; got keys: {:?}",
        old_exports.keys().map(|k| &k.name).collect::<Vec<_>>()
    );
    assert!(
        new_exports.keys().any(|k| k.name == "D.x"),
        " should emit dotted D.x; got keys: {:?}",
        new_exports.keys().map(|k| &k.name).collect::<Vec<_>>()
    );

    let findings = compare_exports(&old_exports, &new_exports, new_dir.path(), true);
    // Through the full pipeline, IDN-02 should consolidate into exactly 2
    // namespace-member-moved findings. Note: the namespace declarations
    // themselves (A/D in SymbolSpace::Namespace) remain as plain add/remove
    // outside the scope of IDN-02's depth-1 member consolidation.
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
            .count(),
        2,
        "Warning 6: pipeline test must yield 2 namespace-member-moved findings via discover→compare; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
