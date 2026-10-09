//! Discovery compatibility checks.

use interlint::discover::discover_exports;

use crate::fixtures::fixture_path;

#[test]
fn test_discover_simple_js_exports() {
    let (exports, _warnings) =
        discover_exports(&fixture_path("javascript/modules/basic/old")).unwrap();
    assert!(
        exports.keys().any(|k| k.name == "greet"),
        "should find named export 'greet'"
    );
    assert!(
        exports.keys().any(|k| k.name == "farewell"),
        "should find named export 'farewell'"
    );
    assert!(
        exports.keys().any(|k| k.name == "default"),
        "should find default export"
    );
    assert_eq!(exports.len(), 3, "should find exactly 3 exports");
}

#[test]
fn test_discover_reexports() {
    let (exports, _warnings) =
        discover_exports(&fixture_path("javascript/modules/reexports")).unwrap();
    assert!(
        exports.keys().any(|k| k.name == "greet"),
        "should find re-exported 'greet'"
    );
    assert!(
        exports.keys().any(|k| k.name == "helper"),
        "should find star re-exported 'helper'"
    );
}
