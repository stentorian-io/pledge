//! Source extensions compatibility checks.

use pledge::ecosystems::node::discovery::resolver::resolve_module_path;
use std::path::Path;

#[test]
fn test_js_extension_probes_ts() {
    let candidates = resolve_module_path(Path::new("/project/src/index.ts"), "./utils.js");
    let has_ts = candidates.iter().any(|p| {
        let s = p.to_string_lossy();
        s.ends_with("/utils.ts") && !s.ends_with(".js.ts")
    });
    assert!(
        has_ts,
        "specifier ./utils.js should probe utils.ts; got: {candidates:?}"
    );
}

#[test]
fn test_mjs_extension_probes_mts() {
    let candidates = resolve_module_path(Path::new("/project/src/index.ts"), "./helper.mjs");
    let has_mts = candidates
        .iter()
        .any(|p| p.to_string_lossy().ends_with("/helper.mts"));
    assert!(
        has_mts,
        "specifier ./helper.mjs should probe helper.mts; got: {candidates:?}"
    );
}

#[test]
fn test_cjs_extension_probes_cts() {
    let candidates = resolve_module_path(Path::new("/project/src/index.ts"), "./config.cjs");
    let has_cts = candidates
        .iter()
        .any(|p| p.to_string_lossy().ends_with("/config.cts"));
    assert!(
        has_cts,
        "specifier ./config.cjs should probe config.cts; got: {candidates:?}"
    );
}

#[test]
fn test_directory_index_completeness() {
    let candidates = resolve_module_path(Path::new("/project/src/index.ts"), "./components");
    let expected = [
        "index.ts",
        "index.tsx",
        "index.mts",
        "index.cts",
        "index.js",
        "index.jsx",
        "index.mjs",
        "index.cjs",
        "index.d.ts",
        "index.d.mts",
        "index.d.cts",
    ];
    for name in &expected {
        let full = format!("components/{name}");
        let found = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with(&full));
        assert!(found, "candidates must include {full}; got: {candidates:?}");
    }
}
