//! Ambient members compatibility checks.

use pledge::core::findings::Severity;
use pledge::languages::javascript::compare::structure::compare_structural_diff;
use pledge::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol,
};
use std::path::PathBuf;

#[test]
fn test_ambient_class_removal_detected() {
    // A .d.ts file with `export declare class Foo { bar(): void }` should
    // produce class-method-removed when bar() is removed in new version.
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export declare class Foo {\n  bar(): void;\n  baz(): string;\n}\n";
    let new_source = b"export declare class Foo {\n  baz(): string;\n}\n";

    std::fs::write(old_dir.path().join("index.d.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.d.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("Foo"),
        ExportSymbol {
            name: "Foo".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("index.d.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: vec![DeclarationKind::Class],
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
        },
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("Foo"),
        ExportSymbol {
            name: "Foo".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("index.d.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: vec![DeclarationKind::Class],
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
        },
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-method-removed" && f.severity == Severity::Major),
        "should detect bar() removal from ambient class; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
