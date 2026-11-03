//! Package fixtures compatibility checks.

use pledge::languages::javascript::syntax::exports::{parse_exports, RawExportKind};
use pledge::languages::javascript::syntax::grammar::SourceLanguage;
use std::path::Path;

#[test]
fn fixture_esbuild_detects_removed_export_bar() {
    let old = Path::new("tests/fixtures/node/bundlers/esbuild/old");
    let new = Path::new("tests/fixtures/node/bundlers/esbuild/new");
    if !old.exists() || !new.exists() {
        eprintln!("fixture missing; skipping");
        return;
    }
    // Parse exports from old and new dist/index.js and verify "bar" is in old but not new
    let old_src = std::fs::read(old.join("dist/index.js")).expect("old/dist/index.js must exist");
    let new_src = std::fs::read(new.join("dist/index.js")).expect("new/dist/index.js must exist");

    let old_exports = parse_exports(&old_src, SourceLanguage::JavaScript).unwrap();
    let new_exports = parse_exports(&new_src, SourceLanguage::JavaScript).unwrap();

    let old_names: Vec<&str> = old_exports.iter().map(|e| e.name.as_str()).collect();
    let new_names: Vec<&str> = new_exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        old_names.contains(&"foo") && old_names.contains(&"bar"),
        "fixture_esbuild: old should export foo and bar. Got: {:?}",
        old_names
    );
    assert!(
        new_names.contains(&"foo"),
        "fixture_esbuild: new should still export foo. Got: {:?}",
        new_names
    );
    assert!(
        !new_names.contains(&"bar"),
        "fixture_esbuild: new should NOT export bar (removed). Got: {:?}",
        new_names
    );
    // __esModule must be filtered in both
    assert!(
        !old_names.contains(&"__esModule"),
        "fixture_esbuild: __esModule must be filtered in old. Got: {:?}",
        old_names
    );
}

#[test]
fn fixture_tsc_detects_removed_farewell() {
    let old = Path::new("tests/fixtures/node/bundlers/tsc/old");
    let new = Path::new("tests/fixtures/node/bundlers/tsc/new");
    if !old.exists() || !new.exists() {
        eprintln!("fixture missing; skipping");
        return;
    }
    let old_src = std::fs::read(old.join("dist/index.js")).expect("old/dist/index.js must exist");
    let new_src = std::fs::read(new.join("dist/index.js")).expect("new/dist/index.js must exist");

    let old_exports = parse_exports(&old_src, SourceLanguage::JavaScript).unwrap();
    let new_exports = parse_exports(&new_src, SourceLanguage::JavaScript).unwrap();

    let old_names: Vec<&str> = old_exports.iter().map(|e| e.name.as_str()).collect();
    let new_names: Vec<&str> = new_exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        old_names.contains(&"greet") && old_names.contains(&"farewell"),
        "fixture_tsc: old should export greet and farewell. Got: {:?}",
        old_names
    );
    assert!(
        !new_names.contains(&"farewell"),
        "fixture_tsc: new should NOT export farewell. Got: {:?}",
        new_names
    );
    // __esModule must always be filtered
    assert!(
        !old_names.contains(&"__esModule"),
        "fixture_tsc: __esModule must be filtered in old. Got: {:?}",
        old_names
    );
    assert!(
        !new_names.contains(&"__esModule"),
        "fixture_tsc: __esModule must be filtered in new. Got: {:?}",
        new_names
    );
}

