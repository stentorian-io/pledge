//! Phase 19 META-06 regression guard: release tag detection uniformity.
//!
//! For every RawExport construction form that the discovery pipeline
//! supports, a `/** @deprecated */` block comment placed on the export
//! MUST surface via `compare_exports` as a `RULE_EXPORT_DEPRECATED` Warning
//! when the same symbol exists in both versions. If any form regresses,
//! the post-process populate pass at `src/discover/resolver.rs:~420` has
//! lost a construction site.

use std::path::{Path, PathBuf};

use interlint::compare::compare_exports;
use interlint::compare::subtype::RULE_EXPORT_DEPRECATED;
use interlint::discover::discover_exports;
use interlint::types::Severity;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-19/release-tag-forms";

/// Each fixture form: directory name under FIXTURE_ROOT and the symbol name
/// whose `@deprecated` transition between old/ and new/ we expect to detect.
const FORMS: &[(&str, &str)] = &[
    ("default-export", "default"),
    ("named-reexport", "foo"),
    ("destructured", "foo"),
    ("compound-assignment", "foo"),
    ("star-reexport", "foo"),
    ("bun-01-esbuild-treeshake", "foo"),
    ("bun-02-esbuild-export-helper", "foo"),
    ("bun-04-tsc-void-exports", "foo"),
    ("bun-05-swc-star-reexport", "foo"),
    ("bun-06a-object-define", "foo"),
    ("bun-06b-bracket-subscript", "foo"),
];

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

#[test]
fn meta_06_regression_guard_all_forms() {
    let mut failures: Vec<String> = Vec::new();

    for (form, symbol) in FORMS {
        let old_dir = fixture_dir(form, "old");
        let new_dir = fixture_dir(form, "new");

        assert!(
            old_dir.exists(),
            "META-06 fixture missing: {}",
            old_dir.display()
        );
        assert!(
            new_dir.exists(),
            "META-06 fixture missing: {}",
            new_dir.display()
        );

        let (old_exports, _old_warnings) = match discover_exports(&old_dir) {
            Ok(result) => result,
            Err(e) => {
                failures.push(format!(
                    "form `{}`: discover_exports on old/ failed: {}",
                    form, e
                ));
                continue;
            }
        };
        let (new_exports, _new_warnings) = match discover_exports(&new_dir) {
            Ok(result) => result,
            Err(e) => {
                failures.push(format!(
                    "form `{}`: discover_exports on new/ failed: {}",
                    form, e
                ));
                continue;
            }
        };

        let findings = compare_exports(&old_exports, &new_exports, &new_dir, true);

        let deprecated_hits: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_EXPORT_DEPRECATED)
            .collect();

        if deprecated_hits.is_empty() {
            failures.push(format!(
                "form `{}` symbol `{}`: expected 1 `export-deprecated` Warning, got 0. Findings: {:?}",
                form,
                symbol,
                findings.iter().map(|f| (f.rule, f.message.clone())).collect::<Vec<_>>()
            ));
            continue;
        }

        let matches_symbol = deprecated_hits
            .iter()
            .any(|f| f.message.contains(symbol) || *symbol == "default");
        if !matches_symbol {
            failures.push(format!(
                "form `{}`: export-deprecated fired but not for symbol `{}`. Messages: {:?}",
                form,
                symbol,
                deprecated_hits
                    .iter()
                    .map(|f| f.message.clone())
                    .collect::<Vec<_>>()
            ));
            continue;
        }

        // Severity must be Warning (per existing META-04 emission at compare/mod.rs:288-302).
        let severity_ok = deprecated_hits
            .iter()
            .any(|f| f.severity == Severity::Warning);
        if !severity_ok {
            failures.push(format!(
                "form `{}`: export-deprecated fired but not at Warning severity. Severities: {:?}",
                form,
                deprecated_hits
                    .iter()
                    .map(|f| f.severity.clone())
                    .collect::<Vec<_>>()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "META-06 regression guard: {} of {} forms failed:\n{}",
        failures.len(),
        FORMS.len(),
        failures.join("\n")
    );
}
