//! Discovery compatibility checks.

use interlint::discover::discover_exports;

use crate::fixtures::fixture_path;

#[test]
fn test_discover_typescript_exports() {
    let (exports, _warnings) =
        discover_exports(&fixture_path("typescript/declarations/basic")).unwrap();
    assert!(
        exports.keys().any(|k| k.name == "VERSION"),
        "should find const export 'VERSION'"
    );
    assert!(
        exports.keys().any(|k| k.name == "Config"),
        "should find interface export 'Config'"
    );
    assert!(
        exports.keys().any(|k| k.name == "configure"),
        "should find function export 'configure'"
    );
    assert!(
        exports.keys().any(|k| k.name == "default"),
        "should find default export 'App'"
    );
}