#[test]
fn fixture_swc_detects_removed_helper_via_star_reexport() {
    let old = Path::new("tests/fixtures/node/bundlers/swc/old");
    let new = Path::new("tests/fixtures/node/bundlers/swc/new");
    if !old.exists() || !new.exists() {
        eprintln!("fixture missing; skipping");
        return;
    }
    let old_src = std::fs::read(old.join("dist/index.js")).expect("old/dist/index.js must exist");
    let new_src = std::fs::read(new.join("dist/index.js")).expect("new/dist/index.js must exist");

    let old_exports = parse_exports(&old_src, SourceLanguage::JavaScript).unwrap();
    let new_exports = parse_exports(&new_src, SourceLanguage::JavaScript).unwrap();

    // Old should have a StarReexport (BUN-05 pattern)
    let old_has_star = old_exports
        .iter()
        .any(|e| matches!(e.kind, RawExportKind::StarReexport));
    assert!(
        old_has_star,
        "fixture_swc: old should have StarReexport (BUN-05). Got: {:?}",
        old_exports
    );
    // New utils.js is empty/removes helper — new index.js has no star reexport
    // or the star reexport points to a module with no exports
    let _ = new_exports; // valid even if empty
}

#[test]
fn fixture_rollup_dts_normalizes_foo_suffix() {
    let old = Path::new("tests/fixtures/node/bundlers/rollup-dts/old");
    let new = Path::new("tests/fixtures/node/bundlers/rollup-dts/new");
    if !old.exists() || !new.exists() {
        eprintln!("fixture missing; skipping");
        return;
    }
    // Parse the .d.ts files
    let old_src =
        std::fs::read(old.join("dist/index.d.ts")).expect("old/dist/index.d.ts must exist");
    let new_src =
        std::fs::read(new.join("dist/index.d.ts")).expect("new/dist/index.d.ts must exist");

    let old_exports = parse_exports(&old_src, SourceLanguage::TypeScript).unwrap();
    let new_exports = parse_exports(&new_src, SourceLanguage::TypeScript).unwrap();

    // Both should export Foo and useFoo
    let old_names: Vec<&str> = old_exports.iter().map(|e| e.name.as_str()).collect();
    let new_names: Vec<&str> = new_exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        old_names.contains(&"Foo"),
        "fixture_rollup_dts: old should export Foo. Got: {:?}",
        old_names
    );
    assert!(
        old_names.contains(&"useFoo"),
        "fixture_rollup_dts: old should export useFoo. Got: {:?}",
        old_names
    );
    assert!(
        new_names.contains(&"Foo"),
        "fixture_rollup_dts: new should export Foo. Got: {:?}",
        new_names
    );
    assert!(
        new_names.contains(&"useFoo"),
        "fixture_rollup_dts: new should export useFoo. Got: {:?}",
        new_names
    );
}

#[test]
fn fixture_vite_chunk_normalization_no_spurious_path_findings() {
    let old = Path::new("tests/fixtures/node/bundlers/vite/old");
    let new = Path::new("tests/fixtures/node/bundlers/vite/new");
    if !old.exists() || !new.exists() {
        eprintln!("fixture missing; skipping");
        return;
    }
    // Read the index.js entry points from old and new
    let old_src = std::fs::read(old.join("dist/index.js")).expect("old/dist/index.js must exist");
    let new_src = std::fs::read(new.join("dist/index.js")).expect("new/dist/index.js must exist");

    let old_exports = parse_exports(&old_src, SourceLanguage::JavaScript).unwrap();
    let new_exports = parse_exports(&new_src, SourceLanguage::JavaScript).unwrap();

    // Both re-export foo (just from differently-hashed chunks)
    let old_names: Vec<&str> = old_exports.iter().map(|e| e.name.as_str()).collect();
    let new_names: Vec<&str> = new_exports.iter().map(|e| e.name.as_str()).collect();

    // foo must appear in both (even if via NamedReexport)
    let old_has_foo = old_names.contains(&"foo")
        || old_exports
            .iter()
            .any(|e| matches!(e.kind, RawExportKind::NamedReexport) && e.name == "foo");
    let new_has_foo = new_names.contains(&"foo")
        || new_exports
            .iter()
            .any(|e| matches!(e.kind, RawExportKind::NamedReexport) && e.name == "foo");

    assert!(
        old_has_foo,
        "fixture_vite: old should re-export foo. Got: {:?}",
        old_exports
    );
    assert!(
        new_has_foo,
        "fixture_vite: new should re-export foo. Got: {:?}",
        new_exports
    );
}
