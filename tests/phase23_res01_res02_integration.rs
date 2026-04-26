//! Phase 23 RES-01 / RES-02 integration tests: `node:`-prefixed and URL-scheme
//! specifiers must NOT pollute the ExportMap as synthetic external re-exports
//! AND must NOT emit any ResolveWarning.

use std::path::{Path, PathBuf};

use interlint::discover::discover_exports;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-23";

fn fixture_dir(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join("new")
}

/// Generic negative-assertion helper.
///
/// Asserts no `*<{prefix}` key in ExportMap and no warning containing `prefix`.
fn assert_silent_skip(case: &str, prefix: &str) {
    let dir = fixture_dir(case);
    let (exports, warnings) = discover_exports(&dir).expect("discover_exports should not error");

    let synthetic_marker = format!("*<{}", prefix);
    let leaked: Vec<_> = exports
        .iter()
        .filter(|(k, _)| k.name.starts_with(&synthetic_marker))
        .collect();
    assert!(
        leaked.is_empty(),
        "[{}] specifier `{}` leaked into ExportMap as synthetic entry: {:?}",
        case,
        prefix,
        leaked.iter().map(|(k, _)| &k.name).collect::<Vec<_>>()
    );

    let warns: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains(prefix))
        .collect();
    assert!(
        warns.is_empty(),
        "[{}] specifier `{}` emitted ResolveWarning(s): {:?}",
        case,
        prefix,
        warns.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn res_01_node_fs_silent_skip() {
    assert_silent_skip("res-01-node-fs", "node:");
}

#[test]
fn res_01_node_path_silent_skip() {
    assert_silent_skip("res-01-node-path", "node:");
}

#[test]
fn res_01_node_test_silent_skip() {
    assert_silent_skip("res-01-node-test", "node:");
}

#[test]
fn res_02_data_url_silent_skip() {
    assert_silent_skip("res-02-data-url", "data:");
}

#[test]
fn res_02_https_url_silent_skip() {
    assert_silent_skip("res-02-https-url", "https:");
}

#[test]
fn res_02_http_url_silent_skip() {
    assert_silent_skip("res-02-http-url", "http:");
}

#[test]
fn res_02_file_url_silent_skip() {
    assert_silent_skip("res-02-file-url", "file:");
}

/// Negative-control: `./src/node-utils.js` is a relative path whose name
/// happens to begin with `node`. The classifier MUST treat it as Relative,
/// not NodeBuiltin — i.e., it must resolve to a real file on disk.
#[test]
fn res_01_relative_shadow_resolves_normally() {
    let dir = fixture_dir("res-01-relative-shadow");
    let (exports, warnings) = discover_exports(&dir).expect("discover_exports should not error");

    // The fixture's src/node-utils.ts exports `x`. If the classifier mis-
    // classified `./src/node-utils.js` as NodeBuiltin, `x` would be missing
    // and the only entries would be (empty) or a synthetic *<...> leak.
    let has_x = exports.iter().any(|(k, _)| k.name == "x");
    assert!(
        has_x,
        "expected `x` to be resolved through the relative path; got exports: {:?} \
         and warnings: {:?}",
        exports.keys().map(|k| &k.name).collect::<Vec<_>>(),
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    // Negative side: must not have classified the path as NodeBuiltin (which
    // would have produced no synthetic entry but also no `x`). Combined with
    // the positive `has_x` check, this is dispositive.
    let leaked_node: Vec<_> = exports
        .iter()
        .filter(|(k, _)| k.name.starts_with("*<node:"))
        .collect();
    assert!(
        leaked_node.is_empty(),
        "relative path mis-classified as NodeBuiltin: {:?}",
        leaked_node
    );
}
