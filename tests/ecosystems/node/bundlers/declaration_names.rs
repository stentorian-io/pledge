//! Declaration names compatibility checks.

use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn rollup_suffix_no_spurious_finding_on_same_base_exported() {
    // SEM-08: rollup-plugin-dts emits Foo$1 suffixes.
    // normalize_rollup_suffix strips them when the base name is exported.
    // Verified via the fixture test: fixture_rollup_dts_normalizes_foo_suffix.
    // Here we verify parse_exports on the rollup fixture's .d.ts file captures correctly.
    let source = br#"
        interface Foo { x: number; }
        interface Foo$1 { y: string; }
        declare function useFoo(f: Foo$1): void;
        export { Foo, useFoo };
    "#;
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    // Both Foo and useFoo should be exported (Foo$1 is an internal type, not exported directly)
    assert!(
        names.contains(&"Foo"),
        "SEM-08: Foo should be exported. Got: {:?}",
        names
    );
    assert!(
        names.contains(&"useFoo"),
        "SEM-08: useFoo should be exported. Got: {:?}",
        names
    );
}
