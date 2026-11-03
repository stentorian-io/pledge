//! Internal exports compatibility checks.

use pledge::languages::javascript::syntax::exports::parse_exports;
use pledge::languages::javascript::syntax::grammar::SourceLanguage;
use pledge::languages::typescript::model::ReleaseTag;

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

#[test]
fn non_public_exports_are_excluded_from_structural_comparison() {
    use pledge::ecosystems::node::discovery::discover_exports;
    use pledge::languages::javascript::compare::{compare_exports, structure};

    for tag in ["internal", "alpha", "beta"] {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        for (directory, body) in [(&old, "method() {}"), (&new, "")] {
            std::fs::write(
                directory.path().join("package.json"),
                r#"{"name":"release-tag-test","exports":"./index.ts"}"#,
            )
            .unwrap();
            std::fs::write(
                directory.path().join("index.ts"),
                format!("/** @{tag} */\nexport class Internal {{ {body} }}"),
            )
            .unwrap();
        }
        let (before, warnings) = discover_exports(old.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let (after, warnings) = discover_exports(new.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");

        let mut findings = compare_exports(&before, &after, new.path(), false);
        findings.extend(structure::compare_structural_diff(
            old.path(),
            new.path(),
            &before,
            &after,
        ));

        assert!(findings.is_empty(), "@{tag}: {findings:?}");
    }
}
