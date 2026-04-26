//! Phase 22 INF-04 integration tests: `typeof import('./y').Z` cross-file
//! type-reference detection.
//!
//! Split into two modules:
//!   - `lsp_path` (behind `lsp-tests` feature): tests that require a real
//!     typescript-language-server to prove the verified Major-emission path.
//!     Serialized via a process-wide `LSP_TEST_LOCK` Mutex so async-lsp
//!     0.2.3's per-process client race cannot fire (Phase 27 closure of
//!     INF-04 harness tech-debt).
//!   - `degraded_path` (default suite): tests the degraded-mode limitation
//!     annotation; exercises the diff pipeline directly with
//!     lsp_was_available=false, no subprocess needed.
//!
//! CONTEXT.md D-08: degraded message contains `(limitation: lsp-required)`.
//! Rule-ID for both branches: `type-reference-removed` (Major / Minor).
//!
//! Recommended LSP-suite invocation (developer-only — not part of `make check`):
//!   cargo test --features lsp-tests --test phase22_inf04_integration -- --test-threads=1 --nocapture

use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures/phase-22/inf-04-typeof-import";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

#[cfg(feature = "lsp-tests")]
mod lsp_path {
    use super::*;
    use interlint::compare::compare_exports;
    use interlint::compare::subtype::RULE_TYPE_REFERENCE_REMOVED;
    use interlint::discover::discover_exports;
    use interlint::lsp::enrich_exports;
    use interlint::types::{Finding, Severity};
    use std::sync::Mutex;

    /// INF-04 (Phase 22, closed Phase 27): harness-level serialization for
    /// LSP-gated tests.
    ///
    /// async-lsp 0.2.3 has a race when two `typescript-language-server`
    /// subprocesses are spawned against the same workspace root within
    /// the same process: `textDocument/definition` requests on one client
    /// can intermittently time out or return null while the other client
    /// is still initializing. Serializing the entire spawn-request-shutdown
    /// window per-test eliminates the race without depending on the
    /// runner's `--test-threads` value.
    ///
    /// Why a static `Mutex` and not the `serial_test` crate:
    ///   1. Zero new dependency surface (the project does not already
    ///      carry `serial_test`).
    ///   2. The lock is gated to `#[cfg(feature = "lsp-tests")]`, so it
    ///      has zero compile or runtime cost in the default `cargo test`
    ///      build — only present when the developer opts in.
    ///   3. Behaviour is equivalent to the per-test `#[serial]` attribute:
    ///      the lock is acquired at the head of `run_with_lsp` (the only
    ///      function in this module that spawns a subprocess) and
    ///      released when the guard goes out of scope at the end of the
    ///      test body.
    ///
    /// Recommended invocation (documented for developers who run the LSP
    /// suite manually):
    /// ```text
    /// cargo test --features lsp-tests --test phase22_inf04_integration -- --test-threads=1 --nocapture
    /// ```
    /// The `--test-threads=1` flag is no longer REQUIRED for correctness
    /// (the lock provides correctness on its own), but it is still
    /// RECOMMENDED to keep test output linear when reading subprocess
    /// stderr via `--nocapture`. Without `--test-threads=1`, multiple
    /// tokio runtimes will queue at the lock and their stdout/stderr
    /// can interleave.
    ///
    /// `make check` does NOT run the lsp-tests feature — the LSP harness
    /// is a developer-only path. The `degraded_path` module exercises
    /// the same `(limitation: lsp-required)` annotation logic without
    /// any subprocess and runs in the canonical gate.
    static LSP_TEST_LOCK: Mutex<()> = Mutex::new(());

