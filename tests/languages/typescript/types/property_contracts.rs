//! Property contracts compatibility checks.

use interlint::compare::subtype::{RULE_INDEX_SIGNATURE_CHANGED, RULE_PROPERTY_MADE_REQUIRED};
use interlint::compare::{ref_diff, type_diff};
use interlint::discover::tree_query::{self, IndexSignature};
use interlint::types::shape::{FieldShape, PrimitiveKind, TypeShape};
use interlint::types::{DeclarationKind, ExportKind, ExportSymbol, Finding};
use std::collections::BTreeMap;
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

/// Find a declaration node by name, recursing into export_statement wrappers.
fn find_decl_by_name<'a>(
    root: tree_sitter::Node<'a>,
    source: &[u8],
    name: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind();

        let decl_kinds = [
            "class_declaration",
            "abstract_class_declaration",
            "enum_declaration",
            "function_declaration",
            "function_signature",
            "interface_declaration",
            "type_alias_declaration",
            "internal_module",
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

#[test]
fn property_made_required() {
    // Test REF-06 via type_diff::diff_types directly with constructed TypeShape::Object
    let mut old_fields = BTreeMap::new();
    old_fields.insert(
        "name".to_string(),
        FieldShape {
            write_type: None,
            optional: true,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );

    let mut new_fields = BTreeMap::new();
    new_fields.insert(
        "name".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );

    let findings = type_diff::diff_types(
        &TypeShape::Object(old_fields),
        &TypeShape::Object(new_fields),
        "MyType",
        &test_file(),
        1,
        1,
    );

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_PROPERTY_MADE_REQUIRED),
        "Expected property-made-required; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn index_signature_changed() {
    // Use tree-sitter to extract index signatures from interface source
    let old_src = b"export interface Dict { [key: string]: number; }";
    let new_src = b"export interface Dict { [key: string]: string; }";

    let (old_tree, old_bytes) = parse_ts(old_src);
    let (new_tree, new_bytes) = parse_ts(new_src);

    let old_decl = find_decl_by_name(old_tree.root_node(), &old_bytes, "Dict")
        .expect("Dict not found in old source");
    let new_decl = find_decl_by_name(new_tree.root_node(), &new_bytes, "Dict")
        .expect("Dict not found in new source");

    let old_sigs = tree_query::find_interface_body(old_decl)
        .map(|body| tree_query::extract_index_signatures(body, &old_bytes))
        .unwrap_or_default();

    let new_sigs = tree_query::find_interface_body(new_decl)
        .map(|body| tree_query::extract_index_signatures(body, &new_bytes))
        .unwrap_or_default();

    let old_sym = ExportSymbol {
        name: "Dict".to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: vec![DeclarationKind::Interface],
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    };

    let new_sym = ExportSymbol {
        name: "Dict".to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: vec![DeclarationKind::Interface],
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    };

    let findings = ref_diff::compare_ref_changes(
        &old_sym,
        &new_sym,
        "Dict",
        &test_file(),
        1,
        1,
        &old_sigs,
        &new_sigs,
    );

    assert!(
        findings.iter().any(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED),
        "Expected index-signature-changed (value type changed number->string); old_sigs={:?}, new_sigs={:?}; got: {:?}",
        old_sigs,
        new_sigs,
        rule_ids(&findings)
    );

    let f = findings
        .iter()
        .find(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED)
        .unwrap();
    assert!(
        f.message.contains("number") && f.message.contains("string"),
        "Message should mention old and new types: {}",
        f.message
    );
}

#[test]
fn index_signature_removed() {
    let old_sigs = vec![IndexSignature {
        key_type_text: "string".to_string(),
        value_type_text: "number".to_string(),
    }];

    let old_sym = ExportSymbol {
        name: "Dict".to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: vec![DeclarationKind::Interface],
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    };

    let new_sym = old_sym.clone();

    let findings = ref_diff::compare_ref_changes(
        &old_sym,
        &new_sym,
        "Dict",
        &test_file(),
        1,
        1,
        &old_sigs,
        &[],
    );

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED),
        "Expected index-signature-changed (removed); got: {:?}",
        rule_ids(&findings)
    );
}

/// Extract rule IDs from a findings slice.
fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}
