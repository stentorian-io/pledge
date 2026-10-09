//! Exports compatibility checks.

use interlint::languages::javascript::compare::compare_exports;
use interlint::languages::javascript::model::{ExportKey, ExportKind, ExportMap, ExportSymbol};
use interlint::languages::javascript::syntax::exports::{parse_exports, RawExportKind};
use interlint::languages::javascript::syntax::grammar::SourceLanguage;
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

/// ECF-01: `export type { Foo, Bar }` should produce 2 Named RawExports.
/// Validates that type-only export statements flow through parse_exports.
#[test]
fn test_type_only_export_captured() {
    let src = b"export type { Foo, Bar };";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        2,
        "Expected 2 exports from type-only export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    assert!(
        exports
            .iter()
            .any(|e| e.name == "Foo" && e.kind == RawExportKind::Named),
        "Expected export named 'Foo' with kind Named; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );

    assert!(
        exports
            .iter()
            .any(|e| e.name == "Bar" && e.kind == RawExportKind::Named),
        "Expected export named 'Bar' with kind Named; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

/// ECF-01: `export type { Foo } from './other'` should produce a NamedReexport.
/// Validates that type-only re-exports record the source module specifier.
#[test]
fn test_type_only_reexport_captured() {
    let src = b"export type { Foo } from './other';";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        1,
        "Expected 1 export from type-only re-export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    let exp = &exports[0];
    assert_eq!(exp.name, "Foo", "Export name should be 'Foo'");
    assert_eq!(
        exp.kind,
        RawExportKind::NamedReexport,
        "Type-only re-export should have kind NamedReexport"
    );
    assert!(
        exp.source
            .as_deref()
            .map(|s| s.contains("other"))
            .unwrap_or(false),
        "Source should contain 'other'; got: {:?}",
        exp.source
    );
}

/// ECF-01: Type-only exports must participate in the diff pipeline.
/// Old has "Foo" (from type-only export), new is empty → export-removed fires.
#[test]
fn test_type_only_export_removal_detected() {
    let old_src = b"export type { Foo };";
    let new_src = b"";

    let old_raw = parse_exports(old_src, SourceLanguage::TypeScript).expect("parse old");
    let new_raw = parse_exports(new_src, SourceLanguage::TypeScript).expect("parse new");

    let mut old_map = ExportMap::new();
    for raw in &old_raw {
        old_map.insert(
            ExportKey::new_root_value(&raw.name),
            make_export_sym(&raw.name),
        );
    }

    let mut new_map = ExportMap::new();
    for raw in &new_raw {
        new_map.insert(
            ExportKey::new_root_value(&raw.name),
            make_export_sym(&raw.name),
        );
    }

    let findings = compare_exports(&old_map, &new_map, std::path::Path::new("."), true);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("Foo")),
        "Expected export-removed for 'Foo'; got: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

/// ECF-03: `export = MyClass` is a CJS module.exports equivalent.
/// Should produce 1 RawExport with name="default" and kind=Default.
#[test]
fn test_export_assignment_captured() {
    let src = b"export = MyClass;";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        1,
        "Expected 1 export from export assignment; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    let exp = &exports[0];
    assert_eq!(
        exp.name, "default",
        "export = ... should use name 'default'; got: '{}'",
        exp.name
    );
    assert_eq!(
        exp.kind,
        RawExportKind::Default,
        "export = ... should have kind Default; got: {:?}",
        exp.kind
    );
}

/// ECF-04: Ambient declarations in .d.ts-style sources should be captured.
/// `export declare function/class/const` forms must all produce Named exports.
#[test]
fn test_ambient_declarations_captured() {
    let src = b"
export declare function greet(name: string): string;
export declare class Logger {}
export declare const VERSION: string;
";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        3,
        "Expected 3 exports from ambient declarations; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    for expected_name in ["greet", "Logger", "VERSION"] {
        assert!(
            exports.iter().any(|e| e.name == expected_name),
            "Expected export named '{}'; got: {:?}",
            expected_name,
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }
}

/// ECF-01/02: Mixed export statement with both type-only and regular named exports
/// are both captured correctly.
#[test]
fn test_type_and_value_exports_in_same_file() {
    let src = b"
export const foo = 42;
export type { MyType };
export interface MyInterface {}
";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        names.contains(&"foo"),
        "Expected 'foo' in exports; got: {:?}",
        names
    );
    assert!(
        names.contains(&"MyType"),
        "Expected 'MyType' (type-only export) in exports; got: {:?}",
        names
    );
    assert!(
        names.contains(&"MyInterface"),
        "Expected 'MyInterface' (interface export) in exports; got: {:?}",
        names
    );
}
