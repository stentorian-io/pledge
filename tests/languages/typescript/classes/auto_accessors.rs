//! Auto accessors compatibility checks.

use interlint::compare::class_diff::compare_class_shapes;
use interlint::types::{ClassMember, ClassMemberKind, ClassShape, Visibility};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn make_property(name: &str, is_optional: bool, is_readonly: bool) -> ClassMember {
    ClassMember {
        name: name.to_string(),
        kind: ClassMemberKind::Property,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly,
        param_count: 0,
        type_signature: None,
        is_optional,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
        is_private: false,
    }
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
fn accessor_flip_fires_warning() {
    let mut old_member = make_property("bar", false, false);
    old_member.is_accessor = true;
    let new_member = make_property("bar", false, false); // is_accessor = false

    let old_shape = make_class(vec![old_member], vec![]);
    let new_shape = make_class(vec![new_member], vec![]);
    let findings = compare_class_shapes(&old_shape, &new_shape, "MyClass", &test_file(), 1, 1);
    assert!(
        findings.iter().any(|f| f.rule == "class-accessor-changed"),
        "TLF-06: expected class-accessor-changed. Got: {:?}",
        findings
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "class-accessor-changed")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::types::Severity::Warning,
        "TLF-06: class-accessor-changed should be Warning"
    );
}
