//! Inline type exports compatibility checks.

use interlint::discover::tree_query::{parse_exports, SourceLanguage};

#[test]
fn test_inline_type_modifier_both_captured() {
    // `export { type Foo, Bar }` — both should appear exactly once
    let source = b"export { type Foo, Bar };";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    let foo_count = exports.iter().filter(|e| e.name == "Foo").count();
    let bar_count = exports.iter().filter(|e| e.name == "Bar").count();
    assert_eq!(
        foo_count,
        1,
        "Foo should appear exactly once; got {foo_count} in {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        bar_count,
        1,
        "Bar should appear exactly once; got {bar_count} in {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}
