//! Commonjs aliases compatibility checks.

use interlint::discover::tree_query::{parse_exports, SourceLanguage};

#[test]
fn cjs_exports_aliasing_pattern() {
    // exports = module.exports = { foo, bar };
    let source = b"exports = module.exports = { foo, bar };";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "foo"),
        "Should capture 'foo' from CJS aliasing pattern; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "bar"),
        "Should capture 'bar' from CJS aliasing pattern; got: {:?}",
        names
    );
}

#[test]
fn cjs_aliasing_multiple_properties() {
    // exports = module.exports = { foo, bar, baz };
    // All three properties should be captured
    let source = b"exports = module.exports = { foo, bar, baz };";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "foo"),
        "Should capture 'foo' from CJS aliasing; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "bar"),
        "Should capture 'bar' from CJS aliasing; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "baz"),
        "Should capture 'baz' from CJS aliasing; got: {:?}",
        names
    );
}
