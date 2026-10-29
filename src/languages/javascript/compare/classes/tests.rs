use crate::languages::javascript::compare::classes::*;
use crate::languages::javascript::compare::rules as runtime_rules;
use crate::languages::javascript::model::{ClassMemberKind, Visibility};
use std::path::PathBuf;

/// Build a ClassMember with sensible defaults.
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

/// Build a minimal ClassShape containing the given members.
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

fn file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn run(old: &ClassShape, new: &ClassShape) -> Vec<Finding> {
    compare_class_shapes(old, new, "MyClass", &file(), 1, 0)
}

fn rules(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}

// ── CLS-01: Constructor param added ──────────────────────────────────────

#[test]
fn test_cls01_constructor_param_added() {
    let mut old_ctor = make_member(
        "constructor",
        ClassMemberKind::Constructor,
        Visibility::Public,
    );
    old_ctor.param_count = 1;
    let mut new_ctor = make_member(
        "constructor",
        ClassMemberKind::Constructor,
        Visibility::Public,
    );
    new_ctor.param_count = 2;

    let old = make_class(vec![old_ctor]);
    let new = make_class(vec![new_ctor]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-constructor-param-added"),
        "Expected class-constructor-param-added, got: {:?}",
        rules(&findings)
    );
    assert!(
        findings[0].message.contains("was 1, now 2"),
        "Message should include old and new param counts"
    );
}

#[test]
fn test_cls01_negative_param_count_decreased() {
    // Fewer params is not breaking for callers — no CLS-01 finding.
    let mut old_ctor = make_member(
        "constructor",
        ClassMemberKind::Constructor,
        Visibility::Public,
    );
    old_ctor.param_count = 2;
    let mut new_ctor = make_member(
        "constructor",
        ClassMemberKind::Constructor,
        Visibility::Public,
    );
    new_ctor.param_count = 1;

    let old = make_class(vec![old_ctor]);
    let new = make_class(vec![new_ctor]);
    let findings = run(&old, &new);

    assert!(
        !rules(&findings).contains(&"class-constructor-param-added"),
        "Decreased param count should NOT fire class-constructor-param-added"
    );
}

// ── CLS-02: Method visibility changed ────────────────────────────────────

#[test]
fn test_cls02_public_to_protected() {
    let old_m = make_member("doWork", ClassMemberKind::Method, Visibility::Public);
    let new_m = make_member("doWork", ClassMemberKind::Method, Visibility::Protected);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-method-visibility-changed"),
        "Public->Protected should fire class-method-visibility-changed"
    );
}

#[test]
fn test_cls02_public_to_private() {
    let old_m = make_member("doWork", ClassMemberKind::Method, Visibility::Public);
    let new_m = make_member("doWork", ClassMemberKind::Method, Visibility::Private);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-method-visibility-changed"),
        "Public->Private should fire class-method-visibility-changed"
    );
}

#[test]
fn test_cls02_negative_protected_to_public() {
    // Widening visibility is safe — no finding.
    let old_m = make_member("doWork", ClassMemberKind::Method, Visibility::Protected);
    let new_m = make_member("doWork", ClassMemberKind::Method, Visibility::Public);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        !rules(&findings).contains(&"class-method-visibility-changed"),
        "Protected->Public (widening) should NOT fire class-method-visibility-changed"
    );
}

// ── CLS-03: Method removed ────────────────────────────────────────────────

#[test]
fn test_cls03_method_removed() {
    let old_m = make_member("foo", ClassMemberKind::Method, Visibility::Public);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-method-removed"),
        "Removed method should fire class-method-removed"
    );
    assert!(
        findings.iter().any(|f| f.message.contains("`foo`")),
        "Message should include the method name"
    );
}

// ── CLS-04: Class made abstract ───────────────────────────────────────────

#[test]
fn test_cls04_concrete_to_abstract() {
    let mut new_shape = make_class(vec![]);
    new_shape.is_abstract = true;

    let old = make_class(vec![]);
    let findings = run(&old, &new_shape);

    assert!(
        rules(&findings).contains(&"class-made-abstract"),
        "Concrete->Abstract should fire class-made-abstract"
    );
}

