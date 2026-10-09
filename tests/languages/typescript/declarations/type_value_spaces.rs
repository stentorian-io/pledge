//! Type value spaces compatibility checks.

use interlint::compare::compare_exports;
use interlint::types::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, SymbolSpace,
};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

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

/// Benign pkg_root placeholder — used for synthetic ExportMap tests that don't
/// need real source files (IDN-01 source-scan will find no alias trailer).
fn noop_root() -> &'static std::path::Path {
    std::path::Path::new(".")
}

#[test]
fn type_and_value_space_coexist_and_diff_independently() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();

    old.insert(
        ExportKey::new(".", None, SymbolSpace::Type, "Foo"),
        make_sym("Foo", test_file(), 1, vec![DeclarationKind::Interface]),
    );
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
        make_sym("Foo", test_file(), 5, vec![DeclarationKind::Variable]),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
        make_sym("Foo", test_file(), 5, vec![DeclarationKind::Variable]),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1
    );
    assert_eq!(
        findings.iter().filter(|f| f.rule == "export-added").count(),
        0
    );
}
