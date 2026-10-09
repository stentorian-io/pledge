//! Nested destructuring compatibility checks.

use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn nested_object_destructuring() {
    let source = b"export const { a: { b, c } } = obj;";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "b"),
        "Should capture 'b' from nested destructuring; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "c"),
        "Should capture 'c' from nested destructuring; got: {:?}",
        names
    );
}

#[test]
fn nested_array_destructuring() {
    let source = b"export const [a, [b, c]] = arr;";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "a"),
        "Should capture 'a'; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "b"),
        "Should capture 'b' from nested array destructuring; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "c"),
        "Should capture 'c' from nested array destructuring; got: {:?}",
        names
    );
}
