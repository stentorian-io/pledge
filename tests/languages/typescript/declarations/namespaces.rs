//! Namespaces compatibility checks.

use interlint::languages::javascript::compare::compare_exports;
use interlint::languages::javascript::model::{ExportKey, ExportKind, ExportMap, ExportSymbol};
use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;
use std::path::PathBuf;

#[test]
fn test_namespace_member_removed() {
    // Old: export namespace Foo { export function bar(): void {} }
    // New: export namespace Foo { }
    // Expected: export-removed for "Foo.bar"
    let old_source = b"export namespace Foo { export function bar(): void {} }\n";
    let new_source = b"export namespace Foo { }\n";

    let old_exports_raw = parse_exports(old_source, SourceLanguage::TypeScript).unwrap();
    let new_exports_raw = parse_exports(new_source, SourceLanguage::TypeScript).unwrap();

    let mut old_exports = ExportMap::new();
    for raw in &old_exports_raw {
        old_exports.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: raw.decl_kind.clone().map(|k| vec![k]).unwrap_or_default(),
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
    }

    let mut new_exports = ExportMap::new();
    for raw in &new_exports_raw {
        new_exports.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: raw.decl_kind.clone().map(|k| vec![k]).unwrap_or_default(),
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
    }

    // Verify that parse_exports produced "Foo.bar" for old source
    assert!(
        old_exports.contains_key(&ExportKey::new_root_value("Foo.bar")),
        "old exports should contain 'Foo.bar'; got: {:?}",
        old_exports.keys().collect::<Vec<_>>()
    );

    let findings = compare_exports(&old_exports, &new_exports, std::path::Path::new("."), true);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("Foo.bar")),
        "removing namespace member Foo.bar should produce export-removed finding; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_namespace_member_added() {
    // Old: export namespace Foo { }
    // New: export namespace Foo { export function bar(): void {} }
    // Expected: export-added for "Foo.bar"
    let old_source = b"export namespace Foo { }\n";
    let new_source = b"export namespace Foo { export function bar(): void {} }\n";

    let old_exports_raw = parse_exports(old_source, SourceLanguage::TypeScript).unwrap();
    let new_exports_raw = parse_exports(new_source, SourceLanguage::TypeScript).unwrap();

    let mut old_exports = ExportMap::new();
    for raw in &old_exports_raw {
        old_exports.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: raw.decl_kind.clone().map(|k| vec![k]).unwrap_or_default(),
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
    }

    let mut new_exports = ExportMap::new();
    for raw in &new_exports_raw {
        new_exports.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: raw.decl_kind.clone().map(|k| vec![k]).unwrap_or_default(),
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
    }

    // Verify parse_exports produced "Foo.bar" for new source
    assert!(
        new_exports.contains_key(&ExportKey::new_root_value("Foo.bar")),
        "new exports should contain 'Foo.bar'; got: {:?}",
        new_exports.keys().collect::<Vec<_>>()
    );

    let findings = compare_exports(&old_exports, &new_exports, std::path::Path::new("."), true);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-added" && f.message.contains("Foo.bar")),
        "adding namespace member Foo.bar should produce export-added finding; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_namespace_itself_still_tracked() {
    // The namespace itself (e.g. "Foo") should still appear, not just members
    let source = b"export namespace Foo { export function bar(): void {} }\n";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    assert!(
        exports.iter().any(|e| e.name == "Foo"),
        "namespace 'Foo' itself should still be in exports; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        exports.iter().any(|e| e.name == "Foo.bar"),
        "namespace member 'Foo.bar' should also be in exports; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}
