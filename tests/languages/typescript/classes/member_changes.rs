//! Member changes compatibility checks.

use interlint::compare::class_diff;
use interlint::discover::tree_query;
use interlint::types::{ClassMember, ClassMemberKind, ClassShape, Finding, Visibility};
use std::path::PathBuf;

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

#[test]
fn constructor_param_added() {
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

#[test]
fn method_visibility_changed() {
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

#[test]
fn method_removed() {
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

#[test]
fn class_made_abstract() {
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

#[test]
fn abstract_method_added() {
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

#[test]
fn inheritance_changed() {
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

#[test]
fn static_type_changed() {
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
        decorators: Vec::new(),
        is_private: false,
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
        decorators: Vec::new(),
        is_private: false,
    };

    let old_shape = ClassShape {
        members: vec![old_member],
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: vec![],
        decorators: Vec::new(),
        has_dynamic_static_key: false,
    };
    let new_shape = ClassShape {
        members: vec![new_member],
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: vec![],
        decorators: Vec::new(),
        has_dynamic_static_key: false,
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

#[test]
fn readonly_changed() {
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

#[test]
fn implements_removed() {
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

/// Extract rule IDs from a findings slice.
fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}
