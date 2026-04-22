//! Integration tests for Phase 7 class, enum, and generic type parameter rules.
//!
//! These tests exercise the FULL pipeline:
//!   tree-sitter extraction → diff engine → Finding output
//!
//! Each test covers one of the 20 new rule IDs:
//!   CLS-01..09: class breaking changes
//!   ENUM-01..05: enum breaking changes
//!   GEN-01..06: generic type parameter breaking changes

use interlint::compare::{class_diff, enum_diff, generics_diff};
use interlint::discover::tree_query;
use interlint::types::{ClassMember, ClassMemberKind, ClassShape, Finding, Visibility};
use std::path::PathBuf;

// ─── Test helpers ─────────────────────────────────────────────────────────────

/// Return the file path used for finding location in tests.
fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

/// Parse TypeScript source and return the tree + bytes.
fn parse_ts(source: &[u8]) -> (tree_sitter::Tree, Vec<u8>) {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&lang).unwrap();
    let bytes = source.to_vec();
    let tree = parser
        .parse(&bytes, None)
        .expect("tree-sitter parse failed");
    (tree, bytes)
}

/// Find declaration node by name in an export_statement wrapper.
fn find_decl_by_name<'a>(
    root: tree_sitter::Node<'a>,
    source: &[u8],
    name: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind();

        // Check this node directly
        let decl_kinds = [
            "class_declaration",
            "abstract_class_declaration",
            "enum_declaration",
            "function_declaration",
            "interface_declaration",
            "type_alias_declaration",
        ];

        if decl_kinds.contains(&kind) {
            let decl_name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok());
            if decl_name == Some(name) {
                return Some(child);
            }
        }

        // Recurse into export_statement wrappers
        if kind == "export_statement" {
            if let Some(found) = find_decl_by_name(child, source, name) {
                return Some(found);
            }
        }
    }
    None
}

/// Parse TS source and extract ClassShape for the named class.
fn extract_class(source: &[u8], name: &str) -> ClassShape {
    let (tree, bytes) = parse_ts(source);
    let decl = find_decl_by_name(tree.root_node(), &bytes, name)
        .unwrap_or_else(|| panic!("class `{}` not found in source", name));
    tree_query::parse_class_shape(&bytes, decl)
}

/// Parse TS source and extract EnumShape for the named enum.
fn extract_enum(source: &[u8], name: &str) -> interlint::types::EnumShape {
    let (tree, bytes) = parse_ts(source);
    let decl = find_decl_by_name(tree.root_node(), &bytes, name)
        .unwrap_or_else(|| panic!("enum `{}` not found in source", name));
    tree_query::parse_enum_shape(&bytes, decl)
}

/// Parse TS source and extract TypeParams for the named declaration.
fn extract_type_params_from(source: &[u8], name: &str) -> Vec<interlint::types::TypeParam> {
    let (tree, bytes) = parse_ts(source);
    let decl = find_decl_by_name(tree.root_node(), &bytes, name)
        .unwrap_or_else(|| panic!("declaration `{}` not found in source", name));
    tree_query::parse_type_params(&bytes, decl)
}

/// Extract rule IDs from a findings slice.
fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}

// ─── CLS-01: Constructor parameter added ─────────────────────────────────────

#[test]
fn test_cls_01_constructor_param_added() {
    let old = b"export class Foo { constructor(a: string) {} }";
    let new = b"export class Foo { constructor(a: string, b: number) {} }";

    let old_shape = extract_class(old, "Foo");
    let new_shape = extract_class(new, "Foo");

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-constructor-param-added"),
        "Expected class-constructor-param-added, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── CLS-02: Method visibility narrowed ──────────────────────────────────────

#[test]
fn test_cls_02_method_visibility_changed() {
    let old = b"export class Foo { public greet(): void {} }";
    let new = b"export class Foo { protected greet(): void {} }";

    let old_shape = extract_class(old, "Foo");
    let new_shape = extract_class(new, "Foo");

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-method-visibility-changed"),
        "Expected class-method-visibility-changed, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── CLS-03: Method removed ───────────────────────────────────────────────────

#[test]
fn test_cls_03_method_removed() {
    let old = b"export class Foo { greet(): void {} hello(): void {} }";
    let new = b"export class Foo { greet(): void {} }";

    let old_shape = extract_class(old, "Foo");
    let new_shape = extract_class(new, "Foo");

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "class-method-removed"),
        "Expected class-method-removed, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-method-removed" && f.message.contains("hello")),
        "Finding should mention removed method `hello`"
    );
}

// ─── CLS-04: Concrete class made abstract ────────────────────────────────────

#[test]
fn test_cls_04_class_made_abstract() {
    let old = b"export class Foo {}";
    let new = b"export abstract class Foo {}";

    let old_shape = extract_class(old, "Foo");
    let new_shape = extract_class(new, "Foo");

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "class-made-abstract"),
        "Expected class-made-abstract, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── CLS-05: Abstract method added ───────────────────────────────────────────