#[test]
fn test_cls04_negative_abstract_to_concrete() {
    // Abstract -> concrete is relaxing, not breaking.
    let mut old_shape = make_class(vec![]);
    old_shape.is_abstract = true;

    let new_shape = make_class(vec![]);
    let findings = run(&old_shape, &new_shape);

    assert!(
        !rules(&findings).contains(&"class-made-abstract"),
        "Abstract->Concrete should NOT fire class-made-abstract"
    );
}

// ── CLS-05: Abstract method added ────────────────────────────────────────

#[test]
fn test_cls05_abstract_method_added() {
    let mut new_m = make_member("bar", ClassMemberKind::Method, Visibility::Public);
    new_m.is_abstract = true;

    let mut new_shape = make_class(vec![new_m]);
    new_shape.is_abstract = true;

    let old = make_class(vec![]);
    let findings = run(&old, &new_shape);

    assert!(
        rules(&findings).contains(&"class-abstract-method-added"),
        "New abstract method should fire class-abstract-method-added"
    );
    assert!(
        findings.iter().any(|f| f.message.contains("`bar`")),
        "Message should include the method name"
    );
}

// ── CLS-06: Inheritance changed ──────────────────────────────────────────

#[test]
fn test_cls06_extends_changed() {
    let mut old_shape = make_class(vec![]);
    old_shape.extends_text = Some("Base".to_string());

    let mut new_shape = make_class(vec![]);
    new_shape.extends_text = Some("OtherBase".to_string());

    let findings = run(&old_shape, &new_shape);

    assert!(
        rules(&findings).contains(&"class-inheritance-changed"),
        "Changed extends should fire class-inheritance-changed"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("Base") && f.message.contains("OtherBase")),
        "Message should include old and new extends"
    );
}

#[test]
fn test_cls06_extends_removed() {
    let mut old_shape = make_class(vec![]);
    old_shape.extends_text = Some("Base".to_string());

    let new_shape = make_class(vec![]);
    // new_shape.extends_text = None (default)

    let findings = run(&old_shape, &new_shape);

    assert!(
        rules(&findings).contains(&"class-inheritance-changed"),
        "Removed extends should fire class-inheritance-changed"
    );
    assert!(
        findings.iter().any(|f| f.message.contains("(none)")),
        "Message should use (none) for missing extends"
    );
}

// ── CLS-07: Static type changed ──────────────────────────────────────────

#[test]
fn test_cls07_static_type_changed() {
    let mut old_m = make_member("count", ClassMemberKind::Property, Visibility::Public);
    old_m.is_static = true;
    old_m.type_signature = Some("number".to_string());

    let mut new_m = make_member("count", ClassMemberKind::Property, Visibility::Public);
    new_m.is_static = true;
    new_m.type_signature = Some("string".to_string());

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-static-type-changed"),
        "Changed static member type should fire class-static-type-changed"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("number") && f.message.contains("string")),
        "Message should include old and new types"
    );
}

// ── CLS-08: Readonly changed ──────────────────────────────────────────────

#[test]
fn test_cls08_readonly_added() {
    let old_m = make_member("name", ClassMemberKind::Property, Visibility::Public);
    let mut new_m = make_member("name", ClassMemberKind::Property, Visibility::Public);
    new_m.is_readonly = true;

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-readonly-changed"),
        "Adding readonly should fire class-readonly-changed"
    );
    assert!(
        findings.iter().any(|f| f.message.contains("added")),
        "Message should say 'added'"
    );
}

#[test]
fn test_cls08_readonly_removed() {
    let mut old_m = make_member("name", ClassMemberKind::Property, Visibility::Public);
    old_m.is_readonly = true;
    let new_m = make_member("name", ClassMemberKind::Property, Visibility::Public);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-readonly-changed"),
        "Removing readonly should fire class-readonly-changed"
    );
    assert!(
        findings.iter().any(|f| f.message.contains("removed")),
        "Message should say 'removed'"
    );
}

