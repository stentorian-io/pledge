//! Nested namespaces compatibility checks.

use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn nested_namespace_two_levels() {
    let source = b"export namespace A { export namespace B { export function baz(): void {} } }";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "A.B"),
        "Should have 'A.B' for nested namespace; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "A.B.baz"),
        "Should have 'A.B.baz' for doubly-nested function; got: {:?}",
        names
    );
}

#[test]
fn nested_namespace_includes_parent() {
    let source = b"export namespace A { export namespace B { export const x = 1; } }";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    // Top-level namespace A should also be present
    assert!(
        exports.iter().any(|e| e.name == "A"),
        "Top-level namespace 'A' should also be exported; got: {:?}",
        names
    );
}
