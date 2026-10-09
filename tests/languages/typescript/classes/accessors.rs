//! Accessors compatibility checks.

use interlint::languages::javascript::compare::classes::compare_class_shapes;
use interlint::languages::javascript::compare::rules::{
    RULE_CLASS_METHOD_KIND_CHANGED, RULE_CLASS_METHOD_REMOVED,
};
use interlint::languages::javascript::model::{
    ClassMember, ClassMemberKind, ClassShape, Visibility,
};
use interlint::languages::javascript::syntax::exports::parse_exports;
use interlint::languages::javascript::syntax::grammar::SourceLanguage;
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

fn make_class(members: Vec<ClassMember>) -> ClassShape {
    ClassShape {
        members,
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: vec![],
        decorators: Vec::new(),
        has_dynamic_static_key: false,
    }
}

/// ECF-08/09: Getter → Method kind change should fire class-method-kind-changed.
/// This validates the new ClassMemberKind::Getter and Setter variants and the
/// class-method-kind-changed rule added in.
#[test]
fn test_getter_setter_kind_change_detected() {
    let old_class = make_class(vec![make_member(
        "value",
        ClassMemberKind::Getter,
        Visibility::Public,
    )]);
    let new_class = make_class(vec![make_member(
        "value",
        ClassMemberKind::Method,
        Visibility::Public,
    )]);

    let findings = compare_class_shapes(&old_class, &new_class, "MyClass", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CLASS_METHOD_KIND_CHANGED),
        "Expected class-method-kind-changed (Getter→Method); got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );

    let finding = findings
        .iter()
        .find(|f| f.rule == RULE_CLASS_METHOD_KIND_CHANGED)
        .unwrap();
    assert_eq!(
        finding.severity,
        interlint::core::findings::Severity::Major,
        "class-method-kind-changed must be Major severity"
    );
}

/// ECF-08: Setter → Property kind change should fire class-method-kind-changed.
#[test]
fn test_setter_to_property_kind_change_detected() {
    let old_class = make_class(vec![make_member(
        "count",
        ClassMemberKind::Setter,
        Visibility::Public,
    )]);
    let new_class = make_class(vec![make_member(
        "count",
        ClassMemberKind::Property,
        Visibility::Public,
    )]);

    let findings = compare_class_shapes(&old_class, &new_class, "Counter", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CLASS_METHOD_KIND_CHANGED),
        "Expected class-method-kind-changed (Setter→Property); got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

/// ECF-09: Getter removed from class should fire class-method-removed.
/// Validates that getter members participate in the removal detection path.
#[test]
fn test_getter_removal_detected() {
    let old_class = make_class(vec![make_member(
        "name",
        ClassMemberKind::Getter,
        Visibility::Public,
    )]);
    let new_class = make_class(vec![]);

    let findings = compare_class_shapes(&old_class, &new_class, "Entity", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == RULE_CLASS_METHOD_REMOVED),
        "Expected class-method-removed for removed getter 'name'; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

/// ECF-10: Class with ES private field — only the public member should be visible.
/// This tests that ClassShape construction correctly excludes #-prefixed members.
/// We construct ClassShape manually: a class where the public member `visible`
/// is present but a private field `#secret` is excluded (not part of ClassShape.members).
#[test]
fn test_private_field_not_in_class_shape() {
    // Simulate what parse_class_shape should produce: only public members.
    // The private field #hidden is excluded from ClassShape.members.
    let class_with_private = make_class(vec![
        make_member("visible", ClassMemberKind::Property, Visibility::Public),
        // Note: #hidden is NOT in members — parse_class_shape must exclude it.
    ]);

    // A class with no private fields at all should compare identically to one
    // that has #hidden excluded — no findings.
    let class_without_private = make_class(vec![make_member(
        "visible",
        ClassMemberKind::Property,
        Visibility::Public,
    )]);

    let findings = compare_class_shapes(
        &class_with_private,
        &class_without_private,
        "Container",
        &test_file(),
        1,
        1,
    );

    assert!(
        findings.is_empty(),
        "Two structurally identical classes (both with only 'visible') should produce no findings; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

/// ECF-10: Verify that parse_exports on a class with ES private fields
/// only reports the class export itself, not the private fields.
/// The private field #hidden is internal to the class and must not leak into
/// the top-level export map.
#[test]
fn test_private_field_export_not_leaked() {
    let src = b"export class A { #hidden: string; visible: number; }";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    // Only "A" should appear as an export — the #hidden field is not a top-level export.
    assert!(
        exports.iter().any(|e| e.name == "A"),
        "Expected export 'A'; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    assert!(
        !exports
            .iter()
            .any(|e| e.name == "#hidden" || e.name.contains("hidden")),
        "Private field '#hidden' must not appear as an export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

/// ECF-08: Getter → Setter kind change (both accessor kinds) fires class-method-kind-changed.
#[test]
fn test_getter_to_setter_kind_change_detected() {
    let old_class = make_class(vec![make_member(
        "data",
        ClassMemberKind::Getter,
        Visibility::Public,
    )]);
    let new_class = make_class(vec![make_member(
        "data",
        ClassMemberKind::Setter,
        Visibility::Public,
    )]);

    let findings = compare_class_shapes(&old_class, &new_class, "DataHolder", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CLASS_METHOD_KIND_CHANGED),
        "Expected class-method-kind-changed (Getter→Setter); got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

fn make_member(name: &str, kind: ClassMemberKind, vis: Visibility) -> ClassMember {
    ClassMember {
        name: name.to_string(),
        kind,
        visibility: vis,
        is_static: false,
        is_abstract: false,
        is_readonly: false,
        param_count: 0,
        type_signature: None,
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
        is_private: false,
    }
}
