//! Constructor optionality compatibility checks.

use interlint::compare::class_diff::compare_class_shapes;
use interlint::types::{ClassMember, ClassShape};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn make_class(members: Vec<ClassMember>, ctor_optionality: Vec<bool>) -> ClassShape {
    ClassShape {
        members,
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: ctor_optionality,
        decorators: Vec::new(),
        has_dynamic_static_key: false,
    }
}

#[test]
fn constructor_param_optional_to_required_fires_major() {
    // old=[false, true] -> position 1 is optional
    // new=[false, false] -> position 1 is now required
    let old_shape = make_class(vec![], vec![false, true]);
    let new_shape = make_class(vec![], vec![false, false]);
    let findings = compare_class_shapes(&old_shape, &new_shape, "MyClass", &test_file(), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "constructor-param-made-required"),
        "IFT-03: expected constructor-param-made-required. Got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "constructor-param-made-required")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::types::Severity::Major,
        "IFT-03: constructor-param-made-required should be Major"
    );
}
