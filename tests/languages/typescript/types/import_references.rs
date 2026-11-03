//! Cross-file `typeof import('./y').Z` references with compiler evidence and
//! explicit degraded-mode findings when that evidence is unavailable.
//!
//! Run with `--test typescript types::import_references --features lsp-tests`
//! and `-- --test-threads=1` to include the language-server cases.

use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures/typescript/types/import-references";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

#[cfg(feature = "lsp-tests")]
mod lsp_path {
    use super::*;
    use pledge::core::findings::{Finding, Severity};
    use pledge::ecosystems::node::discovery::discover_exports;
    use pledge::languages::javascript::compare::compare_exports;
    use pledge::languages::typescript::compare::rules::RULE_TYPE_REFERENCE_REMOVED;
    use pledge::languages::typescript::compiler::enrich_exports;
    use tokio::sync::Mutex;

    // Serialize server sessions that share fixture roots. Concurrent initialization
    // can otherwise make definition requests intermittently time out or return null.
    static LSP_TEST_LOCK: Mutex<()> = Mutex::const_new(());

    async fn run_with_lsp(form: &str) -> Vec<Finding> {
        // Keep shared-workspace server sessions separate without blocking the runtime.
        let _lsp_guard = LSP_TEST_LOCK.lock().await;

        let old_dir = fixture_dir(form, "old");
        let new_dir = fixture_dir(form, "new");
        let (mut old_exports, _) = discover_exports(&old_dir).unwrap();
        let (mut new_exports, _) = discover_exports(&new_dir).unwrap();
        let _ = enrich_exports(&mut old_exports, &old_dir).await;
        let _ = enrich_exports(&mut new_exports, &new_dir).await;
        // LOCKED wiring: 4th arg `lsp_was_available: bool`.
        compare_exports(&old_exports, &new_exports, &new_dir, true)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn clean_no_finding() {
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
    async fn removed_fires_major() {
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
    async fn multi_ref() {
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
    use pledge::core::findings::Finding;
    use pledge::ecosystems::node::discovery::discover_exports;
    use pledge::languages::javascript::compare::compare_exports;
    use pledge::languages::typescript::compare::rules::RULE_TYPE_REFERENCE_REMOVED;

    #[test]
    fn degraded_annotation() {
        let old_dir = fixture_dir("degraded", "old");
        let new_dir = fixture_dir("degraded", "new");
        let (old_exports, _) = discover_exports(&old_dir).unwrap();
        let (new_exports, _) = discover_exports(&new_dir).unwrap();
        // DO NOT call enrich_exports — simulate degraded mode.
        // LOCKED wiring: pass lsp_was_available=false through
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
