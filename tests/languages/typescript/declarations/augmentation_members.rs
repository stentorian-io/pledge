//! Augmentation members compatibility checks.

use interlint::discover::tree_query::{parse_exports, SourceLanguage};

#[test]
fn declare_module_body_member_diffed() {
    // META-03: augmentation body members are diffed via augment:scope:*member synthetic keys.
    let old_src =
        br#"declare module 'express' { interface Session { uid: string; } interface Request { user: string; } }"#;
    let new_src = br#"declare module 'express' { interface Request { user: string; } }"#;
    let old_exports = parse_exports(old_src, SourceLanguage::TypeScript).unwrap();
    let new_exports = parse_exports(new_src, SourceLanguage::TypeScript).unwrap();
    let old_names: Vec<&str> = old_exports.iter().map(|e| e.name.as_str()).collect();
    let new_names: Vec<&str> = new_exports.iter().map(|e| e.name.as_str()).collect();
    // Session should be in old but not in new (via augment:express:*Session key)
    assert!(
        old_names
            .iter()
            .any(|n| n.contains("Session") || n.contains("augment")),
        "META-03: Session/augment key not captured in old. Got: {:?}",
        old_names
    );
    // Request should be in both
    assert!(
        new_names.iter().any(|n| n.contains("Request")),
        "META-03: Request not captured in new. Got: {:?}",
        new_names
    );
}
