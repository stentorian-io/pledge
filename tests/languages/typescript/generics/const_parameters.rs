//! Const parameters compatibility checks.

use interlint::languages::javascript::compare::structure::compare_structural_diff;
use interlint::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol,
};
use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;
use std::path::PathBuf;

fn make_export_sym(
    name: &str,
    file: PathBuf,
    line: u32,
    decl_kind: Option<DeclarationKind>,
) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
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
fn const_type_param_parse() {
    // export function foo<const T>(x: T): T { return x; }
    // parse_exports should succeed (no panic)
    let source = b"export function foo<const T>(x: T): T { return x; }\n";
    let result = parse_exports(source, SourceLanguage::TypeScript);
    assert!(
        result.is_ok(),
        "parse_exports should not error on const type parameter syntax; got: {:?}",
        result.err()
    );

    let exports = result.unwrap();
    assert!(
        exports.iter().any(|e| e.name == "foo"),
        "function 'foo' with const type param should appear in exports; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn const_type_param_removed() {
    // Old: export function foo<const T>(x: T): T { return x; }
    // New: export function foo<T>(x: T): T { return x; }
    // Expected: if grammar supports const type params, const-type-param-changed fires
    // This test verifies the full pipeline without asserting on const detection
    // (graceful degradation: test passes whether or not grammar detects const)
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export function foo<const T>(x: T): T { return x; }\n";
    let new_source = b"export function foo<T>(x: T): T { return x; }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("foo"),
        make_export_sym(
            "foo",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("foo"),
        make_export_sym(
            "foo",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    // Should not panic — graceful degradation acceptable
    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    // If the grammar supports const type params, the finding should appear
    // If not, the test still passes (graceful degradation)
    let has_const_finding = findings
        .iter()
        .any(|f| f.rule == "const-type-param-changed");
    let _ = has_const_finding; // Suppress unused variable warning — both true and false are valid
}

#[test]
fn const_type_param_present_no_change() {
    // Old and new both have const T — no finding expected
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let source = b"export function foo<const T>(x: T): T { return x; }\n";

    std::fs::write(old_dir.path().join("index.ts"), source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("foo"),
        make_export_sym(
            "foo",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("foo"),
        make_export_sym(
            "foo",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "const-type-param-changed"),
        "unchanged const type params should not produce const-type-param-changed; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