#[test]
fn implements_clause_changes_do_not_change_the_instance_contract() {
    let mut old_shape = make_class(vec![]);
    old_shape.implements_list = vec!["Serializable".to_string(), "Cloneable".to_string()];
    let mut new_shape = make_class(vec![]);
    new_shape.implements_list = vec!["Serializable".to_string()];

    assert!(run(&old_shape, &new_shape).is_empty());
}

// ── ECF-10: Method kind changed (getter/setter/method) ────────────────────

#[test]
fn test_method_to_getter_detected() {
    let old_m = make_member("foo", ClassMemberKind::Method, Visibility::Public);
    let new_m = make_member("foo", ClassMemberKind::Getter, Visibility::Public);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-method-kind-changed"),
        "method->getter should fire class-method-kind-changed, got: {:?}",
        rules(&findings)
    );
}

#[test]
fn test_getter_to_method_detected() {
    let old_m = make_member("foo", ClassMemberKind::Getter, Visibility::Public);
    let new_m = make_member("foo", ClassMemberKind::Method, Visibility::Public);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-method-kind-changed"),
        "getter->method should fire class-method-kind-changed, got: {:?}",
        rules(&findings)
    );
}

#[test]
fn test_getter_to_setter_detected() {
    let old_m = make_member("foo", ClassMemberKind::Getter, Visibility::Public);
    let new_m = make_member("foo", ClassMemberKind::Setter, Visibility::Public);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-method-kind-changed"),
        "getter->setter should fire class-method-kind-changed, got: {:?}",
        rules(&findings)
    );
}

// ── TLF-06: accessor keyword changed ─────────────────────────────────────

#[test]
fn accessor_flipped_fires_warning() {
    let old_m = make_member("data", ClassMemberKind::Property, Visibility::Public);
    let mut new_m = make_member("data", ClassMemberKind::Property, Visibility::Public);
    new_m.is_accessor = true;
    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);
    assert!(
        rules(&findings).contains(&runtime_rules::RULE_CLASS_ACCESSOR_CHANGED),
        "accessor flip should fire class-accessor-changed, got: {:?}",
        rules(&findings)
    );
    let f = findings
        .iter()
        .find(|f| f.rule == runtime_rules::RULE_CLASS_ACCESSOR_CHANGED)
        .unwrap();
    assert_eq!(
        f.severity,
        Severity::Warning,
        "class-accessor-changed should be Warning"
    );
}

#[test]
fn accessor_unchanged_no_finding() {
    let mut old_m = make_member("data", ClassMemberKind::Property, Visibility::Public);
    old_m.is_accessor = true;
    let mut new_m = make_member("data", ClassMemberKind::Property, Visibility::Public);
    new_m.is_accessor = true;
    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);
    assert!(
        !rules(&findings).contains(&runtime_rules::RULE_CLASS_ACCESSOR_CHANGED),
        "unchanged accessor should NOT fire"
    );
}

#[test]
fn test_same_getter_no_kind_changed_finding() {
    let old_m = make_member("foo", ClassMemberKind::Getter, Visibility::Public);
    let new_m = make_member("foo", ClassMemberKind::Getter, Visibility::Public);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![new_m]);
    let findings = run(&old, &new);

    assert!(
        !rules(&findings).contains(&"class-method-kind-changed"),
        "same getter kind should NOT fire class-method-kind-changed"
    );
}

#[test]
fn test_getter_removed_detected() {
    let old_m = make_member("foo", ClassMemberKind::Getter, Visibility::Public);

    let old = make_class(vec![old_m]);
    let new = make_class(vec![]);
    let findings = run(&old, &new);

    assert!(
        rules(&findings).contains(&"class-method-removed"),
        "removed getter should fire class-method-removed (CLS-03), got: {:?}",
        rules(&findings)
    );
}

// ── Identical shapes produce no findings ──────────────────────────────────

#[test]
fn test_identical_shapes_no_findings() {
    let mut ctor = make_member(
        "constructor",
        ClassMemberKind::Constructor,
        Visibility::Public,
    );
    ctor.param_count = 2;
    let method = make_member("doWork", ClassMemberKind::Method, Visibility::Public);
    let mut prop = make_member("name", ClassMemberKind::Property, Visibility::Public);
    prop.is_readonly = true;

    let mut shape = make_class(vec![ctor, method, prop]);
    shape.extends_text = Some("Base".to_string());
    shape.implements_list = vec!["IFoo".to_string(), "IBar".to_string()];

    let findings = run(&shape, &shape);

    assert!(
        findings.is_empty(),
        "Identical shapes should produce no findings, got: {:?}",
        rules(&findings)
    );
}

