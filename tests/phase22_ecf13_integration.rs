//! Phase 22 ECF-13 integration tests: well-known Symbol exports.
//!
//! Allow-list locked per CONTEXT.md: iterator, asyncIterator, toPrimitive,
//! hasInstance, toStringTag. Rule-ID reused: `export-removed` with
//! Symbol-specific message override (CONTEXT.md D-07).
//!
//! False-positive guards (CONTEXT.md):
//!   - Foo.prototype[Symbol.iterator] (class-instance iteration protocol)
//!   - class Foo { [Symbol.iterator]() {} } (class method)
//! Both must produce ZERO @@-prefixed exports.

use std::path::{Path, PathBuf};

use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::{Finding, Severity};

const FIXTURE_ROOT: &str = "tests/fixtures/phase-22/ecf-13-symbol-exports";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    let mut findings = compare_exports(&old_exports, &new_exports, &new_dir, true);
    findings.extend(compare_structural_diff(
        &old_dir,
        &new_dir,
        &old_exports,
        &new_exports,
    ));
    findings
}

fn export_names(form: &str, side: &str) -> Vec<String> {
    let dir = fixture_dir(form, side);
    let (exports, _) = discover_exports(&dir).unwrap();
    exports.iter().map(|(k, _)| k.name.clone()).collect()
}

fn assert_symbol_removed(form: &str, symbol_short: &str) {
    let findings = run(form);
    let symbol_display = format!("[Symbol.{symbol_short}]");
    let hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "export-removed" && f.message.contains(&symbol_display))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly 1 export-removed finding mentioning {symbol_display}; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        matches!(hits[0].severity, Severity::Major),
        "Symbol export removal must be Major; got {:?}",
        hits[0].severity
    );
}

#[test]
fn ecf_13_iterator_removed() {
    assert_symbol_removed("iterator", "iterator");
}

#[test]
fn ecf_13_async_iterator_removed() {
    assert_symbol_removed("async-iterator", "asyncIterator");
}

#[test]
fn ecf_13_to_primitive_removed() {
    assert_symbol_removed("to-primitive", "toPrimitive");
}

#[test]
fn ecf_13_has_instance_removed() {
    assert_symbol_removed("has-instance", "hasInstance");
}

#[test]
fn ecf_13_to_string_tag_removed() {
    assert_symbol_removed("to-string-tag", "toStringTag");
}

#[test]
fn ecf_13_prototype_not_extracted() {
    // Pitfall 2: Foo.prototype[Symbol.iterator] is NOT a module export.
    let names = export_names("prototype-negative", "old");
    let has_symbol_export = names.iter().any(|n| n.starts_with("@@"));
    assert!(
        !has_symbol_export,
        "Range.prototype[Symbol.iterator] must NOT produce an @@-prefixed export; got: {:?}",
        names
    );
}

#[test]
fn ecf_13_class_method_not_extracted() {
    // CONTEXT false-positive guard: class method [Symbol.iterator]() is NOT a module export.
    let names = export_names("class-method-negative", "old");
    let has_symbol_export = names.iter().any(|n| n.starts_with("@@"));
    assert!(
        !has_symbol_export,
        "class {{ [Symbol.iterator]() {{}} }} must NOT produce an @@-prefixed export; got: {:?}",
        names
    );
}
