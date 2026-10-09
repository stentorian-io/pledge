//! Augmentations compatibility checks.

use interlint::languages::javascript::compare::compare_exports;
use interlint::languages::javascript::model::{ExportKey, ExportKind, ExportMap, ExportSymbol};
use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;
use std::path::PathBuf;

fn raw_exports_to_map(
    raw: Vec<interlint::languages::javascript::syntax::exports::RawExport>,
) -> ExportMap {
    let mut map = ExportMap::new();
    for e in raw {
        map.insert(
            ExportKey::new_root_value(&e.name),
            ExportSymbol {
                name: e.name,
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: e.line,
                col: e.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: e.decl_kind.map(|k| vec![k]).unwrap_or_default(),
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
    map
}

#[test]
fn declare_module_captured() {
    // declare module 'express' { interface Request { user?: any; } }
    // Expected: augment:express in exports
    let source = b"declare module 'express' { interface Request { user?: any; } }\n";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    assert!(
        exports.iter().any(|e| e.name == "augment:express"),
        "declare module 'express' should produce augment:express entry; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn declare_global_captured() {
    // declare global { interface Window { myLib: any; } }
    // Expected: augment:global in exports
    let source = b"declare global { interface Window { myLib: any; } }\n";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    assert!(
        exports.iter().any(|e| e.name == "augment:global"),
        "declare global should produce augment:global entry; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn declare_module_removal() {
    // Old: declare module 'express' { ... } + export declare function foo(): void;
    // New: export declare function foo(): void;
    // Expected: export-removed finding for augment:express
    let old_source =
        b"declare module 'express' { interface Request { user?: any; } }\nexport declare function foo(): void;\n";
    let new_source = b"export declare function foo(): void;\n";

    let old_raw = parse_exports(old_source, SourceLanguage::TypeScript).unwrap();
    let new_raw = parse_exports(new_source, SourceLanguage::TypeScript).unwrap();

    let old_map = raw_exports_to_map(old_raw);
    let new_map = raw_exports_to_map(new_raw);

    // Verify old has augment:express
    assert!(
        old_map.contains_key(&ExportKey::new_root_value("augment:express")),
        "old exports should contain augment:express; got: {:?}",
        old_map.keys().collect::<Vec<_>>()
    );

    let findings = compare_exports(&old_map, &new_map, std::path::Path::new("."), true);

    assert!(
        findings.iter().any(|f| f.rule == "export-removed" && f.message.contains("augment:express")),
        "removing declare module 'express' should produce export-removed for augment:express; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}