// ── IFT-03: Constructor parameter optional→required ───────────────────────

fn make_class_with_ctor_optionality(optionality: Vec<bool>) -> ClassShape {
    ClassShape {
        members: vec![],
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: optionality,
        decorators: Vec::new(),
        has_dynamic_static_key: false,
    }
}

#[test]
fn constructor_param_optional_to_required_fires_major() {
    // Old: [required, optional] → New: [required, required] — position 1 flipped
    let old = make_class_with_ctor_optionality(vec![false, true]);
    let new = make_class_with_ctor_optionality(vec![false, false]);
    let findings = run(&old, &new);
    let ift_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == runtime_rules::RULE_CONSTRUCTOR_PARAM_MADE_REQUIRED)
        .collect();
    assert_eq!(
        ift_findings.len(),
        1,
        "optional→required at position 1 should fire exactly once; got: {:?}",
        rules(&findings)
    );
    assert_eq!(ift_findings[0].severity, Severity::Major);
    assert!(
        ift_findings[0].message.contains("position 1"),
        "message should mention position 1: {}",
        ift_findings[0].message
    );
    assert!(
        ift_findings[0].message.contains("MyClass"),
        "message should mention class name: {}",
        ift_findings[0].message
    );
}

#[test]
fn no_change_no_finding() {
    // Both have same optionality — no IFT-03 finding
    let old = make_class_with_ctor_optionality(vec![false, true]);
    let new = make_class_with_ctor_optionality(vec![false, true]);
    let findings = run(&old, &new);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == runtime_rules::RULE_CONSTRUCTOR_PARAM_MADE_REQUIRED)
            .count(),
        0,
        "same optionality should NOT fire; got: {:?}",
        rules(&findings)
    );
}

// ── DOC-01: paired Severity::Info tests ───────────────────────────

fn make_decorated_class(decorator_name: &str, decorator_args: Option<&str>) -> ClassShape {
    let mut cls = make_class(vec![]);
    cls.decorators = vec![DecoratorSpec {
        name: decorator_name.to_string(),
        args: decorator_args.map(|s| s.to_string()),
    }];
    cls
}

#[test]
fn paired_info_at_decorator_arguments_changed_site() {
    use crate::core::findings::Severity;
    use crate::languages::typescript::compare::rules::{
        RULE_DECORATOR_ARGUMENTS_CHANGED, RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT,
    };
    let old = make_decorated_class("Route", Some("'/users'"));
    let new = make_decorated_class("Route", Some("'/people'"));
    let findings = compare_class_shapes(&old, &new, "C", &PathBuf::from("f.ts"), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Major && f.rule == RULE_DECORATOR_ARGUMENTS_CHANGED),
        "expected Major decorator-arguments-changed; got: {:?}",
        findings
    );
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Info
                && f.rule == RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT),
        "expected Info limitation: decorator-runtime-effect-opaque; got: {:?}",
        findings
    );
}

#[test]
fn paired_info_at_decorator_removed_site() {
    use crate::core::findings::Severity;
    use crate::languages::typescript::compare::rules::{
        RULE_DECORATOR_REMOVED, RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT,
    };
    let old = make_decorated_class("LogCall", None);
    let new = make_class(vec![]); // no decorator
    let findings = compare_class_shapes(&old, &new, "C", &PathBuf::from("f.ts"), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Major && f.rule == RULE_DECORATOR_REMOVED),
        "expected Major decorator-removed; got: {:?}",
        findings
    );
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Info
                && f.rule == RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT),
        "expected Info limitation: decorator-runtime-effect-opaque; got: {:?}",
        findings
    );
}