    async fn run_with_lsp(form: &str) -> Vec<Finding> {
        // INF-04 harness gate: serialize subprocess-spawning LSP tests so
        // async-lsp 0.2.3's per-process tsserver client race cannot fire.
        // Acquired BEFORE any path computation so the entire spawn -> request
        // -> shutdown window for one test is exclusive against any other
        // running test in this binary. `expect` is correct here — a poisoned
        // mutex means a previous test panicked while holding the lock; a
        // panic in a test should fail subsequent tests of the same binary
        // hard rather than silently mask the LSP race.
        let _lsp_guard = LSP_TEST_LOCK
            .lock()
            .expect("LSP_TEST_LOCK poisoned — a previous LSP test panicked while holding the lock");

        let old_dir = fixture_dir(form, "old");
        let new_dir = fixture_dir(form, "new");
        let (mut old_exports, _) = discover_exports(&old_dir).unwrap();
        let (mut new_exports, _) = discover_exports(&new_dir).unwrap();
        let _ = enrich_exports(&mut old_exports, &old_dir).await;
        let _ = enrich_exports(&mut new_exports, &new_dir).await;
        // Plan 22-04 Task 5 LOCKED wiring: 4th arg `lsp_was_available: bool`.
        compare_exports(&old_exports, &new_exports, &new_dir, true)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn inf_04_clean_no_finding() {
        let findings = run_with_lsp("clean").await;
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_TYPE_REFERENCE_REMOVED)
            .collect();
        assert_eq!(
            hits.len(),
            0,
            "clean fixture (Z unchanged) must produce zero type-reference-removed findings; got: {:?}",
            hits.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn inf_04_removed_fires_major() {
        let findings = run_with_lsp("removed").await;
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_TYPE_REFERENCE_REMOVED)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "removed fixture must produce exactly 1 type-reference-removed; got: {:?}",
            findings
                .iter()
                .map(|f| (f.rule, &f.message))
                .collect::<Vec<_>>()
        );
        assert!(
            matches!(hits[0].severity, Severity::Major),
            "verified removal must be Major; got {:?}",
            hits[0].severity
        );
        assert!(
            hits[0].message.contains("./y") && hits[0].message.contains(".Z"),
            "message should mention specifier and property; got: {}",
            hits[0].message
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn inf_04_multi_ref() {
        let findings = run_with_lsp("multi-ref").await;
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_TYPE_REFERENCE_REMOVED)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "multi-ref should fire exactly one finding (only B1 removed); got: {:?}",
            hits.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
        assert!(
            hits[0].message.contains("./b") && hits[0].message.contains("B1"),
            "message should mention ./b and B1 (the removed ref); got: {}",
            hits[0].message
        );
    }
}

mod degraded_path {
    use super::*;
    use interlint::compare::compare_exports;
    use interlint::compare::subtype::RULE_TYPE_REFERENCE_REMOVED;
    use interlint::discover::discover_exports;
    use interlint::types::Finding;

    #[test]
    fn inf_04_degraded_annotation() {
        let old_dir = fixture_dir("degraded", "old");
        let new_dir = fixture_dir("degraded", "new");
        let (old_exports, _) = discover_exports(&old_dir).unwrap();
        let (new_exports, _) = discover_exports(&new_dir).unwrap();
        // DO NOT call enrich_exports — simulate degraded mode.
        // Plan 22-04 Task 5 LOCKED wiring: pass lsp_was_available=false through
        // the 4th argument of compare_exports. The diff pipeline reads this
        // flag and emits the `(limitation: lsp-required)` branch for every
        // ExportSymbol with non-empty typeof_import_refs.
        let findings: Vec<Finding> = compare_exports(&old_exports, &new_exports, &new_dir, false);

        let hits: Vec<_> = findings
            .iter()
            .filter(|f| {
                f.rule == RULE_TYPE_REFERENCE_REMOVED
                    && f.message.contains("(limitation: lsp-required)")
            })
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "degraded mode must emit 1 finding with `(limitation: lsp-required)`; got: {:?}",
            findings
                .iter()
                .map(|f| (f.rule, &f.message))
                .collect::<Vec<_>>()
        );
    }
}
