//! Resource management compatibility checks.

use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn using_graceful() {
    // export using handle = getResource() should not panic.
    // Grammar may not natively support this syntax — graceful degradation required.
    let source = b"export using handle = getResource();\n";
    // The key guarantee: no panic. Result can be empty or contain the export.
    let result = parse_exports(source, SourceLanguage::TypeScript);
    assert!(
        result.is_ok(),
        "parse_exports should not error on 'export using' syntax; got: {:?}",
        result.err()
    );
    // No assertion on content — graceful degradation means empty is acceptable
}

#[test]
fn await_using_graceful() {
    // export await using handle = getResource() should not panic.
    let source =
        b"export function cleanup() { await using handle = getResource(); return handle; }\n";
    let result = parse_exports(source, SourceLanguage::TypeScript);
    assert!(
        result.is_ok(),
        "parse_exports should not error on 'await using' syntax; got: {:?}",
        result.err()
    );
}
