//! Export bindings compatibility checks.

use interlint::discover::tree_query::{self, RawExportKind, SourceLanguage};
use interlint::types::{ExportKey, ExportKind, ExportMap, ExportSymbol};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

/// BUG 2: `export { foo as bar }` should record `bar` as the exported name.
/// When `bar` is removed in the new version (only `foo` exported without alias),
/// the finding should mention `bar`, not `foo`.
#[test]
fn test_export_alias_detected_correctly() {
    // Old version: `export { foo as bar }` — public name is `bar`
    let old_src = b"export { foo as bar }\nfunction foo() { return 1; }";

    // New version: `export { foo }` — public name is `foo`, `bar` is gone
    let new_src = b"export { foo }\nfunction foo() { return 1; }";

    let old_exports =
        tree_query::parse_exports(old_src, SourceLanguage::TypeScript).expect("parse old");
    let new_exports =
        tree_query::parse_exports(new_src, SourceLanguage::TypeScript).expect("parse new");

    // Old should export `bar` (the alias), not `foo`
    assert!(
        old_exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
        "Old exports should contain `bar` (alias), got: {:?}",
        old_exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // New should export `foo`, not `bar`
    assert!(
        new_exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "New exports should contain `foo`, got: {:?}",
        new_exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // Old should NOT contain `foo` as a direct export name (it's the local binding)
    assert!(
        !old_exports.iter().any(|e| e.name == "foo"),
        "Old exports should NOT contain `foo` (it's the local name, not public), got: {:?}",
        old_exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // Now build ExportMaps and compare — `bar` removed should produce export-removed finding
    let mut old_map = ExportMap::new();
    for raw in &old_exports {
        old_map.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: test_file(),
                line: raw.line,
                col: raw.col,
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
            },
        );
    }

    let mut new_map = ExportMap::new();
    for raw in &new_exports {
        new_map.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: test_file(),
                line: raw.line,
                col: raw.col,
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
            },
        );
    }

    let findings =
        interlint::compare::compare_exports(&old_map, &new_map, std::path::Path::new("."), true);

    // `bar` was removed — should fire export-removed
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("bar")),
        "Should find export-removed for `bar`; got: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

/// BUG 3: `export * as utils from './helpers'` should produce a RawExport
/// with `name: "utils"`, not `name: "*"`.
#[test]
fn test_namespace_reexport_name_captured() {
    let src = b"export * as utils from './helpers'";

    let exports = tree_query::parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    let ns_export = exports
        .iter()
        .find(|e| e.name == "utils")
        .unwrap_or_else(|| panic!("No export named 'utils' found; got: {:?}", exports));

    assert_eq!(ns_export.name, "utils");
    // Should NOT be recorded as '*'
    assert!(
        !exports.iter().any(|e| e.name == "*"),
        "Namespace re-export should not be recorded as '*'"
    );
}
