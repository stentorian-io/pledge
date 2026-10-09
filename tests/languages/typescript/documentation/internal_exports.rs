//! Internal exports compatibility checks.

use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;
use interlint::languages::typescript::model::ReleaseTag;

#[test]
fn internal_tag_excludes_from_comparison() {
    let source = br#"
        /** @internal */
        export function hidden() {}
        export function visible() {}
    "#;
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    let hidden = exports.iter().find(|e| e.name == "hidden");
    let visible = exports.iter().find(|e| e.name == "visible");

    assert!(
        hidden.is_some(),
        "META-04: hidden export missing. Got: {:?}",
        exports
    );
    assert!(
        hidden.unwrap().release_tag == Some(ReleaseTag::Internal),
        "META-04: hidden release_tag should be Internal. Got: {:?}",
        hidden.unwrap().release_tag
    );
    assert!(
        visible.is_some(),
        "META-04: visible export missing. Got: {:?}",
        exports
    );
    assert!(
        visible.unwrap().release_tag.is_none(),
        "META-04: visible should have no release_tag. Got: {:?}",
        visible.unwrap().release_tag
    );
}