#[test]
fn test_cls_05_abstract_method_added() {
    let old = b"export abstract class Foo {}";
    let new = b"export abstract class Foo { abstract bar(): void; }";

    let old_shape = extract_class(old, "Foo");
    let new_shape = extract_class(new, "Foo");

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-abstract-method-added"),
        "Expected class-abstract-method-added, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-abstract-method-added" && f.message.contains("bar")),
        "Finding should mention abstract method `bar`"
    );
}

// ─── CLS-06: Inheritance changed ─────────────────────────────────────────────

#[test]
fn test_cls_06_inheritance_changed() {
    let old = b"export class Foo extends Base {}";
    let new = b"export class Foo extends OtherBase {}";

    let old_shape = extract_class(old, "Foo");
    let new_shape = extract_class(new, "Foo");

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-inheritance-changed"),
        "Expected class-inheritance-changed, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-inheritance-changed"
                && f.message.contains("Base")
                && f.message.contains("OtherBase")),
        "Finding should mention old and new base classes"
    );
}

// ─── CLS-07: Static member type changed ──────────────────────────────────────
//
// tree-sitter cannot resolve types without LSP, so we construct ClassShapes
// manually with type_signature set (as unit-style test within the integration file).

#[test]
fn test_cls_07_static_type_changed() {
    // Manually construct shapes with type_signature — LSP not available in tests
    let old_member = ClassMember {
        name: "val".to_string(),
        kind: ClassMemberKind::Property,
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_readonly: false,
        param_count: 0,
        type_signature: Some("number".to_string()),
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
    };
    let new_member = ClassMember {
        name: "val".to_string(),
        kind: ClassMemberKind::Property,
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_readonly: false,
        param_count: 0,
        type_signature: Some("string".to_string()),
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
    };

    let old_shape = ClassShape {
        members: vec![old_member],
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: vec![],
    };
    let new_shape = ClassShape {
        members: vec![new_member],
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: vec![],
    };

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-static-type-changed"),
        "Expected class-static-type-changed, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-static-type-changed"
                && f.message.contains("number")
                && f.message.contains("string")),
        "Finding should mention old and new types"
    );
}

// ─── CLS-08: Readonly modifier changed ───────────────────────────────────────

#[test]
fn test_cls_08_readonly_changed() {
    let old = b"export class Foo { name: string; }";
    let new = b"export class Foo { readonly name: string; }";

    let old_shape = extract_class(old, "Foo");
    let new_shape = extract_class(new, "Foo");

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "class-readonly-changed"),
        "Expected class-readonly-changed, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── CLS-09: Implements list reduced ─────────────────────────────────────────

#[test]
fn test_cls_09_implements_removed() {
    let old = b"export class Foo implements A, B {}";
    let new = b"export class Foo implements A {}";

    let old_shape = extract_class(old, "Foo");
    let new_shape = extract_class(new, "Foo");

    let findings =
        class_diff::compare_class_shapes(&old_shape, &new_shape, "Foo", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-implements-removed"),
        "Expected class-implements-removed, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-implements-removed" && f.message.contains("B")),
        "Finding should mention removed interface B"
    );
}

// ─── ENUM-01: Enum member removed ────────────────────────────────────────────

#[test]
fn test_enum_01_member_removed() {
    let old = b"export enum Color { Red, Green, Blue }";
    let new = b"export enum Color { Red, Blue }";

    let old_shape = extract_enum(old, "Color");
    let new_shape = extract_enum(new, "Color");

    let findings =
        enum_diff::compare_enum_shapes(&old_shape, &new_shape, "Color", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "enum-member-removed"),
        "Expected enum-member-removed, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "enum-member-removed" && f.message.contains("Green")),
        "Finding should mention removed member Green"
    );
}

// ─── ENUM-02: Enum member value changed ──────────────────────────────────────

