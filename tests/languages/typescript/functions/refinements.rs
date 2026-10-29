//! Refinements compatibility checks.

use interlint::core::findings::Finding;
use interlint::languages::javascript::model::{DeclarationKind, ExportKind, ExportSymbol};
use interlint::languages::typescript::compare::refinements as ref_diff;
use interlint::languages::typescript::compare::rules::{
    RULE_OVERLOAD_REMOVED, RULE_REST_PARAM_NARROWED, RULE_TYPE_GUARD_WIDENED,
};
use interlint::languages::typescript::types::shape::{ParamShape, PrimitiveKind, TypeShape};
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

/// Build an ExportSymbol for the named function with extracted overloads from source.
fn make_fn_sym_with_overloads(source: &[u8], name: &str) -> ExportSymbol {
    let (tree, bytes) = parse_ts(source);
    let root = tree.root_node();

    // Collect all function_signature nodes with this name (overloads)
    let mut overloads = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "export_statement" {
            let mut ec = child.walk();
            for export_child in child.children(&mut ec) {
                if export_child.kind() == "function_signature"
                    || export_child.kind() == "function_declaration"
                {
                    let matches_name = export_child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(&bytes).ok())
                        .map(|n| n == name)
                        .unwrap_or(false);
                    if matches_name {
                        if let Some(shape) =
                            interlint::languages::javascript::syntax::functions::build_function_shape_from_node(export_child, &bytes)
                        {
                            overloads.push(shape);
                        }
                    }
                }
            }
        }
    }

    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads,
        decl_kinds: vec![DeclarationKind::Function],
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
    }
}

#[test]
fn overload_removed() {
    let old_src = b"
        export function foo(x: string): string;
        export function foo(x: number, y: number): number;
        export function foo(x: any): any { return x; }
    ";
    let new_src = b"
        export function foo(x: string): string;
        export function foo(x: any): any { return x; }
    ";

    let old_sym = make_fn_sym_with_overloads(old_src, "foo");
    let new_sym = make_fn_sym_with_overloads(new_src, "foo");

    // old_sym should have 2 overload signatures (arity 1 and arity 2)
    // new_sym should have 1 overload signature (arity 1)
    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "foo", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED),
        "Expected overload-removed; got: {:?}",
        rule_ids(&findings)
    );
}

#[test]
fn rest_param_narrowed() {
    // Build ExportSymbols manually with known overload shapes
    let mut old_sym = ExportSymbol {
        name: "bar".to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: vec![TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "args".to_string(),
                optional: false,
                rest: true,
                shape: TypeShape::Opaque("string[]".to_string()),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }],
        decl_kinds: vec![DeclarationKind::Function],
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
        name: "bar".to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: vec![TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Opaque("string".to_string()),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        }],
        decl_kinds: vec![DeclarationKind::Function],
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

    // Old has rest param, new does not — rest-param-narrowed
    old_sym.overloads = vec![TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "args".to_string(),
            optional: false,
            rest: true,
            shape: TypeShape::Opaque("string[]".to_string()),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
    }];

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "bar", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings.iter().any(|f| f.rule == RULE_REST_PARAM_NARROWED),
        "Expected rest-param-narrowed; got: {:?}",
        rule_ids(&findings)
    );
}

#[test]
fn type_guard_widened() {
    // Use tree-sitter to extract type_guard_type from old source
    let old_src =
        b"export function isStr(x: unknown): x is string { return typeof x === \"string\"; }";
    let new_src = b"export function isStr(x: unknown): boolean { return typeof x === \"string\"; }";

    let (old_tree, old_bytes) = parse_ts(old_src);
    let (new_tree, new_bytes) = parse_ts(new_src);

    let old_decl = find_decl_by_name(old_tree.root_node(), &old_bytes, "isStr")
        .expect("isStr not found in old source");
    let new_decl = find_decl_by_name(new_tree.root_node(), &new_bytes, "isStr")
        .expect("isStr not found in new source");

    let old_type_guard = interlint::languages::typescript::syntax::functions::extract_type_guard(
        old_decl, &old_bytes,
    );
    let new_type_guard = interlint::languages::typescript::syntax::functions::extract_type_guard(
        new_decl, &new_bytes,
    );

    let old_sym = ExportSymbol {
        name: "isStr".to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: vec![DeclarationKind::Function],
        type_guard_type: old_type_guard,
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
        name: "isStr".to_string(),
        kind: ExportKind::Named,
        file: test_file(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: vec![DeclarationKind::Function],
        type_guard_type: new_type_guard,
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

    let findings =
        ref_diff::compare_ref_changes(&old_sym, &new_sym, "isStr", &test_file(), 1, 1, &[], &[]);

    assert!(
        findings.iter().any(|f| f.rule == RULE_TYPE_GUARD_WIDENED),
        "Expected type-guard-widened; old type_guard={:?}, new type_guard={:?}; got: {:?}",
        old_sym.type_guard_type,
        new_sym.type_guard_type,
        rule_ids(&findings)
    );
}

/// Extract rule IDs from a findings slice.
fn rule_ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}
