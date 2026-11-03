//! Index signatures compatibility checks.

use pledge::languages::typescript::compare::interfaces::compare_interface_index_signatures;
use pledge::languages::typescript::syntax::interfaces::IndexSignature;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn index_signature_removed_fires_rule() {
    // Old: [key: string]: number
    // New: (no index signature)
    let old = vec![make_index_sig("string", "number")];
    let new: Vec<IndexSignature> = vec![];

    let findings =
        compare_interface_index_signatures(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert_eq!(
        findings.len(),
        1,
        "removing index sig should produce 1 finding"
    );
    assert_eq!(
        findings[0].rule, "interface-index-signature-changed",
        "rule should be interface-index-signature-changed; got: {}",
        findings[0].rule
    );
    assert_eq!(
        findings[0].severity,
        pledge::core::findings::Severity::Major
    );
    assert!(
        findings[0].message.contains("removed"),
        "message should say 'removed'; got: {}",
        findings[0].message
    );
}

#[test]
fn index_signature_value_type_changed_fires_rule() {
    // Old: [key: string]: number
    // New: [key: string]: string
    let old = vec![make_index_sig("string", "number")];
    let new = vec![make_index_sig("string", "string")];

    let findings =
        compare_interface_index_signatures(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert_eq!(
        findings.len(),
        1,
        "changing index sig value type should produce 1 finding"
    );
    assert_eq!(findings[0].rule, "interface-index-signature-changed");
    assert!(
        findings[0].message.contains("number") && findings[0].message.contains("string"),
        "message should show old and new value types; got: {}",
        findings[0].message
    );
}

#[test]
fn index_signature_unchanged_no_finding() {
    let sigs = vec![make_index_sig("string", "number")];
    let findings =
        compare_interface_index_signatures(&sigs, &sigs, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings.is_empty(),
        "unchanged index signature should produce no finding; got: {:?}",
        findings
    );
}

fn make_index_sig(key: &str, val: &str) -> IndexSignature {
    IndexSignature {
        key_type_text: key.to_string(),
        value_type_text: val.to_string(),
    }
}
