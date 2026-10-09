//! Anonymous exports compatibility checks.

use interlint::core::findings::Severity;
use interlint::languages::javascript::compare::structure::compare_structural_diff;
use interlint::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol,
};
use std::path::PathBuf;

fn make_default_export_sym(
    file: PathBuf,
    line: u32,
    decl_kind: Option<DeclarationKind>,
) -> ExportSymbol {
    ExportSymbol {
        name: "default".to_string(),
        kind: ExportKind::Default,
        file,
        line,
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

#[test]
fn test_anonymous_default_class_method_removed() {
    // Old: export default class { foo(): void {} }
    // New: export default class { }
    // Expected: class-method-removed finding
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export default class { foo(): void {} }\n";
    let new_source = b"export default class { }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(PathBuf::from("index.ts"), 1, Some(DeclarationKind::Class)),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(PathBuf::from("index.ts"), 1, Some(DeclarationKind::Class)),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-method-removed" && f.severity == Severity::Major),
        "removing foo() from anonymous default class should produce class-method-removed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}
