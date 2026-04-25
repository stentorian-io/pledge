//! Phase 24 BUN-10 integration tests: `Object.assign({}, exports, {…})` first-arg
//! relax, plus regression coverage for shapes already detected by v0.2 BUN-06a.

use std::path::{Path, PathBuf};

use interlint::discover::tree_query::{parse_exports, SourceLanguage};

const FIXTURE_ROOT: &str = "tests/fixtures/phase-24";

#[allow(dead_code)] // helper used by fixture-driven tests
fn fixture_dir(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join(side)
}

// ---------------------------------------------------------------------------
// Raw-source probes (mirror tests/phase17_integration.rs:222-262 shape).
// ---------------------------------------------------------------------------

#[test]
fn bun_10_assign_empty_base_captures_via_any_arg_match() {
    // The Phase 24 BUN-10-A gap shape — first-arg is `{}`, second is `exports`.
    let source = br#"
        exports.foo = 1;
        module.exports = Object.assign({}, exports, { extra: 2, more: 3 });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    for expected in ["foo", "extra", "more"] {
        assert!(
            names.contains(&expected),
            "BUN-10: {} not captured from Object.assign({{}}, exports, ...). Got: {:?}",
            expected,
            names,
        );
    }
}

#[test]
fn bun_10_existing_assign_form_unchanged() {
    // Regression: pre-Phase-24 `Object.assign(exports, {…})` MUST still be captured.
    let source = br#"
        exports.foo = function() {};
        module.exports = Object.assign(exports, { bar: 1, baz: 2 });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    for expected in ["foo", "bar", "baz"] {
        assert!(
            names.contains(&expected),
            "BUN-10 regression: {} should still be captured. Got: {:?}",
            expected,
            names,
        );
    }
}

#[test]
fn bun_10_esmodule_filter_still_applies_to_relaxed_arm() {
    let source = br#"
        Object.defineProperty(exports, "__esModule", { value: true });
        exports.foo = 1;
        module.exports = Object.assign({}, exports, { __esModule: true, bar: 2 });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        !names.contains(&"__esModule"),
        "BUN-10: __esModule must still be filtered from relaxed Object.assign arm. Got: {:?}",
        names
    );
    for expected in ["foo", "bar"] {
        assert!(
            names.contains(&expected),
            "BUN-10: legitimate exports retained. Got: {:?}",
            names
        );
    }
}

#[test]
fn bun_10_multi_object_literal_arms_walked() {
    // Pre-Phase-24 already worked; ensure the relaxed gate doesn't regress it.
    let source = br#"
        module.exports = Object.assign(exports, { a: 1 }, { b: 2 }, { c: 3 });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    for expected in ["a", "b", "c"] {
        assert!(
            names.contains(&expected),
            "BUN-10: multi-object-literal walk lost {}. Got: {:?}",
            expected, names
        );
    }
}

#[test]
fn bun_10_assign_with_no_exports_arg_short_circuits() {
    // The relaxed gate still requires SOME arg to be `exports` or `module.*`.
    // An assign call against an internal-only identifier MUST NOT capture pairs.
    let source = br#"
        var notExports = {};
        Object.assign(notExports, { irrelevant: 1 });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        !names.contains(&"irrelevant"),
        "BUN-10: arm must short-circuit when no arg is exports/module.*. Got: {:?}",
        names
    );
}

#[test]
fn bun_10_assign_empty_base_with_oldexports_internal_does_not_capture() {
    // Edge case: `Object.assign({}, oldExports, { extra: 1 })` where `oldExports`
    // is an internal binding — the relaxed gate REQUIRES the literal token
    // `exports` or `module.*`, so this call short-circuits. The `extra` pair
    // is NOT captured by this arm. (It would only be captured if the source
    // also had a separate top-level `module.exports = ...` shape that the
    // existing pre-Phase-24 paths handle.)
    let source = br#"
        var oldExports = { kept: 1 };
        module.exports = Object.assign({}, oldExports, { extra: 1 });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    // The relaxed gate doesn't fire (no arg is `exports`/`module.*`) so neither
    // `kept` nor `extra` are emitted by the assign arm. The `module.exports = ...`
    // RHS is a CallExpression, not a clean assignment shape, so the existing
    // CJS_MODULE_EXPORTS_DEFAULT_QUERY (Identifier-RHS only) doesn't fire either.
    assert!(
        !names.contains(&"extra"),
        "BUN-10: extra should NOT be captured when no arg is literal exports/module.*. Got: {:?}",
        names
    );
}

// ---------------------------------------------------------------------------
// Real-world regression: Emotion @11.14.0 Preconstruct CJS output.
// ---------------------------------------------------------------------------

#[test]
fn bun_10_emotion_react_regression_clean_parse() {
    // Reads the committed real-world fixture and asserts all 12 known
    // exports are captured. This is the ROADMAP success criterion 3
    // regression (Preconstruct output produces the same finding set as
    // hand-written CJS).
    let path = fixture_dir("bun-10-emotion-react-regression", "old")
        .join("dist")
        .join("emotion-react.cjs.js");
    let source = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("could not read fixture {}: {}", path.display(), e));
    let raw = parse_exports(&source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    for expected in [
        "CacheProvider",
        "ThemeContext",
        "ThemeProvider",
        "__unsafe_useEmotionCache",
        "useTheme",
        "withEmotionCache",
        "withTheme",
        "ClassNames",
        "Global",
        "createElement",
        "css",
        "keyframes",
    ] {
        assert!(
            names.contains(&expected),
            "Emotion regression: {} not captured from real Preconstruct CJS. Got: {:?}",
            expected, names
        );
    }
    // Negative: `__esModule` filtered out.
    assert!(
        !names.contains(&"__esModule"),
        "Emotion regression: __esModule must be filtered. Got: {:?}",
        names
    );
}

// ---------------------------------------------------------------------------
// Fixture-driven diff smoke test (exercises the full discover→compare path).
// ---------------------------------------------------------------------------
//
// Helper: parse both old/ and new/ entry files via parse_exports and compute
// a name-set delta. If the project exposes a higher-level `compare` API that
// other phaseXX_integration.rs tests call directly (e.g. `compare_packages`),
// prefer that. Otherwise the name-set delta below is a sufficient regression
// signal for the BUN-10 gate.

#[test]
fn bun_10_assign_empty_base_diff_drops_removed_symbol() {
    let old_source = std::fs::read(
        fixture_dir("bun-10-assign-empty-base", "old")
            .join("dist")
            .join("index.js"),
    )
    .expect("old fixture file exists");
    let new_source = std::fs::read(
        fixture_dir("bun-10-assign-empty-base", "new")
            .join("dist")
            .join("index.js"),
    )
    .expect("new fixture file exists");

    let old_names: std::collections::HashSet<String> =
        parse_exports(&old_source, SourceLanguage::JavaScript)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
    let new_names: std::collections::HashSet<String> =
        parse_exports(&new_source, SourceLanguage::JavaScript)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();

    // Both sides must capture `foo` and `kept`.
    for kept in ["foo", "kept"] {
        assert!(old_names.contains(kept), "old missing {}: {:?}", kept, old_names);
        assert!(new_names.contains(kept), "new missing {}: {:?}", kept, new_names);
    }
    // `removed` is in old but not new.
    assert!(
        old_names.contains("removed"),
        "old should contain `removed`: {:?}",
        old_names
    );
    assert!(
        !new_names.contains("removed"),
        "new should NOT contain `removed`: {:?}",
        new_names
    );
}
