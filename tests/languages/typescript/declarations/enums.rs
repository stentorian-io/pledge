//! Enums compatibility checks.

use pledge::core::findings::Finding;
use pledge::languages::typescript::compare::enums as enum_diff;
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

/// Parse TS source and extract EnumShape for the named enum.
fn extract_enum(source: &[u8], name: &str) -> pledge::languages::typescript::model::EnumShape {
    let (tree, bytes) = parse_ts(source);
    let decl = find_decl_by_name(tree.root_node(), &bytes, name)
        .unwrap_or_else(|| panic!("enum `{}` not found in source", name));
    pledge::languages::typescript::syntax::enums::parse_enum_shape(&bytes, decl)
}

#[test]
fn member_removed() {
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

#[test]
fn member_value_changed() {
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

#[test]
fn member_reordered() {
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

#[test]
fn const_to_regular() {
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

#[test]
fn numeric_to_string() {
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

/// Extract rule IDs from a findings slice.
fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}
