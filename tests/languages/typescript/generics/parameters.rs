//! Parameters compatibility checks.

use pledge::core::findings::Finding;
use pledge::languages::typescript::compare::generics as generics_diff;
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

/// Parse TS source and extract TypeParams for the named declaration.
fn extract_type_params_from(
    source: &[u8],
    name: &str,
) -> Vec<pledge::languages::typescript::model::TypeParam> {
    let (tree, bytes) = parse_ts(source);
    let decl = find_decl_by_name(tree.root_node(), &bytes, name)
        .unwrap_or_else(|| panic!("declaration `{}` not found in source", name));
    pledge::languages::typescript::syntax::generics::parse_type_params(&bytes, decl)
}

#[test]
fn required_type_param_added() {
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

#[test]
fn type_param_removed() {
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

#[test]
fn type_params_reordered() {
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

#[test]
fn constraint_tightened() {
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

#[test]
fn default_changed() {
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

#[test]
fn param_added_with_default() {
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
        pledge::core::findings::Severity::Minor,
        "GEN-06 must be Minor severity"
    );
    assert!(
        gen06_finding.message.contains("U"),
        "Finding should mention the added param U"
    );
}

/// Extract rule IDs from a findings slice.
fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}
