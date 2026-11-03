//! Object destructuring compatibility checks.

use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::model::{ExportKey, ExportKind, ExportMap, ExportSymbol};
use pledge::languages::javascript::syntax::exports::{parse_exports, RawExportKind};
use pledge::languages::javascript::syntax::grammar::SourceLanguage;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

fn make_export_sym(name: &str) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: Vec::new(),
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

fn map_from_names(names: &[&str]) -> ExportMap {
    let mut map = ExportMap::new();
    for &name in names {
        map.insert(ExportKey::new_root_value(name), make_export_sym(name));
    }
    map
}

/// ECF-02: `export const { alpha, beta, gamma } = config` should produce 3 Named exports.
/// Validates that destructured object export patterns are captured.
#[test]
fn test_destructured_export_captured() {
    let src = b"export const { alpha, beta, gamma } = config;";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        3,
        "Expected 3 exports from destructured export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    for expected_name in ["alpha", "beta", "gamma"] {
        assert!(
            exports
                .iter()
                .any(|e| e.name == expected_name && e.kind == RawExportKind::Named),
            "Expected export named '{}' with kind Named; got: {:?}",
            expected_name,
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }
}

/// ECF-02: Removing one member of a destructured export fires export-removed.
/// Old has "alpha" and "beta", new has only "alpha" → export-removed for "beta".
#[test]
fn test_destructured_export_removal_detected() {
    let old_map = map_from_names(&["alpha", "beta"]);
    let new_map = map_from_names(&["alpha"]);

    let findings = compare_exports(&old_map, &new_map, std::path::Path::new("."), true);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("beta")),
        "Expected export-removed for 'beta'; got: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );

    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("alpha")),
        "Should NOT fire export-removed for 'alpha' (still present); got: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}
