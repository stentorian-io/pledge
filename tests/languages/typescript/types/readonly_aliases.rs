//! Readonly aliases compatibility checks.

use pledge::languages::typescript::compare::types::diff_types;
use pledge::languages::typescript::types::shape::TypeShape;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn type_alias_readonly_flip_fires_major() {
    use pledge::languages::typescript::types::shape::FieldShape;
    use std::collections::BTreeMap;

    let mut fields_old = BTreeMap::new();
    fields_old.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Opaque("number".to_string()),
        },
    );
    let mut fields_new = BTreeMap::new();
    fields_new.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: true,
            shape: TypeShape::Opaque("number".to_string()),
        },
    );

    let old_shape = TypeShape::Object(fields_old);
    let new_shape = TypeShape::Object(fields_new);
    let findings = diff_types(&old_shape, &new_shape, "MyAlias", &test_file(), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "type-alias-readonly-changed"),
        "IFT-02: expected type-alias-readonly-changed. Got: {:?}",
        findings
    );
}
