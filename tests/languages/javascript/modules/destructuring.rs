//! Destructuring compatibility checks.

use interlint::languages::javascript::compare::compare_exports;
use interlint::languages::javascript::model::{ExportKey, ExportKind, ExportMap, ExportSymbol};
use interlint::languages::javascript::syntax::exports::{parse_exports, RawExportKind};
use interlint::languages::javascript::syntax::grammar::SourceLanguage;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

fn make_export_sym(name: &str, file: PathBuf, line: u32) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file,
        line,
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

#[test]
fn test_destructured_pair_exports_renamed_binding() {
    // `export const { foo: renamed } = obj` should export `renamed`, not `foo`
    let source = b"export const { foo: renamed, bar: other } = obj;";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    assert!(
        exports
            .iter()
            .any(|e| e.name == "renamed" && e.kind == RawExportKind::Named),
        "should export 'renamed'; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "other" && e.kind == RawExportKind::Named),
        "should export 'other'; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        !exports.iter().any(|e| e.name == "foo"),
        "source key 'foo' should NOT appear; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        !exports.iter().any(|e| e.name == "bar"),
        "source key 'bar' should NOT appear; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_destructured_pair_removal_detected() {
    // Removing a destructured pair export should be detected by compare_exports
    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("renamed"),
        make_export_sym("renamed", test_file(), 1),
    );
    old_exports.insert(
        ExportKey::new_root_value("other"),
        make_export_sym("other", test_file(), 1),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("renamed"),
        make_export_sym("renamed", test_file(), 1),
    );
    // "other" removed

    let findings = compare_exports(&old_exports, &new_exports, std::path::Path::new("."), true);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("other")),
        "removing 'other' should produce export-removed; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_array_destructured_exports_captured() {
    let source = b"export const [first, second, third] = values;";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    assert!(exports
        .iter()
        .any(|e| e.name == "first" && e.kind == RawExportKind::Named));
    assert!(exports
        .iter()
        .any(|e| e.name == "second" && e.kind == RawExportKind::Named));
    assert!(exports
        .iter()
        .any(|e| e.name == "third" && e.kind == RawExportKind::Named));
}

#[test]
fn test_array_destructured_rest_captured() {
    let source = b"export const [head, ...tail] = items;";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    assert!(exports
        .iter()
        .any(|e| e.name == "head" && e.kind == RawExportKind::Named));
    assert!(exports
        .iter()
        .any(|e| e.name == "tail" && e.kind == RawExportKind::Named));
}

#[test]
fn test_array_destructured_removal_detected() {
    // Removing an array-destructured export should be detected
    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("a"),
        make_export_sym("a", test_file(), 1),
    );
    old_exports.insert(
        ExportKey::new_root_value("b"),
        make_export_sym("b", test_file(), 1),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("a"),
        make_export_sym("a", test_file(), 1),
    );
    // b removed

    let findings = compare_exports(&old_exports, &new_exports, std::path::Path::new("."), true);
    assert!(findings
        .iter()
        .any(|f| f.rule == "export-removed" && f.message.contains('b')));
}
