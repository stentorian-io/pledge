//! BUN-10 integration tests: `Object.assign({}, exports, {…})` first-arg
//! relax, plus regression coverage for shapes already detected by v0.2 BUN-06a.

use pledge::languages::javascript::syntax::exports::parse_exports;
use pledge::languages::javascript::syntax::grammar::SourceLanguage;
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures";

#[allow(dead_code)] // helper used by fixture-driven tests
fn fixture_dir(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join(side)
}

//
// Raw-source probes (mirror tests/README.md:222-262 shape).
//

#[test]
fn assign_empty_base_captures_via_any_arg_match() {
    // The  BUN-10-A gap shape — first-arg is `{}`, second is `exports`.
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
fn existing_assign_form_unchanged() {
    // Regression:  `Object.assign(exports, {…})` MUST still be captured.
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
fn esmodule_filter_still_applies_to_relaxed_arm() {
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
fn multi_object_literal_arms_walked() {
    // already worked; ensure the relaxed gate doesn't regress it.
    let source = br#"
        module.exports = Object.assign(exports, { a: 1 }, { b: 2 }, { c: 3 });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    for expected in ["a", "b", "c"] {
        assert!(
            names.contains(&expected),
            "BUN-10: multi-object-literal walk lost {}. Got: {:?}",
            expected,
            names
        );
    }
}

#[test]
fn assign_with_no_exports_arg_short_circuits() {
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
fn assign_empty_base_with_oldexports_internal_does_not_capture() {
    // Edge case: `Object.assign({}, oldExports, { extra: 1 })` where `oldExports`
    // is an internal binding — the relaxed gate REQUIRES the literal token
    // `exports` or `module.*`, so this call short-circuits. The `extra` pair
    // is NOT captured by this arm. (It would only be captured if the source
    // also had a separate top-level `module.exports = ...` shape that the
    // existing  paths handle.)
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

//
// Real-world regression: Emotion @11.14.0 Preconstruct CJS output.
//

#[test]
fn emotion_react_regression_clean_parse() {
    // Preconstruct output must expose the same twelve known exports as the
    // original package surface.
    let path = fixture_dir(
        "node/bundlers/object-assign/emotion-react-regression",
        "old",
    )
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
            expected,
            names
        );
    }
    // Negative: `__esModule` filtered out.
    assert!(
        !names.contains(&"__esModule"),
        "Emotion regression: __esModule must be filtered. Got: {:?}",
        names
    );
}

// Compare discovered export names across the two Object.assign package versions.
// The removed property must disappear from the public surface.

#[test]
fn assign_empty_base_diff_drops_removed_symbol() {
    let old_source = std::fs::read(
        fixture_dir("node/bundlers/object-assign/assign-empty-base", "old")
            .join("dist")
            .join("index.js"),
    )
    .expect("old fixture file exists");
    let new_source = std::fs::read(
        fixture_dir("node/bundlers/object-assign/assign-empty-base", "new")
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
        assert!(
            old_names.contains(kept),
            "old missing {}: {:?}",
            kept,
            old_names
        );
        assert!(
            new_names.contains(kept),
            "new missing {}: {:?}",
            kept,
            new_names
        );
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
