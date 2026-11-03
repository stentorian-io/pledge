//! Variance compatibility checks.

use pledge::languages::typescript::compare::generics::compare_type_params;
use pledge::languages::typescript::model::{TypeParam, Variance};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn variance_modifier_change_fires_major() {
    let old = vec![make_type_param("T", Some(Variance::In))];
    let new = vec![make_type_param("T", None)];
    let findings = compare_type_params(&old, &new, "X", &test_file(), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "generic-variance-changed"),
        "TLF-05: expected generic-variance-changed. Got: {:?}",
        findings
    );
}

fn make_type_param(name: &str, variance: Option<Variance>) -> TypeParam {
    TypeParam {
        name: name.to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: false,
        variance,
    }
}
