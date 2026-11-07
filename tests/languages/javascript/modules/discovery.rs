//! Discovery compatibility checks.

use crate::fixtures::fixture_path;
use pledge::ecosystems::node::discovery::discover_exports;

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
    for subpath in [".", "./index.js"] {
        let mut names: Vec<_> = exports
            .keys()
            .filter(|key| key.subpath == subpath)
            .map(|key| key.name.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["default", "farewell", "greet"], "{subpath}");
    }
    assert_eq!(exports.len(), 6, "root and deep imports retain all names");
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
