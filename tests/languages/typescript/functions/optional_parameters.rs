//! Optional parameters compatibility checks.

use interlint::languages::typescript::compare::types::diff_types;
use interlint::languages::typescript::types::shape::{ParamShape, TypeShape};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

#[test]
fn optional_to_required_param_fires_rule() {
    // Old: (x?: string) => void  (optional param)
    // New: (x: string) => void   (required param)
    // Expected: param-made-required Major finding
    let old_shape = TypeShape::Function {
        contract: Default::default(),
        params: vec![make_param(
            "x",
            true,
            TypeShape::Opaque("string".to_string()),
        )],
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

    let findings = diff_types(&old_shape, &new_shape, "myFunc", &test_file(), 1, 0);

    assert!(
        findings.iter().any(|f| f.rule == "param-made-required"),
        "optional->required param should fire param-made-required; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );

    let f = findings
        .iter()
        .find(|f| f.rule == "param-made-required")
        .unwrap();
    assert_eq!(
        f.severity,
        interlint::core::findings::Severity::Major,
        "param-made-required should be Major"
    );
    assert!(
        f.message.contains("x"),
        "message should mention param name 'x'; got: {}",
        f.message
    );
}

#[test]
fn required_to_optional_does_not_fire() {
    // Removing the required constraint is safe (callers can still pass the arg)
    let old_shape = TypeShape::Function {
        contract: Default::default(),
        params: vec![make_param(
            "x",
            false,
            TypeShape::Opaque("string".to_string()),
        )],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };
    let new_shape = TypeShape::Function {
        contract: Default::default(),
        params: vec![make_param(
            "x",
            true,
            TypeShape::Opaque("string".to_string()),
        )],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };

    let findings = diff_types(&old_shape, &new_shape, "myFunc", &test_file(), 1, 0);

    assert!(
        !findings.iter().any(|f| f.rule == "param-made-required"),
        "required->optional change should NOT fire param-made-required; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn multiple_params_middle_made_required() {
    // Old: (a: string, b?: number, c?: boolean) => void
    // New: (a: string, b: number, c?: boolean) => void
    // Expected: param-made-required for 'b', no finding for unchanged params
    let old_shape = TypeShape::Function {
        contract: Default::default(),
        params: vec![
            make_param("a", false, TypeShape::Opaque("string".to_string())),
            make_param("b", true, TypeShape::Opaque("number".to_string())),
            make_param("c", true, TypeShape::Opaque("boolean".to_string())),
        ],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };
    let new_shape = TypeShape::Function {
        contract: Default::default(),
        params: vec![
            make_param("a", false, TypeShape::Opaque("string".to_string())),
            make_param("b", false, TypeShape::Opaque("number".to_string())),
            make_param("c", true, TypeShape::Opaque("boolean".to_string())),
        ],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };

    let findings = diff_types(&old_shape, &new_shape, "myFunc", &test_file(), 1, 0);

    let param_required_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "param-made-required")
        .collect();
    assert_eq!(
        param_required_findings.len(),
        1,
        "should have exactly 1 param-made-required finding for 'b'; got: {:?}",
        param_required_findings
    );
    assert!(
        param_required_findings[0].message.contains("b"),
        "finding should mention param 'b'; got: {}",
        param_required_findings[0].message
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
