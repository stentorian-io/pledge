//! Resource management compatibility checks.

use pledge::languages::javascript::syntax::exports::parse_exports;
use pledge::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn invalid_exported_using_declarations_report_missing_evidence() {
    // `using` declarations cannot be exported; recovering a name would invent
    // a usable public interface for a module the runtime cannot parse.
    let source = b"export using handle = getResource();\n";
    assert!(parse_exports(source, SourceLanguage::TypeScript).is_err());
}

#[test]
fn await_using_graceful() {
    // export await using handle = getResource() should not panic.
    let source =
        b"export async function cleanup() { await using handle = getResource(); return handle; }\n";
    let result = parse_exports(source, SourceLanguage::TypeScript);
    assert!(
        result.is_ok(),
        "parse_exports should not error on 'await using' syntax; got: {:?}",
        result.err()
    );
}
