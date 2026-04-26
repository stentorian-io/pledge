//! Phase 24 BUN-11 integration tests:
//! - BUN-11-A: UMD wrapper factory returns object literal
//! - BUN-11 regression: UMD wrapper with `factory(exports)` arg (already detected pre-Phase-24)
//! - BUN-11-B: lodash-style IIFE with `module.exports = <ident>` (Path 1A best-effort)

use std::path::{Path, PathBuf};

use interlint::discover::tree_query::{parse_exports, RawExport, RawExportKind, SourceLanguage};

const FIXTURE_ROOT: &str = "tests/fixtures/phase-24";

#[allow(dead_code)] // helper used by fixture-driven tests
fn fixture_dir(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(case)
        .join(side)
}

fn names_in(raw: &[RawExport]) -> Vec<&str> {
    raw.iter().map(|e| e.name.as_str()).collect()
}

// ===========================================================================
// BUN-11-A — UMD factory returns object literal (Form E)
// ===========================================================================

#[test]
fn bun_11_umd_factory_returns_object_literal_captured() {
    // Form E from 24-RESEARCH.md §Empirical Coverage Audit — the BUN-11-A target.
    let source = br#"
        (function (root, factory) {
          if (typeof define === 'function' && define.amd) define([], factory);
          else if (typeof module === 'object' && module.exports) module.exports = factory();
          else root.MyLib = factory();
        }(typeof self !== 'undefined' ? self : this, function () {
          return { foo: function() {}, bar: function() {}, VERSION: '1.0' };
        }));
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names = names_in(&raw);
    for expected in ["foo", "bar", "VERSION"] {
        assert!(
            names.contains(&expected),
            "BUN-11-A: {} not captured from UMD factory return-object. Got: {:?}",
            expected,
            names
        );
    }
}

#[test]
fn bun_11_umd_factory_esmodule_filtered() {
    let source = br#"
        (function (root, factory) {
          module.exports = factory();
        }(this, function () {
          return { __esModule: true, real: 1 };
        }));
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names = names_in(&raw);
    assert!(
        !names.contains(&"__esModule"),
        "BUN-11-A: __esModule must be filtered. Got: {:?}",
        names
    );
    assert!(
        names.contains(&"real"),
        "BUN-11-A: legitimate exports retained. Got: {:?}",
        names
    );
}

#[test]
fn bun_11_umd_factory_returns_non_object_no_panic() {
    // The factory returns an identifier (Form D-adjacent). The BUN-11-A
    // return-object handler skips this case (kind != "object"). The BUN-11-B
    // Path 1A handler MAY pick up the `module.exports = <ident>` shape; this
    // test only asserts no panic.
    let source = br#"
        (function (root, factory) {
          module.exports = factory();
        }(this, function () {
          var x = function() {};
          return x;
        }));
    "#;
    // Just verify no panic.
    let _ = parse_exports(source, SourceLanguage::JavaScript).unwrap();
}

// ===========================================================================
// BUN-11 regression — UMD with exports arg (already-detected, must stay green)
// ===========================================================================

