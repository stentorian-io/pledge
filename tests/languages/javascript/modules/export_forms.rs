//! Export forms compatibility checks.

use pledge::languages::javascript::syntax::exports::{parse_exports, RawExportKind};
use pledge::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn test_star_and_namespace_from_same_source_both_captured() {
    // Both `export * from './mod'` and `export * as utils from './mod'` should produce entries
    let source = b"export * from './mod';\nexport * as utils from './mod';";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    let stars: Vec<_> = exports
        .iter()
        .filter(|e| e.kind == RawExportKind::StarReexport && e.source.as_deref() == Some("./mod"))
        .collect();
    assert_eq!(
        stars.len(),
        2,
        "should have 2 StarReexport entries from './mod'; got: {:?}",
        stars.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(stars.iter().any(|e| e.name == "*"), "should have name='*'");
    assert!(
        stars.iter().any(|e| e.name == "utils"),
        "should have name='utils'"
    );
}

#[test]
fn test_export_default_as_named_no_from() {
    let source = b"export { default as MyComponent };";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "MyComponent" && e.kind == RawExportKind::Named),
        "export {{ default as MyComponent }} should export 'MyComponent'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_export_default_reexport_from_module() {
    let source = b"export { default } from './component';";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "default" && matches!(e.kind, RawExportKind::NamedReexport)),
        "export {{ default }} from './component' should produce NamedReexport 'default'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_export_default_as_named_reexport_from_module() {
    let source = b"export { default as Button } from './button';";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "Button" && matches!(e.kind, RawExportKind::NamedReexport)),
        "export {{ default as Button }} from './button' should export 'Button'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_mixed_export_styles_in_js_file() {
    // A JavaScript file mixing ESM and CJS exports (common in transitional codebases)
    let source = b"export function greet() {}\nexports.helper = function() {};";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    assert!(
        exports.iter().any(|e| e.name == "greet"),
        "ESM export should be captured"
    );
    assert!(
        exports.iter().any(|e| e.name == "helper"),
        "CJS export should be captured"
    );
}
