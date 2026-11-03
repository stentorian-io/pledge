//! Extensions compatibility checks.

use pledge::ecosystems::node::discovery::resolver::resolve_module_path;
use std::path::Path;

/// ECF-05: resolve_module_path must preserve compound file stems.
/// `./utils.config` → candidates must include "utils.config.ts", NOT just "utils.ts".
#[test]
fn test_resolver_compound_extension() {
    let candidates = resolve_module_path(Path::new("/project/src/index.ts"), "./utils.config");

    let candidate_strings: Vec<String> = candidates
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Must have a candidate with utils.config.ts (compound stem preserved).
    assert!(
        candidate_strings
            .iter()
            .any(|s| s.contains("utils.config.ts")),
        "Candidates must include 'utils.config.ts'; got: {:?}",
        candidate_strings
    );

    // Must NOT have a candidate that is just "utils.ts" (the bug: with_extension drops .config).
    assert!(
        !candidate_strings.iter().any(|s| s.ends_with("utils.ts")),
        "Candidates must NOT contain 'utils.ts' (compound stem bug); got: {:?}",
        candidate_strings
    );
}

/// ECF-06: resolve_module_path must include .mts and .cts candidates for ESM packages.
/// Validates that modern TS extension resolution is complete.
#[test]
fn test_resolver_mts_cts_candidates() {
    let candidates = resolve_module_path(Path::new("/project/src/index.ts"), "./module");

    let candidate_strings: Vec<String> = candidates
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    assert!(
        candidate_strings.iter().any(|s| s.ends_with("module.mts")),
        "Candidates must include 'module.mts'; got: {:?}",
        candidate_strings
    );

    assert!(
        candidate_strings.iter().any(|s| s.ends_with("module.cts")),
        "Candidates must include 'module.cts'; got: {:?}",
        candidate_strings
    );

    assert!(
        candidate_strings
            .iter()
            .any(|s| s.ends_with("module.d.mts")),
        "Candidates must include 'module.d.mts'; got: {:?}",
        candidate_strings
    );

    assert!(
        candidate_strings
            .iter()
            .any(|s| s.ends_with("module.d.cts")),
        "Candidates must include 'module.d.cts'; got: {:?}",
        candidate_strings
    );
}
