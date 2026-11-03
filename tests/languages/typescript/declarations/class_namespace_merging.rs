//! Class namespace merging compatibility checks.

use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::model::{
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
fn class_namespace_hybrid_preserved_no_ref05_fp() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Namespace, "User"),
        make_sym("User", test_file(), 1, vec![DeclarationKind::Namespace]),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Namespace, "User"),
        make_sym(
            "User",
            test_file(),
            1,
            vec![DeclarationKind::Class, DeclarationKind::Namespace],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    let ref05_fired = findings.iter().any(|f| f.rule == "namespace-to-other");
    assert!(
        !ref05_fired,
        "hybrid class+namespace must NOT fire REF-05; got findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
