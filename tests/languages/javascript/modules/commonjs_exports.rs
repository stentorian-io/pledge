//! Commonjs exports compatibility checks.

use interlint::discover::tree_query::{parse_exports, RawExportKind, SourceLanguage};

#[test]
fn test_cjs_module_exports_bulk_assignment() {
    let source = b"const foo = 1;\nconst bar = 2;\nmodule.exports = { foo, bar };";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "CJS bulk export should capture foo; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
        "CJS bulk export should capture bar; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_cjs_exports_property_assignment() {
    let source = b"exports.hello = function() {};\nexports.world = 42;";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    assert!(exports
        .iter()
        .any(|e| e.name == "hello" && e.kind == RawExportKind::Named));
    assert!(exports
        .iter()
        .any(|e| e.name == "world" && e.kind == RawExportKind::Named));
}

#[test]
fn test_cjs_module_exports_single_default() {
    let source = b"module.exports = MyClass;";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "default" && e.kind == RawExportKind::Default),
        "CJS single export should be treated as default; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_cjs_not_detected_in_typescript() {
    // CJS patterns should be ignored in TypeScript (which uses `export =` instead)
    let source = b"module.exports = { foo: 1 };";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    assert!(
        !exports.iter().any(|e| e.name == "foo"),
        "CJS should not fire in TypeScript; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}
