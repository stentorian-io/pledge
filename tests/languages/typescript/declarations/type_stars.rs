//! Type stars compatibility checks.

use pledge::languages::javascript::syntax::exports::parse_exports;
use pledge::languages::javascript::syntax::grammar::SourceLanguage;

#[test]
fn export_type_star_is_type_only_flag_set() {
    let source = br#"export type * from './mod';"#;
    let raw = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    assert!(
        raw.iter().any(|e| e.is_type_only),
        "TLF-07: expected is_type_only=true for export type *. Got: {:?}",
        raw
    );
}
