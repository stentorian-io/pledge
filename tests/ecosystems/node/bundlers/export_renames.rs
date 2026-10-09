//! Export renames compatibility checks.

use interlint::core::findings::Severity;
use interlint::languages::javascript::compare::compare_exports;
use interlint::languages::javascript::compare::rules::RULE_EXPORT_RENAMED;
use interlint::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, SymbolSpace,
};
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

#[test]
fn alias_trailer_rename_produces_minor_renamed_finding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("old.ts"), "export const OldApi = 1;\n").unwrap();
    std::fs::write(
        dir.path().join("new.ts"),
        r#"
const NewApi = 1;
export { NewApi as OldApi, NewApi };
"#,
    )
    .unwrap();

    let mut old_map = ExportMap::new();
    let mut new_map = ExportMap::new();
    old_map.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "OldApi"),
        make_sym(
            "OldApi",
            PathBuf::from("old.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    new_map.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "NewApi"),
        make_sym(
            "NewApi",
            PathBuf::from("new.ts"),
            3,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old_map, &new_map, dir.path(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_EXPORT_RENAMED)
            .count(),
        1,
        "exactly one export-renamed expected; got {:?}",
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
        "IDN-01 should suppress the base export-removed"
    );
    assert_eq!(
        findings.iter().filter(|f| f.rule == "export-added").count(),
        0,
        "IDN-01 should suppress the base export-added"
    );
    let renamed = findings
        .iter()
        .find(|f| f.rule == RULE_EXPORT_RENAMED)
        .unwrap();
    assert!(matches!(renamed.severity, Severity::Minor));
    assert!(
        renamed.message.contains("OldApi") && renamed.message.contains("NewApi"),
        "rename message must mention both names; got: {}",
        renamed.message
    );
}

#[test]
fn no_alias_trailer_stays_as_major_removed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("new.ts"), "export const NewApi = 1;\n").unwrap();

    let mut old_map = ExportMap::new();
    let mut new_map = ExportMap::new();
    old_map.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "OldApi"),
        make_sym(
            "OldApi",
            PathBuf::from("old.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    new_map.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "NewApi"),
        make_sym(
            "NewApi",
            PathBuf::from("new.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old_map, &new_map, dir.path(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_EXPORT_RENAMED)
            .count(),
        0,
        "without alias trailer, IDN-01 must NOT coalesce"
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1,
        "plain removal stays as Major export-removed"
    );
}
