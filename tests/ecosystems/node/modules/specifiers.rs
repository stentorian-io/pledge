//! RES-01 / RES-02 integration tests: `node:`-prefixed and URL-scheme
//! specifiers must NOT pollute the ExportMap as synthetic external re-exports
//! while preserving explicit incomplete evidence for unmodeled host surfaces.

use pledge::ecosystems::node::discovery::discover_exports;
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures";

fn fixture_dir(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join("new")
}

/// Unmodeled host interfaces stay incomplete without inventing external names.
fn assert_host_evidence_limit(case: &str, prefix: &str) {
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
        warns.iter().any(|warning| warning.incomplete),
        "[{}] specifier `{}` did not retain incomplete evidence: {:?}",
        case,
        prefix,
        warns.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn node_fs_retains_missing_evidence() {
    assert_host_evidence_limit("node/modules/specifiers/node-fs", "node:");
}

#[test]
fn node_path_retains_missing_evidence() {
    assert_host_evidence_limit("node/modules/specifiers/node-path", "node:");
}

#[test]
fn node_test_retains_missing_evidence() {
    assert_host_evidence_limit("node/modules/specifiers/node-test", "node:");
}

#[test]
fn data_url_retains_missing_evidence() {
    assert_host_evidence_limit("node/modules/specifiers/data-url", "data:");
}

#[test]
fn https_url_retains_missing_evidence() {
    assert_host_evidence_limit("node/modules/specifiers/https-url", "https:");
}

#[test]
fn http_url_retains_missing_evidence() {
    assert_host_evidence_limit("node/modules/specifiers/http-url", "http:");
}

#[test]
fn file_url_retains_missing_evidence() {
    assert_host_evidence_limit("node/modules/specifiers/file-url", "file:");
}

/// Negative-control: `./src/node-utils.js` is a relative path whose name
/// happens to begin with `node`. The classifier MUST treat it as Relative,
/// not NodeBuiltin — i.e., it must resolve to a real file on disk.
#[test]
fn relative_shadow_resolves_normally() {
    let dir = fixture_dir("node/modules/specifiers/relative-shadow");
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
