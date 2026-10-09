//! JavaScript default aliases use an anonymous `default` keyword token in the
//! grammar. Verify discovery and comparison for both `.mjs` and `.js` sources.

use interlint::discover::discover_exports;
use interlint::types::ExportKind;
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures/javascript/modules/default-aliases";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn exports_of(form: &str, side: &str) -> Vec<(String, ExportKind)> {
    let dir = fixture_dir(form, side);
    let (exports, _) = discover_exports(&dir).unwrap();
    exports
        .iter()
        .map(|(key, sym)| (key.name.clone(), sym.kind.clone()))
        .collect()
}

#[test]
fn mjs_default_alias_captured() {
    let exports = exports_of("default-alias", "new");
    let has_default = exports
        .iter()
        .any(|(name, kind)| name == "default" && matches!(kind, ExportKind::Default));
    assert!(
        has_default,
        "expected a Default export named `default` from `export {{ foo as default }}` in .mjs; got exports: {:?}",
        exports
    );
    let has_foo = exports
        .iter()
        .any(|(name, kind)| name == "foo" && matches!(kind, ExportKind::Named));
    assert!(
        has_foo,
        "expected Named export `foo` alongside the Default; got: {:?}",
        exports
    );
}

#[test]
fn dual_emission() {
    let exports = exports_of("dual-emission", "new");
    let default_count = exports
        .iter()
        .filter(|(name, kind)| name == "default" && matches!(kind, ExportKind::Default))
        .count();
    let foo_named_count = exports
        .iter()
        .filter(|(name, kind)| name == "foo" && matches!(kind, ExportKind::Named))
        .count();
    assert_eq!(
        default_count, 1,
        "expected exactly one Default `default` from `export {{ foo, foo as default }}`; got: {:?}",
        exports
    );
    assert_eq!(
        foo_named_count, 1,
        "expected exactly one Named `foo` (no duplicate); got: {:?}",
        exports
    );
}

#[test]
fn non_default_alias_unchanged() {
    let exports = exports_of("non-default-alias", "new");
    let has_bar = exports
        .iter()
        .any(|(name, kind)| name == "Bar" && matches!(kind, ExportKind::Named));
    assert!(
        has_bar,
        "non-default alias `export {{ foo as Bar }}` must still produce Named `Bar` via pre-existing NAMED_EXPORT_ALIAS_QUERY; got: {:?}",
        exports
    );
    let has_spurious_default = exports.iter().any(|(name, _)| name == "default");
    assert!(
        !has_spurious_default,
        "non-default alias must NOT emit a `default` export; got: {:?}",
        exports
    );
}

#[test]
fn existing_ts_regression() {
    // Proves the new JS-only code path does NOT touch the .ts path — the
    // pre-existing INF-03  TS handler is the one that emits `default`
    // for .ts sources.
    let exports = exports_of("existing-ts-regression", "new");
    let has_default = exports
        .iter()
        .any(|(name, kind)| name == "default" && matches!(kind, ExportKind::Default));
    assert!(
        has_default,
        ".ts `export {{ foo as default }}` must still capture Default `default` via INF-03; got: {:?}",
        exports
    );
}