#[test]
fn bun_11_umd_with_exports_arg_regression() {
    // Form C from 24-RESEARCH.md — already detected via depth-agnostic
    // CJS_EXPORTS_PROPERTY_QUERY. Phase 24 must NOT regress this.
    let source = br#"
        (function (root, factory) {
          if (typeof define === 'function' && define.amd) define(['exports'], factory);
          else if (typeof exports !== 'undefined') factory(exports);
          else factory((root.MyLib = {}));
        })(this, function (exports) {
          exports.foo = function() {};
          exports.bar = 42;
          Object.defineProperty(exports, "qux", { value: 1 });
        });
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names = names_in(&raw);
    for expected in ["foo", "bar", "qux"] {
        assert!(
            names.contains(&expected),
            "BUN-11 regression: {} should still be captured. Got: {:?}",
            expected,
            names
        );
    }
    // Negative: each name appears exactly once (no double-emission from the
    // BUN-11-A driver overlapping with CJS_EXPORTS_PROPERTY_QUERY).
    let foo_count = raw.iter().filter(|e| e.name == "foo").count();
    let bar_count = raw.iter().filter(|e| e.name == "bar").count();
    let qux_count = raw.iter().filter(|e| e.name == "qux").count();
    assert_eq!(
        foo_count,
        1,
        "foo appears {}× — dedup broken; raw: {:?}",
        foo_count,
        raw.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(bar_count, 1, "bar appears {}× — dedup broken", bar_count);
    assert_eq!(qux_count, 1, "qux appears {}× — dedup broken", qux_count);
}

// ===========================================================================
// BUN-11-B — lodash-style IIFE with module.exports = <ident>
// ===========================================================================

#[test]
fn bun_11_lodash_style_default_with_compound_members_path1() {
    // Form D from 24-RESEARCH.md — the BUN-11-B canonical shape.
    let source = br#"
        (function() {
          var _ = function() {};
          _.chunk = function() {};
          _.compact = function() {};
          _.VERSION = "4.17.21";
          if (typeof module === 'object' && module.exports) {
            (module.exports = _)._ = _;
          }
        }.call(this));
    "#;
    let raw = parse_exports(source, SourceLanguage::JavaScript).unwrap();

    // Path 1A assertion (sentinel `BUN-11-B SENTINEL PATH 1A IMPLEMENTED` present
    // in src/discover/tree_query.rs).
    let default_export = raw
        .iter()
        .find(|e| e.name == "default" && matches!(e.kind, RawExportKind::Default));
    assert!(
        default_export.is_some(),
        "BUN-11-B Path 1A: expected a Default export named 'default'. Got: {:?}",
        raw.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
    );
    let members = &default_export.unwrap().compound_members;
    for expected in ["chunk", "compact", "VERSION"] {
        assert!(
            members.iter().any(|m| m == expected),
            "BUN-11-B Path 1A: {} not in compound_members. Got: {:?}",
            expected,
            members
        );
    }
}

// ===========================================================================
// Fixture-driven diff regressions
// ===========================================================================

#[test]
fn bun_11_umd_factory_returns_object_diff_drops_removed_symbol() {
    let old_source = std::fs::read(
        fixture_dir("bun-11-umd-factory-returns-object", "old")
            .join("dist")
            .join("index.js"),
    )
    .expect("old fixture file exists");
    let new_source = std::fs::read(
        fixture_dir("bun-11-umd-factory-returns-object", "new")
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

    for kept in ["foo", "bar", "VERSION"] {
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
    assert!(
        old_names.contains("removed_symbol"),
        "old should contain removed_symbol: {:?}",
        old_names
    );
    assert!(
        !new_names.contains("removed_symbol"),
        "new should NOT contain removed_symbol: {:?}",
        new_names
    );
}

#[test]
fn bun_11_umd_with_exports_arg_regression_diff() {
    // Regression: UMD-with-exports-arg shape — depth-agnostic detection
    // still produces a clean removed-symbol delta after Phase 24.
    let old_source = std::fs::read(
        fixture_dir("bun-11-umd-with-exports-arg-regression", "old")
            .join("dist")
            .join("index.js"),
    )
    .expect("old fixture exists");
    let new_source = std::fs::read(
        fixture_dir("bun-11-umd-with-exports-arg-regression", "new")
            .join("dist")
            .join("index.js"),
    )
    .expect("new fixture exists");

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

    for kept in ["foo", "bar", "qux"] {
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
    assert!(
        old_names.contains("removed_symbol"),
        "old should contain removed_symbol: {:?}",
        old_names
    );
    assert!(
        !new_names.contains("removed_symbol"),
        "new should NOT contain removed_symbol: {:?}",
        new_names
    );
}

// ===========================================================================
// BUN-11-B fixture-driven diff — Path 1A
// ===========================================================================

#[test]
fn bun_11_lodash_style_diff_drops_removed_member() {
    let old_source = std::fs::read(
        fixture_dir("bun-11-lodash-style-default-with-members", "old")
            .join("dist")
            .join("index.js"),
    )
    .expect("old fixture exists");
    let new_source = std::fs::read(
        fixture_dir("bun-11-lodash-style-default-with-members", "new")
            .join("dist")
            .join("index.js"),
    )
    .expect("new fixture exists");

    let old_raw = parse_exports(&old_source, SourceLanguage::JavaScript).unwrap();
    let new_raw = parse_exports(&new_source, SourceLanguage::JavaScript).unwrap();

    // Path 1A: the diff target lives inside the Default's compound_members.
    let old_default = old_raw
        .iter()
        .find(|e| e.name == "default" && matches!(e.kind, RawExportKind::Default));
    let new_default = new_raw
        .iter()
        .find(|e| e.name == "default" && matches!(e.kind, RawExportKind::Default));

    assert!(
        old_default.is_some(),
        "old fixture must produce a Default export. Got: {:?}",
        old_raw
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        new_default.is_some(),
        "new fixture must produce a Default export. Got: {:?}",
        new_raw
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );

    let old_members = &old_default.unwrap().compound_members;
    let new_members = &new_default.unwrap().compound_members;

    assert!(
        old_members.iter().any(|m| m == "removed_member"),
        "old compound_members should contain `removed_member`: {:?}",
        old_members
    );
    assert!(
        !new_members.iter().any(|m| m == "removed_member"),
        "new compound_members should NOT contain `removed_member`: {:?}",
        new_members
    );
    // Both versions should share the kept members.
    for kept in ["chunk", "compact", "VERSION"] {
        assert!(
            old_members.iter().any(|m| m == kept) && new_members.iter().any(|m| m == kept),
            "kept member {} missing on one side. old: {:?}, new: {:?}",
            kept,
            old_members,
            new_members
        );
    }
}