#[test]
fn test_enum_02_member_value_changed() {
    let old = br#"export enum Color { Red = "red" }"#;
    let new = br#"export enum Color { Red = "RED" }"#;

    let old_shape = extract_enum(old, "Color");
    let new_shape = extract_enum(new, "Color");

    let findings =
        enum_diff::compare_enum_shapes(&old_shape, &new_shape, "Color", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "enum-member-value-changed"),
        "Expected enum-member-value-changed, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── ENUM-03: Enum member reordered with value shift ─────────────────────────

#[test]
fn test_enum_03_member_reordered() {
    let old = b"export enum Color { Red, Green, Blue }";
    let new = b"export enum Color { Green, Red, Blue }";

    let old_shape = extract_enum(old, "Color");
    let new_shape = extract_enum(new, "Color");

    let findings =
        enum_diff::compare_enum_shapes(&old_shape, &new_shape, "Color", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "enum-member-reordered"),
        "Expected enum-member-reordered, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── ENUM-04: const enum to regular enum ─────────────────────────────────────

#[test]
fn test_enum_04_const_to_regular() {
    let old = b"export const enum Dir { Up, Down }";
    let new = b"export enum Dir { Up, Down }";

    let old_shape = extract_enum(old, "Dir");
    let new_shape = extract_enum(new, "Dir");

    let findings =
        enum_diff::compare_enum_shapes(&old_shape, &new_shape, "Dir", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "enum-const-to-regular"),
        "Expected enum-const-to-regular, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── ENUM-05: Numeric enum changed to string enum ────────────────────────────

#[test]
fn test_enum_05_numeric_to_string() {
    let old = b"export enum Dir { Up = 0, Down = 1 }";
    let new = br#"export enum Dir { Up = "up", Down = "down" }"#;

    let old_shape = extract_enum(old, "Dir");
    let new_shape = extract_enum(new, "Dir");

    let findings =
        enum_diff::compare_enum_shapes(&old_shape, &new_shape, "Dir", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "enum-numeric-to-string"),
        "Expected enum-numeric-to-string, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── GEN-01: Required type param added ───────────────────────────────────────

#[test]
fn test_gen_01_required_type_param_added() {
    let old = b"export function foo<T>(): T { return null as any; }";
    let new = b"export function foo<T, U>(): T { return null as any; }";

    let old_params = extract_type_params_from(old, "foo");
    let new_params = extract_type_params_from(new, "foo");

    let findings =
        generics_diff::compare_type_params(&old_params, &new_params, "foo", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "generic-param-added"),
        "Expected generic-param-added, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "generic-param-added" && f.message.contains("U")),
        "Finding should mention the added param U"
    );
}

// ─── GEN-02: Type param removed ──────────────────────────────────────────────

#[test]
fn test_gen_02_type_param_removed() {
    let old = b"export function foo<T, U>(): T { return null as any; }";
    let new = b"export function foo<T>(): T { return null as any; }";

    let old_params = extract_type_params_from(old, "foo");
    let new_params = extract_type_params_from(new, "foo");

    let findings =
        generics_diff::compare_type_params(&old_params, &new_params, "foo", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "generic-param-removed"),
        "Expected generic-param-removed, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "generic-param-removed" && f.message.contains("U")),
        "Finding should mention the removed param U"
    );
}

// ─── GEN-03: Type params reordered ───────────────────────────────────────────

#[test]
fn test_gen_03_type_params_reordered() {
    let old = b"export function foo<T, U>(): T { return null as any; }";
    let new = b"export function foo<U, T>(): T { return null as any; }";

    let old_params = extract_type_params_from(old, "foo");
    let new_params = extract_type_params_from(new, "foo");

    let findings =
        generics_diff::compare_type_params(&old_params, &new_params, "foo", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "generic-param-reordered"),
        "Expected generic-param-reordered, got: {:?}",
        rule_ids(&findings)
    );
}

// ─── GEN-04: Constraint tightened ────────────────────────────────────────────

#[test]
fn test_gen_04_constraint_tightened() {
    // Adding a constraint where none existed = tightening (text fallback, no LSP)
    let old = b"export function foo<T>(): T { return null as any; }";
    let new = b"export function foo<T extends string>(): T { return null as any; }";

    let old_params = extract_type_params_from(old, "foo");
    let new_params = extract_type_params_from(new, "foo");

    let findings =
        generics_diff::compare_type_params(&old_params, &new_params, "foo", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "generic-constraint-tightened"),
        "Expected generic-constraint-tightened, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "generic-constraint-tightened" && f.message.contains("string")),
        "Finding should mention the constraint type"
    );
}

// ─── GEN-05: Default type changed ────────────────────────────────────────────

#[test]
fn test_gen_05_default_changed() {
    let old = b"export function foo<T = string>(): T { return null as any; }";
    let new = b"export function foo<T = number>(): T { return null as any; }";

    let old_params = extract_type_params_from(old, "foo");
    let new_params = extract_type_params_from(new, "foo");

    let findings =
        generics_diff::compare_type_params(&old_params, &new_params, "foo", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == "generic-default-changed"),
        "Expected generic-default-changed, got: {:?}",
        rule_ids(&findings)
    );
    assert!(
        findings.iter().any(|f| f.rule == "generic-default-changed"
            && f.message.contains("string")
            && f.message.contains("number")),
        "Finding should mention old and new defaults"
    );
}

// ─── GEN-06: Type param with default added ────────────────────────────────────

#[test]
fn test_gen_06_param_added_with_default() {
    let old = b"export function foo<T>(): T { return null as any; }";
    let new = b"export function foo<T, U = string>(): T { return null as any; }";

    let old_params = extract_type_params_from(old, "foo");
    let new_params = extract_type_params_from(new, "foo");

    let findings =
        generics_diff::compare_type_params(&old_params, &new_params, "foo", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "generic-param-added-with-default"),
        "Expected generic-param-added-with-default, got: {:?}",
        rule_ids(&findings)
    );

    // GEN-06 must be Minor severity
    let gen06_finding = findings
        .iter()
        .find(|f| f.rule == "generic-param-added-with-default")
        .expect("GEN-06 finding must exist");
    assert_eq!(
        gen06_finding.severity,
        interlint::types::Severity::Minor,
        "GEN-06 must be Minor severity"
    );
    assert!(
        gen06_finding.message.contains("U"),
        "Finding should mention the added param U"
    );
}