#[test]
fn paired_info_at_decorator_added_site() {
    use crate::core::findings::Severity;
    use crate::languages::typescript::compare::rules::{
        RULE_DECORATOR_ADDED, RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT,
    };
    let old = make_class(vec![]); // no decorator
    let new = make_decorated_class("LogCall", None);
    let findings = compare_class_shapes(&old, &new, "C", &PathBuf::from("f.ts"), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Major && f.rule == RULE_DECORATOR_ADDED),
        "expected Major decorator-added; got: {:?}",
        findings
    );
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Info
                && f.rule == RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT),
        "expected Info limitation: decorator-runtime-effect-opaque; got: {:?}",
        findings
    );
}

#[test]
fn paired_info_at_cls11_made_private_site() {
    use crate::core::findings::Severity;
    use crate::languages::javascript::compare::rules::{
        RULE_CLASS_FIELD_MADE_PRIVATE, RULE_LIMITATION_PRIVATE_BRAND_ACCESS,
    };
    use crate::languages::javascript::model::{ClassMemberKind, Visibility};
    // old has public 'foo', new has private '#foo'
    let mut pub_member = make_member("foo", ClassMemberKind::Property, Visibility::Public);
    pub_member.is_private = false;
    let mut priv_member = make_member("#foo", ClassMemberKind::Property, Visibility::Private);
    priv_member.is_private = true;
    let old = make_class(vec![pub_member]);
    let new = make_class(vec![priv_member]);
    let findings = compare_class_shapes(&old, &new, "C", &PathBuf::from("f.ts"), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Major && f.rule == RULE_CLASS_FIELD_MADE_PRIVATE),
        "expected Major class-field-made-private; got: {:?}",
        findings
    );
    assert!(
            findings
                .iter()
                .any(|f| f.severity == Severity::Info
                    && f.rule == RULE_LIMITATION_PRIVATE_BRAND_ACCESS),
            "expected Info limitation: private-brand-access-not-checkable; got: {:?}",
            findings
        );
}

#[test]
fn paired_info_at_cls11_made_public_site() {
    use crate::core::findings::Severity;
    use crate::languages::javascript::compare::rules::{
        RULE_CLASS_FIELD_MADE_PUBLIC, RULE_LIMITATION_PRIVATE_BRAND_ACCESS,
    };
    use crate::languages::javascript::model::{ClassMemberKind, Visibility};
    // old has private '#foo', new has public 'foo'
    let mut priv_member = make_member("#foo", ClassMemberKind::Property, Visibility::Private);
    priv_member.is_private = true;
    let mut pub_member = make_member("foo", ClassMemberKind::Property, Visibility::Public);
    pub_member.is_private = false;
    let old = make_class(vec![priv_member]);
    let new = make_class(vec![pub_member]);
    let findings = compare_class_shapes(&old, &new, "C", &PathBuf::from("f.ts"), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Minor && f.rule == RULE_CLASS_FIELD_MADE_PUBLIC),
        "expected Minor class-field-made-public; got: {:?}",
        findings
    );
    assert!(
            findings
                .iter()
                .any(|f| f.severity == Severity::Info
                    && f.rule == RULE_LIMITATION_PRIVATE_BRAND_ACCESS),
            "expected Info limitation: private-brand-access-not-checkable; got: {:?}",
            findings
        );
}

#[test]
fn paired_info_at_dynamic_static_key_site() {
    use crate::core::findings::Severity;
    use crate::languages::javascript::compare::rules::{
        RULE_CLASS_STATIC_TYPE_CHANGED, RULE_LIMITATION_DYNAMIC_STATIC_MEMBER_KEY,
    };
    let mut old = make_class(vec![]);
    old.has_dynamic_static_key = true;
    let new = make_class(vec![]);
    let findings = compare_class_shapes(&old, &new, "C", &PathBuf::from("f.ts"), 1, 1);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Minor && f.rule == RULE_CLASS_STATIC_TYPE_CHANGED),
        "expected Minor class-static-type-changed; got: {:?}",
        findings
    );
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Info
                && f.rule == RULE_LIMITATION_DYNAMIC_STATIC_MEMBER_KEY),
        "expected Info limitation: dynamic-static-member-key; got: {:?}",
        findings
    );
}
