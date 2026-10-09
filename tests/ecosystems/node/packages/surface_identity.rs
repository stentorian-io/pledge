//! Surface identity compatibility checks.

use interlint::compare::compare_exports;
use interlint::types::{
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

/// Benign pkg_root placeholder — used for synthetic ExportMap tests that don't
/// need real source files (IDN-01 source-scan will find no alias trailer).
fn noop_root() -> &'static std::path::Path {
    std::path::Path::new(".")
}

#[test]
fn symbol_moved_between_subpaths_produces_removed_finding() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();

    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "createElement"),
        make_sym(
            "createElement",
            PathBuf::from("index.js"),
            1,
            vec![DeclarationKind::Function],
        ),
    );
    new.insert(
        ExportKey::new("./jsx-runtime", None, SymbolSpace::Value, "createElement"),
        make_sym(
            "createElement",
            PathBuf::from("jsx-runtime.js"),
            1,
            vec![DeclarationKind::Function],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1,
        "removal at original (root) subpath must fire"
    );
    assert_eq!(
        findings.iter().filter(|f| f.rule == "export-added").count(),
        1,
        "addition at new subpath must fire"
    );
    let removed = findings
        .iter()
        .find(|f| f.rule == "export-removed")
        .unwrap();
    assert_eq!(removed.file, PathBuf::from("index.js"));
}

#[test]
fn dropped_cjs_condition_produces_condition_scoped_finding() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();

    old.insert(
        ExportKey::new(".", Some("import"), SymbolSpace::Value, "z"),
        make_sym(
            "z",
            PathBuf::from("lib/index.mjs"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    old.insert(
        ExportKey::new(".", Some("require"), SymbolSpace::Value, "z"),
        make_sym(
            "z",
            PathBuf::from("lib/index.js"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", Some("import"), SymbolSpace::Value, "z"),
        make_sym(
            "z",
            PathBuf::from("lib/index.mjs"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1
    );
    let removed = findings
        .iter()
        .find(|f| f.rule == "export-removed")
        .unwrap();
    assert!(
        removed.message.contains("require") || removed.message.contains("z"),
        "removed finding message should name the dropped condition or symbol, got: {}",
        removed.message
    );
}

#[test]
fn multi_subpath_same_name_class_both_diffed() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();

    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "User"),
        make_sym(
            "User",
            PathBuf::from("src/a.ts"),
            1,
            vec![DeclarationKind::Class],
        ),
    );
    old.insert(
        ExportKey::new("./admin", None, SymbolSpace::Value, "User"),
        make_sym(
            "User",
            PathBuf::from("src/b.ts"),
            1,
            vec![DeclarationKind::Class],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "User"),
        make_sym(
            "User",
            PathBuf::from("src/a.ts"),
            1,
            vec![DeclarationKind::Class],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    let removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "export-removed")
        .collect();
    assert_eq!(removed.len(), 1, "exactly one removal (admin User)");
    assert_eq!(removed[0].file, PathBuf::from("src/b.ts"));
}
