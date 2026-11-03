//! Compound types compatibility checks.

use pledge::languages::typescript::types::parser::parse_type_string;
use pledge::languages::typescript::types::shape::TypeShape;

#[test]
fn conditional_type_parsing() {
    let shape = parse_type_string("T extends string ? number : boolean");
    assert!(
        matches!(shape, TypeShape::Conditional { .. }),
        "Should parse as Conditional, got {:?}",
        shape
    );
}

#[test]
fn mapped_type_parsing() {
    let shape = parse_type_string("{ [K in keyof T]: string }");
    assert!(
        matches!(shape, TypeShape::Mapped { .. }),
        "Should parse as Mapped, got {:?}",
        shape
    );
}

#[test]
fn indexed_access_type_parsing() {
    let shape = parse_type_string("T[\"key\"]");
    assert!(
        matches!(shape, TypeShape::Indexed { .. }),
        "Should parse as Indexed, got {:?}",
        shape
    );
}

#[test]
fn keyof_type_parsing() {
    let shape = parse_type_string("keyof T");
    match &shape {
        TypeShape::Generic { name, args } => {
            assert_eq!(
                name, "keyof",
                "keyof should be parsed as Generic with name 'keyof'"
            );
            assert_eq!(args.len(), 1, "keyof T should have 1 arg");
        }
        _ => panic!("Should parse keyof as Generic, got {:?}", shape),
    }
}

#[test]
fn conditional_nested_parsing() {
    // More complex conditional type
    let shape = parse_type_string("T extends Array<infer U> ? U : never");
    assert!(
        matches!(shape, TypeShape::Conditional { .. }),
        "Nested conditional should parse as Conditional, got {:?}",
        shape
    );
}
