//! Any parameters compatibility checks.

use pledge::languages::typescript::compare::types::diff_types;
use pledge::languages::typescript::types::shape::{ParamShape, TypeShape};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn any_to_concrete_fires_narrowing() {
    // Old param: any -> New param: string (narrowing — SEM-03)
    let old_shape = TypeShape::Function {
        contract: Default::default(),
        params: vec![make_param("x", false, TypeShape::Opaque("any".to_string()))],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };
    let new_shape = TypeShape::Function {
        contract: Default::default(),
        params: vec![make_param(
            "x",
            false,
            TypeShape::Opaque("string".to_string()),
        )],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };
    let findings = diff_types(&old_shape, &new_shape, "myFn", &test_file(), 1, 1);
    assert!(
        findings.iter().any(|f| f.rule == "param-type-narrowed"),
        "SEM-03: any -> string should fire param-type-narrowed. Got: {:?}",
        findings
    );
}

fn make_param(name: &str, optional: bool, shape: TypeShape) -> ParamShape {
    ParamShape {
        name: name.to_string(),
        optional,
        rest: false,
        shape,
    }
}
