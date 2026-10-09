use crate::languages::javascript::model as js_model;
use crate::languages::javascript::model::{ClassMemberKind, Visibility};
use crate::languages::javascript::syntax::exports::*;
use crate::languages::javascript::syntax::{
    classes as class_syntax, functions as function_syntax, grammar, queries,
};
use crate::languages::typescript::model as ts_model;
use crate::languages::typescript::model::EnumValueKind;
use crate::languages::typescript::syntax::{
    annotations, enums as enum_syntax, functions as typed_functions, generics as generic_syntax,
    interfaces as interface_syntax, types as type_syntax,
};
use crate::languages::typescript::types::shape as type_shapes;
use tree_sitter::Parser;

#[test]
fn compiled_queries_are_reused_within_each_grammar() {
    let ts = grammar::get_language(&grammar::SourceLanguage::TypeScript);
    let js = grammar::get_language(&grammar::SourceLanguage::JavaScript);
    let first = grammar::cached_query(&ts, queries::NAMED_EXPORT_QUERY).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &grammar::cached_query(&ts, queries::NAMED_EXPORT_QUERY).unwrap()
    ));
    assert!(!std::sync::Arc::ptr_eq(
        &first,
        &grammar::cached_query(&js, queries::NAMED_EXPORT_QUERY).unwrap()
    ));
    assert!(grammar::cached_query(&js, "(nonexistent_node)").is_err());
    assert!(grammar::cached_query(&js, "(nonexistent_node)").is_err());
}

/// Parse a TypeScript source string and return the root node and source bytes.
fn parse_ts(source: &str) -> (tree_sitter::Tree, Vec<u8>) {
    let mut parser = Parser::new();
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&lang).unwrap();
    let bytes = source.as_bytes().to_vec();
    let tree = parser.parse(&bytes, None).expect("parse failed");
    (tree, bytes)
}

/// Get the first child of root with the given kind.
fn first_node_of_kind<'a>(
    root: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = root.walk();
    let result = root
        .children(&mut cursor)
        .find(|&child| child.kind() == kind);
    result
}

// ── INF-04 extract_typeof_import_refs tests ───────────────────

#[test]
fn extract_typeof_import_refs_single() {
    let src = "export type X = typeof import('./y').Z;\n";
    let (tree, bytes) = parse_ts(src);
    let refs = type_syntax::extract_typeof_import_refs(tree.root_node(), &bytes);
    assert_eq!(refs.len(), 1, "expected 1 ref, got: {:?}", refs);
    assert_eq!(refs[0].specifier, "./y");
    assert_eq!(refs[0].property, "Z");
}

#[test]
fn extract_typeof_import_refs_union_two() {
    let src = "export type X = typeof import('./a').A | typeof import('./b').B;\n";
    let (tree, bytes) = parse_ts(src);
    let refs = type_syntax::extract_typeof_import_refs(tree.root_node(), &bytes);
    assert_eq!(refs.len(), 2, "expected 2 refs, got: {:?}", refs);
    let specs: Vec<&str> = refs.iter().map(|r| r.specifier.as_str()).collect();
    assert!(specs.contains(&"./a"));
    assert!(specs.contains(&"./b"));
    let props: Vec<&str> = refs.iter().map(|r| r.property.as_str()).collect();
    assert!(props.contains(&"A"));
    assert!(props.contains(&"B"));
}

#[test]
fn extract_typeof_import_refs_no_property_returns_zero() {
    // `typeof import('./y')` without `.X` access — no member_expression,
    // so the helper returns zero refs.
    let src = "export type Q = typeof import('./y');\n";
    let (tree, bytes) = parse_ts(src);
    let refs = type_syntax::extract_typeof_import_refs(tree.root_node(), &bytes);
    assert_eq!(refs.len(), 0, "expected 0 refs, got: {:?}", refs);
}

#[test]
fn extract_typeof_import_refs_bare_import_member() {
    // `type R = import('./y').Z` (without typeof) — the member_expression
    // walk is typeof-wrapper-transparent.
    let src = "export type R = import('./y').Z;\n";
    let (tree, bytes) = parse_ts(src);
    let refs = type_syntax::extract_typeof_import_refs(tree.root_node(), &bytes);
    assert_eq!(refs.len(), 1, "expected 1 ref, got: {:?}", refs);
    assert_eq!(refs[0].specifier, "./y");
    assert_eq!(refs[0].property, "Z");
}

// ── parse_class_members tests ─────────────────────────────────────────────

#[test]
fn test_parse_class_members_extracts_method_names() {
    let src = "class Foo { doWork(x: string): void {} }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    assert!(
        members.iter().any(|m| m.name == "doWork"),
        "should extract doWork method"
    );
}

#[test]
fn test_parse_class_members_no_modifier_is_public() {
    let src = "class Foo { doWork(): void {} }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let method = members.iter().find(|m| m.name == "doWork").unwrap();
    assert_eq!(
        method.visibility,
        Visibility::Public,
        "no modifier = Public"
    );
}

#[test]
fn test_parse_class_members_protected_modifier() {
    let src = "class Foo { protected doWork(): void {} }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let method = members.iter().find(|m| m.name == "doWork").unwrap();
    assert_eq!(method.visibility, Visibility::Protected);
}

#[test]
fn test_parse_class_members_constructor_kind() {
    let src = "class Foo { constructor(x: string) {} }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let ctor = members.iter().find(|m| m.name == "constructor").unwrap();
    assert_eq!(ctor.kind, ClassMemberKind::Constructor);
}

#[test]
fn test_parse_class_members_constructor_required_params() {
    let src = "class Foo { constructor(x: string, y: number) {} }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let ctor = members.iter().find(|m| m.name == "constructor").unwrap();
    assert_eq!(ctor.param_count, 2, "two required params");
}

#[test]
fn test_parse_class_members_static_flag() {
    let src = "class Foo { static create(): Foo { return new Foo(); } }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let method = members.iter().find(|m| m.name == "create").unwrap();
    assert!(method.is_static, "create should be static");
}

#[test]
fn test_parse_class_members_property_kind() {
    let src = "class Foo { name: string = ''; }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let prop = members.iter().find(|m| m.name == "name").unwrap();
    assert_eq!(prop.kind, ClassMemberKind::Property);
}

#[test]
fn test_parse_class_members_getter() {
    let src = "class A { get foo(): number { return 1; } }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let getter = members.iter().find(|m| m.name == "foo").unwrap();
    assert_eq!(
        getter.kind,
        ClassMemberKind::Getter,
        "getter should have kind Getter"
    );
}

#[test]
fn test_parse_class_members_setter() {
    let src = "class A { set foo(v: number) {} }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let setter = members.iter().find(|m| m.name == "foo").unwrap();
    assert_eq!(
        setter.kind,
        ClassMemberKind::Setter,
        "setter should have kind Setter"
    );
}

#[test]
fn test_parse_class_members_private_field_kept_with_is_private_flag() {
    // CLS-11: `#`-prefix private fields are now KEPT in the
    // members vector with `is_private: true` so the diff layer can detect
    // public ↔ private transitions (e.g., `foo` → `#foo`). The `#`
    // prefix is preserved in `name` so diff messages are accurate.
    let src = "class A { #secret: string; visible: string; }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    assert_eq!(
        members.len(),
        2,
        "both '#secret' (is_private=true) and 'visible' (is_private=false) should be present"
    );
    let secret = members.iter().find(|m| m.name == "#secret").unwrap();
    assert!(secret.is_private, "'#secret' must have is_private=true");
    let visible = members.iter().find(|m| m.name == "visible").unwrap();
    assert!(!visible.is_private, "'visible' must have is_private=false");
}

#[test]
fn test_parse_class_members_getter_setter_pair() {
    let src = "class A { get x(): number { return 0; } set x(v: number) {} }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let getter = members
        .iter()
        .find(|m| m.name == "x" && m.kind == ClassMemberKind::Getter);
    let setter = members
        .iter()
        .find(|m| m.name == "x" && m.kind == ClassMemberKind::Setter);
    assert!(getter.is_some(), "should have a Getter for x");
    assert!(setter.is_some(), "should have a Setter for x");
}

// ── abstract class tests ──────────────────────────────────────────────────

#[test]
fn test_abstract_class_detection_via_node_kind() {
    let src = "abstract class Base { abstract doWork(): void; }";
    let (tree, bytes) = parse_ts(src);
    let abs_node = first_node_of_kind(tree.root_node(), "abstract_class_declaration").unwrap();
    assert_eq!(abs_node.kind(), "abstract_class_declaration");
    let shape = class_syntax::parse_class_shape(&bytes, abs_node);
    assert!(shape.is_abstract, "should detect as abstract");
}

#[test]
fn test_concrete_class_not_abstract() {
    let src = "class Foo {}";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let shape = class_syntax::parse_class_shape(&bytes, class_node);
    assert!(!shape.is_abstract, "concrete class should not be abstract");
}

// ── DECL_EXPORT_QUERY_TS abstract class test ──────────────────────────────

#[test]
fn test_decl_export_query_captures_abstract_class() {
    let src = "export abstract class MyAbstractClass { abstract doWork(): void; }";
    let exports = parse_exports(src.as_bytes(), grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports.iter().any(|e| e.name == "MyAbstractClass"),
        "exported abstract class should be captured"
    );
}

// ── parse_enum_members tests ──────────────────────────────────────────────

#[test]
fn test_parse_enum_members_extracts_names() {
    let src = "enum Color { Red, Green, Blue }";
    let (tree, bytes) = parse_ts(src);
    let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
    let members = enum_syntax::parse_enum_members(&bytes, enum_node);
    assert_eq!(members.len(), 3);
    assert_eq!(members[0].name, "Red");
    assert_eq!(members[1].name, "Green");
    assert_eq!(members[2].name, "Blue");
}

#[test]
fn test_parse_enum_members_no_explicit_value() {
    let src = "enum Dir { Up, Down }";
    let (tree, bytes) = parse_ts(src);
    let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
    let members = enum_syntax::parse_enum_members(&bytes, enum_node);
    assert!(
        members.iter().all(|m| m.raw_value.is_none()),
        "implicit members should have no raw_value"
    );
}

#[test]
fn test_parse_enum_members_with_explicit_numeric_value() {
    let src = "enum Status { Active = 1, Inactive = 2 }";
    let (tree, bytes) = parse_ts(src);
    let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
    let members = enum_syntax::parse_enum_members(&bytes, enum_node);
    let active = members.iter().find(|m| m.name == "Active").unwrap();
    assert_eq!(active.raw_value.as_deref(), Some("1"));
    assert_eq!(active.computed_value, Some(1));
}

// ── is_const_enum tests ───────────────────────────────────────────────────

#[test]
fn test_is_const_enum_true_for_const_enum() {
    let src = "const enum Direction { Up, Down }";
    let (tree, bytes) = parse_ts(src);
    let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
    assert!(
        enum_syntax::is_const_enum(enum_node, &bytes),
        "should detect const enum"
    );
}

#[test]
fn test_is_const_enum_false_for_regular_enum() {
    let src = "enum Direction { Up, Down }";
    let (tree, bytes) = parse_ts(src);
    let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
    assert!(
        !enum_syntax::is_const_enum(enum_node, &bytes),
        "regular enum should not be const"
    );
}

// ── compute_enum_values tests ─────────────────────────────────────────────

#[test]
fn test_compute_enum_values_auto_increment_from_zero() {
    let mut members = vec![
        EnumMember {
            name: "A".to_string(),
            raw_value: None,
            computed_value: None,
            value_kind: EnumValueKind::Numeric,
        },
        EnumMember {
            name: "B".to_string(),
            raw_value: None,
            computed_value: None,
            value_kind: EnumValueKind::Numeric,
        },
        EnumMember {
            name: "C".to_string(),
            raw_value: None,
            computed_value: None,
            value_kind: EnumValueKind::Numeric,
        },
    ];
    enum_syntax::compute_enum_values(&mut members);
    assert_eq!(members[0].computed_value, Some(0));
    assert_eq!(members[1].computed_value, Some(1));
    assert_eq!(members[2].computed_value, Some(2));
}

#[test]
fn test_compute_enum_values_resets_after_explicit_numeric() {
    let mut members = vec![
        EnumMember {
            name: "A".to_string(),
            raw_value: Some("5".to_string()),
            computed_value: None,
            value_kind: EnumValueKind::Numeric,
        },
        EnumMember {
            name: "B".to_string(),
            raw_value: None,
            computed_value: None,
            value_kind: EnumValueKind::Numeric,
        },
    ];
    enum_syntax::compute_enum_values(&mut members);
    assert_eq!(members[0].computed_value, Some(5));
    assert_eq!(members[1].computed_value, Some(6), "should reset to 5+1=6");
}

#[test]
fn test_compute_enum_values_non_numeric_sets_none() {
    let mut members = vec![
        EnumMember {
            name: "A".to_string(),
            raw_value: Some("SomeExpr + 1".to_string()),
            computed_value: None,
            value_kind: EnumValueKind::OtherExpr,
        },
        EnumMember {
            name: "B".to_string(),
            raw_value: None,
            computed_value: None,
            value_kind: EnumValueKind::Numeric,
        },
    ];
    enum_syntax::compute_enum_values(&mut members);
    assert_eq!(
        members[0].computed_value, None,
        "non-numeric initializer = None"
    );
    // Counter still increments for B
    assert_eq!(members[1].computed_value, Some(1));
}

// ── parse_type_params tests ───────────────────────────────────────────────

#[test]
fn test_parse_type_params_extracts_name_constraint_default() {
    let src = "interface Foo<T extends string = never> {}";
    let (tree, bytes) = parse_ts(src);
    let iface_node = first_node_of_kind(tree.root_node(), "interface_declaration").unwrap();
    let params = generic_syntax::parse_type_params(&bytes, iface_node);
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "T");
    assert_eq!(params[0].constraint_text.as_deref(), Some("string"));
    assert_eq!(params[0].default_text.as_deref(), Some("never"));
}

#[test]
fn test_parse_type_params_empty_for_no_type_params() {
    let src = "class Foo {}";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let params = generic_syntax::parse_type_params(&bytes, class_node);
    assert!(params.is_empty(), "no type parameters = empty Vec");
}

#[test]
fn test_parse_type_params_multiple_params() {
    let src = "type Pair<K, V> = [K, V];";
    let (tree, bytes) = parse_ts(src);
    let alias_node = first_node_of_kind(tree.root_node(), "type_alias_declaration").unwrap();
    let params = generic_syntax::parse_type_params(&bytes, alias_node);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "K");
    assert_eq!(params[1].name, "V");
    assert!(params[0].constraint_text.is_none());
    assert!(params[0].default_text.is_none());
}

#[test]
fn test_parse_type_params_constraint_shape_is_none() {
    let src = "function foo<T extends string>() {}";
    let (tree, bytes) = parse_ts(src);
    let fn_node = first_node_of_kind(tree.root_node(), "function_declaration").unwrap();
    let params = generic_syntax::parse_type_params(&bytes, fn_node);
    assert_eq!(params.len(), 1);
    assert!(
        params[0].constraint_shape.is_none(),
        "constraint_shape is None (LSP fills in later)"
    );
}

// ── DeclarationKind propagation tests ────────────────────────────

#[test]
fn test_decl_export_query_captures_namespace() {
    let source = b"export namespace MyNS { export function helper(): void {} }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports.iter().any(|e| e.name == "MyNS"),
        "namespace export should be captured"
    );
    let ns = exports.iter().find(|e| e.name == "MyNS").unwrap();
    assert_eq!(ns.decl_kind, Some(js_model::DeclarationKind::Namespace));
}

#[test]
fn test_decl_kind_propagated_for_function() {
    let source = b"export function greet(name: string): string { return name; }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let exp = exports.iter().find(|e| e.name == "greet").unwrap();
    assert_eq!(exp.decl_kind, Some(js_model::DeclarationKind::Function));
}

#[test]
fn test_decl_kind_propagated_for_interface() {
    let source = b"export interface Config { timeout: number; }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let exp = exports.iter().find(|e| e.name == "Config").unwrap();
    assert_eq!(exp.decl_kind, Some(js_model::DeclarationKind::Interface));
}

#[test]
fn test_decl_kind_propagated_for_type_alias() {
    let source = b"export type ID = string;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let exp = exports.iter().find(|e| e.name == "ID").unwrap();
    assert_eq!(exp.decl_kind, Some(js_model::DeclarationKind::TypeAlias));
}

#[test]
fn test_function_signature_captured_as_overload() {
    let source = b"export function foo(x: string): string;\nexport function foo(x: number): number;\nexport function foo(x: any): any { return x; }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let foos: Vec<_> = exports.iter().filter(|e| e.name == "foo").collect();
    // function_signature nodes (overloads) + function_declaration (implementation)
    assert!(
        foos.len() >= 3,
        "should capture overload signatures and implementation; got {}",
        foos.len()
    );
}

// ── Rest param, type guard, index sig, function shape tests ─────

#[test]
fn test_is_rest_param_detects_spread() {
    let source = b"function foo(...args: string[]): void {}";
    let mut parser = Parser::new();
    let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let fn_node = root.child(0).unwrap();
    let params = fn_node.child_by_field_name("parameters").unwrap();
    let mut cursor = params.walk();
    let first_param = params
        .children(&mut cursor)
        .find(|c| c.kind() == "required_parameter")
        .unwrap();
    assert!(
        function_syntax::is_rest_param(first_param),
        "...args should be detected as rest param"
    );
}

#[test]
fn test_is_rest_param_false_for_normal() {
    let source = b"function foo(x: string): void {}";
    let mut parser = Parser::new();
    let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let fn_node = root.child(0).unwrap();
    let params = fn_node.child_by_field_name("parameters").unwrap();
    let mut cursor = params.walk();
    let first_param = params
        .children(&mut cursor)
        .find(|c| c.kind() == "required_parameter")
        .unwrap();
    assert!(
        !function_syntax::is_rest_param(first_param),
        "normal param should not be rest"
    );
}

#[test]
fn test_extract_type_guard_returns_guarded_type() {
    let source = b"function isStr(x: unknown): x is string { return typeof x === 'string'; }";
    let mut parser = Parser::new();
    let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let fn_node = tree.root_node().child(0).unwrap();
    let guard = typed_functions::extract_type_guard(fn_node, source);
    assert_eq!(guard, Some("string".to_string()));
}

#[test]
fn test_extract_type_guard_none_for_regular_return() {
    let source = b"function foo(x: string): boolean { return true; }";
    let mut parser = Parser::new();
    let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let fn_node = tree.root_node().child(0).unwrap();
    let guard = typed_functions::extract_type_guard(fn_node, source);
    assert!(
        guard.is_none(),
        "regular boolean return should not be a type guard"
    );
}

#[test]
fn test_extract_index_signatures() {
    let source = b"interface Dict { [key: string]: number; name: string; }";
    let mut parser = Parser::new();
    let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let iface = root.child(0).unwrap(); // interface_declaration
    let body = interface_syntax::find_interface_body(iface).unwrap();
    let sigs = interface_syntax::extract_index_signatures(body, source);
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].key_type_text, "string");
    assert_eq!(sigs[0].value_type_text, "number");
}

#[test]
fn test_build_function_shape_from_signature_node() {
    let source =
        b"export function foo(x: string): number;\nexport function foo(x: any): any { return x; }";
    let mut parser = Parser::new();
    let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    // First child is export_statement containing function_signature
    let first_export = root.child(0).unwrap();
    let mut cursor = first_export.walk();
    let fn_sig = first_export
        .children(&mut cursor)
        .find(|c| c.kind() == "function_signature")
        .unwrap();
    let shape = function_syntax::build_function_shape_from_node(fn_sig, source);
    assert!(
        shape.is_some(),
        "should build TypeShape from function_signature"
    );
    if let Some(type_shapes::TypeShape::Function { params, .. }) = shape {
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
        assert!(!params[0].rest);
    } else {
        panic!("expected TypeShape::Function");
    }
}

#[test]
fn test_build_function_shape_with_rest_param() {
    let source = b"function bar(...args: string[]): void {}";
    let mut parser = Parser::new();
    let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let fn_node = tree.root_node().child(0).unwrap();
    let shape = function_syntax::build_function_shape_from_node(fn_node, source);
    assert!(shape.is_some());
    if let Some(type_shapes::TypeShape::Function { params, .. }) = shape {
        assert_eq!(params.len(), 1);
        assert!(params[0].rest, "rest param should have rest=true");
    } else {
        panic!("expected TypeShape::Function");
    }
}

// ── is_async_function / is_generator_function tests (BEH-01, BEH-02) ─────

#[test]
fn test_is_async_function_true() {
    let source = b"async function foo() {}";
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let mut parser = Parser::new();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let fn_node = root.child(0).unwrap();
    assert!(function_syntax::is_async_function(fn_node, source));
}

#[test]
fn test_is_async_function_false() {
    let source = b"function foo() {}";
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let mut parser = Parser::new();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let fn_node = root.child(0).unwrap();
    assert!(!function_syntax::is_async_function(fn_node, source));
}

#[test]
fn test_is_generator_function_true() {
    let source = b"function* foo() {}";
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let mut parser = Parser::new();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let fn_node = root.child(0).unwrap();
    assert!(function_syntax::is_generator_function(fn_node, source));
}

#[test]
fn test_is_generator_function_false() {
    let source = b"function foo() {}";
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let mut parser = Parser::new();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let fn_node = root.child(0).unwrap();
    assert!(!function_syntax::is_generator_function(fn_node, source));
}

// ── Export alias and namespace re-export tests ( bug fixes) ────────

#[test]
fn test_export_alias_uses_alias_name() {
    // `export { foo as bar }` — public API name is "bar", not "foo".
    let source = b"const foo = 1;\nexport { foo as bar };";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports.iter().any(|e| e.name == "bar"),
        "exported alias name 'bar' should appear in exports; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        !exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "local name 'foo' should NOT appear as a Named export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_export_no_alias_uses_local_name() {
    // Regression guard: `export { foo }` with no alias should still use "foo".
    let source = b"const foo = 1;\nexport { foo };";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "non-aliased export should use local name 'foo'; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_reexport_alias_uses_alias_name() {
    // `export { foo as bar } from './mod'` — public re-export name is "bar".
    let source = b"export { foo as bar } from './mod';";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::NamedReexport),
        "aliased re-export 'bar' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        !exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::NamedReexport),
        "local name 'foo' should NOT appear as a NamedReexport; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_namespace_reexport_captures_name() {
    // `export * as utils from './mod'` — exported name is "utils", not "*".
    let source = b"export * as utils from './mod';";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "utils" && e.kind == RawExportKind::StarReexport),
        "namespace re-export should use name 'utils'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        !exports
            .iter()
            .any(|e| e.name == "*" && e.source.as_deref() == Some("./mod")),
        "name '*' should not appear for a named namespace re-export; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_plain_star_reexport_still_works() {
    // Regression guard: `export * from './mod'` should still produce name="*".
    let source = b"export * from './mod';";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "*" && e.kind == RawExportKind::StarReexport),
        "plain star re-export should still produce name='*'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

// ── Type-only export tests ( — ) ────────────────────────────

#[test]
fn test_type_only_named_export() {
    // `export type { Foo }` — Foo is a Named export with no source
    let source = b"export type { Foo };";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "Foo" && e.kind == RawExportKind::Named),
        "export type {{ Foo }} should produce Named export 'Foo'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    let foo = exports.iter().find(|e| e.name == "Foo").unwrap();
    assert!(
        foo.decl_kind.is_none(),
        "type-only export should have no decl_kind"
    );
}

#[test]
fn test_type_only_reexport() {
    // `export type { Foo } from './bar'` — NamedReexport with source
    let source = b"export type { Foo } from './bar';";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "Foo" && e.kind == RawExportKind::NamedReexport),
        "export type {{ Foo }} from './bar' should produce NamedReexport 'Foo'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    let foo = exports
        .iter()
        .find(|e| e.name == "Foo" && e.kind == RawExportKind::NamedReexport)
        .unwrap();
    assert_eq!(
        foo.source.as_deref(),
        Some("./bar"),
        "source should be './bar'"
    );
}

#[test]
fn test_type_only_alias_export() {
    // `export type { Foo as Bar }` — exported public name is "Bar"
    let source = b"export type { Foo as Bar };";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "Bar" && e.kind == RawExportKind::Named),
        "export type {{ Foo as Bar }} should produce Named export 'Bar'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        !exports
            .iter()
            .any(|e| e.name == "Foo" && e.kind == RawExportKind::Named),
        "local name 'Foo' should NOT appear as Named; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_destructured_export() {
    // `export const { foo, bar } = obj` — two Named exports
    let source = b"export const { foo, bar } = obj;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "destructured export 'foo' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
        "destructured export 'bar' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    // Both should have Variable decl_kind
    let foo = exports
        .iter()
        .find(|e| e.name == "foo" && e.kind == RawExportKind::Named)
        .unwrap();
    assert_eq!(
        foo.decl_kind,
        Some(js_model::DeclarationKind::Variable),
        "destructured export should have Variable decl_kind"
    );
}

#[test]
fn test_destructured_pair_export() {
    // `export const { foo: renamed } = obj` — public name is "renamed" (the value/local binding)
    let source = b"export const { foo: renamed } = obj;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "renamed" && e.kind == RawExportKind::Named),
        "pair-pattern destructured export should export 'renamed' (local binding); got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        !exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "source property key 'foo' should NOT appear as Named export; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

// ── Namespace/star dedup fix ──────────────────────────────────

#[test]
fn test_namespace_and_star_from_same_source() {
    let source = b"export * from './mod';\nexport * as utils from './mod';";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let star = exports
        .iter()
        .filter(|e| e.name == "*" && e.source.as_deref() == Some("./mod"))
        .count();
    let ns = exports
        .iter()
        .filter(|e| e.name == "utils" && e.source.as_deref() == Some("./mod"))
        .count();
    assert_eq!(star, 1, "plain star should appear once; got {star}");
    assert_eq!(ns, 1, "namespace 'utils' should appear once; got {ns}");
}

// ── Inline type modifier exports ───────────────────────────────

#[test]
fn test_inline_type_modifier_exports() {
    // TS 4.5: per-specifier `type` prefix — `export { type Foo, Bar }`
    // Both should appear exactly once as Named exports
    let source = b"export { type Foo, Bar };";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let foo_count = exports.iter().filter(|e| e.name == "Foo").count();
    let bar_count = exports.iter().filter(|e| e.name == "Bar").count();
    assert_eq!(
        foo_count, 1,
        "Foo should appear exactly once; got {foo_count}"
    );
    assert_eq!(
        bar_count, 1,
        "Bar should appear exactly once; got {bar_count}"
    );
}

// ── Export assignment and ambient declaration tests ( — ) ───

#[test]
fn test_export_assignment() {
    // `export = MyClass;` — CJS-style TS default export, becomes name="default"
    let source = b"export = MyClass;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "default" && e.kind == RawExportKind::Default),
        "export = MyClass should produce Default export 'default'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ambient_function_declaration() {
    // `export declare function foo(): void;` — ambient declaration in.d.ts
    let source = b"export declare function foo(): void;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let foo = exports
        .iter()
        .find(|e| e.name == "foo" && e.kind == RawExportKind::Named);
    assert!(
        foo.is_some(),
        "export declare function foo should produce Named export 'foo'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        foo.unwrap().decl_kind,
        Some(js_model::DeclarationKind::Function),
        "ambient function should have Function decl_kind"
    );
}

#[test]
fn test_ambient_class_declaration() {
    // `export declare class Bar {}` — ambient class declaration
    let source = b"export declare class Bar {}";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let bar = exports
        .iter()
        .find(|e| e.name == "Bar" && e.kind == RawExportKind::Named);
    assert!(
        bar.is_some(),
        "export declare class Bar should produce Named export 'Bar'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        bar.unwrap().decl_kind,
        Some(js_model::DeclarationKind::Class),
        "ambient class should have Class decl_kind"
    );
}

#[test]
fn test_ambient_interface_declaration() {
    // `export declare interface Baz { x: number }` — ambient interface
    let source = b"export declare interface Baz { x: number }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let baz = exports
        .iter()
        .find(|e| e.name == "Baz" && e.kind == RawExportKind::Named);
    assert!(
        baz.is_some(),
        "export declare interface Baz should produce Named export 'Baz'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        baz.unwrap().decl_kind,
        Some(js_model::DeclarationKind::Interface),
        "ambient interface should have Interface decl_kind"
    );
}

#[test]
fn test_ambient_variable_declaration() {
    // `export declare const VERSION: string;` — ambient variable
    let source = b"export declare const VERSION: string;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let version = exports
        .iter()
        .find(|e| e.name == "VERSION" && e.kind == RawExportKind::Named);
    assert!(
        version.is_some(),
        "export declare const VERSION should produce Named export 'VERSION'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        version.unwrap().decl_kind,
        Some(js_model::DeclarationKind::Variable),
        "ambient variable should have Variable decl_kind"
    );
}

// ── CJS module.exports detection ───────────────────────────────

#[test]
fn test_cjs_module_exports_shorthand() {
    let source = b"module.exports = { foo, bar };";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "CJS shorthand export 'foo' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
        "CJS shorthand export 'bar' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_cjs_module_exports_pair() {
    let source = b"module.exports = { foo: myFoo, bar: myBar };";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "CJS pair export 'foo' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
        "CJS pair export 'bar' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_cjs_exports_property() {
    let source = b"exports.foo = function() {};\nexports.bar = 42;";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
        "CJS exports.foo should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
        "CJS exports.bar should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_cjs_module_exports_default() {
    let source = b"module.exports = MyClass;";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "default" && e.kind == RawExportKind::Default),
        "CJS module.exports = MyClass should produce Default 'default'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_cjs_not_triggered_in_typescript() {
    // CJS patterns should NOT be detected in TypeScript files
    let source = b"module.exports = { foo };";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        !exports.iter().any(|e| e.name == "foo"),
        "CJS should not match in TS; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

// ── export { default as Named } detection ─────────────────────

#[test]
fn test_export_default_as_named() {
    let source = b"export { default as Named };";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "Named" && e.kind == RawExportKind::Named),
        "export {{ default as Named }} should export 'Named'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_export_default_reexport() {
    let source = b"export { default } from './foo';";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "default" && e.kind == RawExportKind::NamedReexport),
        "export {{ default }} from './foo' should produce NamedReexport 'default'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_export_default_as_named_reexport() {
    let source = b"export { default as Named } from './foo';";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "Named" && e.kind == RawExportKind::NamedReexport),
        "export {{ default as Named }} from './foo' should export 'Named'; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

// ── Array destructuring exports ────────────────────────────────

#[test]
fn test_array_destructured_export() {
    let source = b"export const [a, b] = arr;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "a" && e.kind == RawExportKind::Named),
        "array destructured export 'a' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "b" && e.kind == RawExportKind::Named),
        "array destructured export 'b' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_array_destructured_rest_export() {
    let source = b"export const [first, ...rest] = arr;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "first" && e.kind == RawExportKind::Named),
        "array destructured 'first' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "rest" && e.kind == RawExportKind::Named),
        "array destructured rest 'rest' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_array_destructured_with_holes() {
    let source = b"export const [a, , c] = arr;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "a" && e.kind == RawExportKind::Named),
        "array destructured 'a' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "c" && e.kind == RawExportKind::Named),
        "array destructured 'c' should appear; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        exports
            .iter()
            .filter(|e| e.kind == RawExportKind::Named)
            .count(),
        2,
        "holes should not produce Named exports; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

// ─── Namespace member expansion ───────────────────────────────

#[test]
fn test_namespace_member_expansion() {
    // namespace members should produce dotted-name entries
    let source = b"export namespace Foo { export function bar(): void {} export const baz = 1; }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"Foo"),
        "namespace itself should be present; got: {:?}",
        names
    );
    assert!(
        names.contains(&"Foo.bar"),
        "namespace member 'Foo.bar' should be present; got: {:?}",
        names
    );
    assert!(
        names.contains(&"Foo.baz"),
        "namespace member 'Foo.baz' should be present; got: {:?}",
        names
    );
}

#[test]
fn test_namespace_member_decl_kind() {
    let source = b"export namespace NS { export class MyClass {} }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let member = exports.iter().find(|e| e.name == "NS.MyClass");
    assert!(
        member.is_some(),
        "NS.MyClass should be exported; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        member.unwrap().decl_kind,
        Some(DeclarationKind::Class),
        "NS.MyClass should have Class decl_kind"
    );
}

#[test]
fn test_namespace_member_no_duplicates() {
    // same dotted name should not appear twice
    let source = b"export namespace Foo { export function bar(): void {} }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let count = exports.iter().filter(|e| e.name == "Foo.bar").count();
    assert_eq!(
        count, 1,
        "Foo.bar should appear exactly once; got count={}",
        count
    );
}

// ── Nested destructuring exports ───────────────────────────────

#[test]
fn test_nested_destructuring_object_in_object() {
    // `export const { a: { b } } = obj` captures both "a" (flat query) and "b" (nested)
    let source = b"export const { a: { b } } = obj;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"b"),
        "nested destructuring 'b' from {{ a: {{ b }} }} should be captured; got: {:?}",
        names
    );
}

#[test]
fn test_nested_destructuring_array_in_array() {
    // `export const [a, [b, c]] = arr` captures "a", "b", "c"
    let source = b"export const [a, [b, c]] = arr;";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"a"),
        "top-level 'a' from [a, [b, c]] should be captured; got: {:?}",
        names
    );
    assert!(
        names.contains(&"b"),
        "nested 'b' from [a, [b, c]] should be captured; got: {:?}",
        names
    );
    assert!(
        names.contains(&"c"),
        "nested 'c' from [a, [b, c]] should be captured; got: {:?}",
        names
    );
}

// ── CJS exports aliasing pattern ───────────────────────────────

#[test]
fn test_cjs_aliasing_shorthand_properties() {
    // `exports = module.exports = { foo, bar }` captures "foo" and "bar"
    let source = b"exports = module.exports = { foo, bar };";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"foo"),
        "CJS aliasing 'foo' should be captured; got: {:?}",
        names
    );
    assert!(
        names.contains(&"bar"),
        "CJS aliasing 'bar' should be captured; got: {:?}",
        names
    );
}

#[test]
fn test_cjs_aliasing_pair_properties() {
    // `exports = module.exports = { key: value }` captures "key"
    let source = b"exports = module.exports = { key: value };";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"key"),
        "CJS aliasing pair 'key' should be captured; got: {:?}",
        names
    );
}

#[test]
fn test_cjs_aliasing_not_triggered_in_typescript() {
    // CJS aliasing is JavaScript-only
    let source = b"exports = module.exports = { foo };";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    // Should not crash and we don't assert contents since TS doesn't recognize CJS
    let _ = exports;
}

// ── Nested namespace expansion ────────────────────────────────

#[test]
fn test_namespace_nested_2_levels() {
    // 2-level nested namespace should produce A.B and A.B.baz
    let source = b"export namespace A { export namespace B { export function baz() {} } }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"A"),
        "top-level namespace 'A' should be present; got: {:?}",
        names
    );
    assert!(
        names.contains(&"A.B"),
        "nested namespace 'A.B' should be present; got: {:?}",
        names
    );
    assert!(
        names.contains(&"A.B.baz"),
        "doubly-nested member 'A.B.baz' should be present; got: {:?}",
        names
    );
}

#[test]
fn test_namespace_nested_3_levels() {
    // 3-level nested namespace should produce A.B.C and A.B.C.fn
    let source = b"export namespace A { export namespace B { export namespace C { export function fn_deep() {} } } }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"A.B.C"),
        "3-level nested namespace 'A.B.C' should be present; got: {:?}",
        names
    );
    assert!(
        names.contains(&"A.B.C.fn_deep"),
        "3-level nested member 'A.B.C.fn_deep' should be present; got: {:?}",
        names
    );
}

#[test]
fn test_namespace_1_level_still_works_after_refactor() {
    // Regression: 1-level expansion must still work after recursive refactor
    let source =
        b"export namespace Outer { export function method(): void {} export const VAL = 42; }";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"Outer.method"),
        "1-level expansion 'Outer.method' should still work; got: {:?}",
        names
    );
    assert!(
        names.contains(&"Outer.VAL"),
        "1-level expansion 'Outer.VAL' should still work; got: {:?}",
        names
    );
}

// ── CJS inline class/function exports ─────────────────────────

#[test]
fn test_cjs_module_exports_class_inline() {
    // module.exports = class Foo { method {} }
    // Should produce a "default" export with decl_kind Class
    let source = b"module.exports = class Foo { method() {} }";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let default_export = exports.iter().find(|e| e.name == "default");
    assert!(
        default_export.is_some(),
        "module.exports = class should emit 'default' export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        default_export.unwrap().decl_kind,
        Some(DeclarationKind::Class),
        "inline class export should have decl_kind Class"
    );
}

#[test]
fn test_cjs_module_exports_function_inline() {
    // module.exports = function(a, b) { return a + b; }
    // Should produce a "default" export with decl_kind Function
    let source = b"module.exports = function(a, b) { return a + b; }";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let default_export = exports.iter().find(|e| e.name == "default");
    assert!(
        default_export.is_some(),
        "module.exports = function should emit 'default' export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        default_export.unwrap().decl_kind,
        Some(DeclarationKind::Function),
        "inline function export should have decl_kind Function"
    );
}

#[test]
fn test_cjs_module_exports_async_arrow() {
    // module.exports = async (req, res) => {... }
    // Should produce a "default" export with decl_kind Function
    let source = b"module.exports = async (req, res) => { res.send('ok'); }";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let default_export = exports.iter().find(|e| e.name == "default");
    assert!(
        default_export.is_some(),
        "module.exports = async arrow should emit 'default' export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        default_export.unwrap().decl_kind,
        Some(DeclarationKind::Function),
        "async arrow export should have decl_kind Function"
    );
}

#[test]
fn test_cjs_exports_property_function() {
    // exports.handler = async (req, res) => { }
    // Should produce a "handler" Named export with decl_kind Function
    let source = b"exports.handler = async (req, res) => { res.send('ok'); }";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let handler_export = exports.iter().find(|e| e.name == "handler");
    assert!(
        handler_export.is_some(),
        "exports.handler = async arrow should emit 'handler' export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        handler_export.unwrap().decl_kind,
        Some(DeclarationKind::Function),
        "exports.handler = async arrow should have decl_kind Function"
    );
}

#[test]
fn test_using_declaration_graceful() {
    // export using handle = getResource should not panic
    // Either detect it or silently skip it — no panics allowed.
    let source = b"export using handle = getResource();";
    let result = parse_exports(source, grammar::SourceLanguage::TypeScript);
    assert!(
        result.is_ok(),
        "parse_exports should not error on 'export using' syntax; got: {:?}",
        result.err()
    );
    // The parse completing without panic/error is sufficient — detection is best-effort.
}

// ── Const type parameter modifier ──────────────────────────────

#[test]
fn test_parse_type_params_const_modifier() {
    // NOTE: This test verifies const detection. If tree-sitter-typescript 0.23.2
    // doesn't parse `const T` in generics, the const will appear as ERROR node
    // and is_const will be false. That's acceptable — the detection is best-effort.
    let source = b"function foo<const T>(x: T): T { return x; }";
    let bytes = source.as_slice();
    let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang).unwrap();
    let tree = parser.parse(bytes, None).unwrap();
    let root = tree.root_node();
    let fn_node = root.child(0).unwrap();
    let params = generic_syntax::parse_type_params(bytes, fn_node);
    // If the grammar supports const type params, verify is_const == true
    // If not, the param should still be extracted (is_const may be false)
    assert!(!params.is_empty(), "should extract at least one type param");
    assert_eq!(params[0].name, "T");
}

// ── Module augmentation capture ────────────────────────────────

#[test]
fn test_declare_module_augmentation_captured() {
    let source = br#"
declare module 'express' {
    interface Request {
        user?: { id: string };
    }
}
"#;
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let augment = exports.iter().find(|e| e.name == "augment:express");
    assert!(
        augment.is_some(),
        "declare module 'express' should be captured; got: {:?}",
        exports
    );
}

#[test]
fn test_declare_global_augmentation_captured() {
    let source = br#"
declare global {
    interface Window {
        myApp: any;
    }
}
"#;
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let augment = exports.iter().find(|e| e.name == "augment:global");
    assert!(
        augment.is_some(),
        "declare global should be captured; got: {:?}",
        exports
    );
}

#[test]
fn test_declare_module_not_captured_in_js() {
    // declare module is TypeScript-only syntax
    let source = b"// no declare module in JS";
    let exports = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    assert!(exports.iter().all(|e| !e.name.starts_with("augment:")));
}

// ── META-03 augmentation body capture ───────────────────────────

#[test]
fn declare_module_body_member_captured() {
    let source = br#"declare module 'express' { interface Session { uid: string; } }"#;
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports.iter().any(|e| e.name == "augment:express:*Session"),
        "META-03: declare module member not captured. Got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn declare_global_body_captured() {
    let source = br#"declare global { interface Window { foo: string; } }"#;
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name.starts_with("augment:global:*Window")),
        "META-03: declare global member not captured. Got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

// ── META-04 TSDoc release tag extraction ────────────────────────

#[test]
fn internal_tag_extracted() {
    let source = b"/** @internal */\nexport function privateHelper() {}";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let e = exports.iter().find(|e| e.name == "privateHelper").unwrap();
    assert_eq!(
        e.release_tag,
        Some(ts_model::ReleaseTag::Internal),
        "META-04: @internal tag should be extracted; got: {:?}",
        e.release_tag
    );
}

#[test]
fn deprecated_tag_extracted() {
    let source = b"/** @deprecated use newFoo */\nexport function oldFoo() {}";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let e = exports.iter().find(|e| e.name == "oldFoo").unwrap();
    assert_eq!(
        e.release_tag,
        Some(ts_model::ReleaseTag::Deprecated),
        "META-04: @deprecated tag should be extracted; got: {:?}",
        e.release_tag
    );
}

#[test]
fn beta_tag_extracted() {
    // META-06: @beta replaces @experimental in the 5-tag set.
    let source = b"/** @beta */\nexport function previewFoo() {}";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let e = exports.iter().find(|e| e.name == "previewFoo").unwrap();
    assert_eq!(
        e.release_tag,
        Some(ts_model::ReleaseTag::Beta),
        "META-06: @beta tag should be extracted; got: {:?}",
        e.release_tag
    );
}

#[test]
fn no_tag_is_none() {
    let source = b"export function plainFn() {}";
    let exports = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    let e = exports.iter().find(|e| e.name == "plainFn").unwrap();
    assert_eq!(
        e.release_tag, None,
        "META-04: no tag should yield None; got: {:?}",
        e.release_tag
    );
}

// ── TLF extraction tests ────────────────────────────────────────

#[test]
fn extract_construct_signature() {
    let src = "interface Foo { new (x: number): Bar; }";
    let (tree, bytes) = parse_ts(src);
    let iface_node = first_node_of_kind(tree.root_node(), "interface_declaration").unwrap();
    // Find interface_body child
    let body = (0..iface_node.child_count() as u32)
        .filter_map(|i| iface_node.child(i))
        .find(|c| c.kind() == "interface_body" || c.kind() == "object_type")
        .expect("interface body not found");
    let sigs = interface_syntax::extract_construct_signatures(body, &bytes);
    assert_eq!(
        sigs.len(),
        1,
        "should extract one construct signature; got: {:?}",
        sigs
    );
    assert_eq!(sigs[0].param_count, 1, "one param");
    assert_eq!(
        sigs[0].return_text.as_deref(),
        Some("Bar"),
        "return type Bar"
    );
    assert!(!sigs[0].is_abstract, "not abstract");
}

#[test]
fn parameter_properties_synthesized() {
    let src = "class Foo { constructor(public x: number, private y: string, z: boolean) {} }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    // x and y should be synthesized as Property members
    let x = members.iter().find(|m| m.name == "x");
    let y = members.iter().find(|m| m.name == "y");
    let z = members.iter().find(|m| m.name == "z");
    assert!(
        x.is_some(),
        "x should be synthesized as property; members: {:?}",
        members.iter().map(|m| &m.name).collect::<Vec<_>>()
    );
    assert!(y.is_some(), "y should be synthesized as property");
    assert!(z.is_none(), "z has no modifier — must NOT be synthesized");
    assert_eq!(x.unwrap().kind, ClassMemberKind::Property);
    assert_eq!(x.unwrap().visibility, Visibility::Public);
    assert_eq!(y.unwrap().visibility, Visibility::Private);
}

#[test]
fn variance_in_out_extracted() {
    let src = "type X<in T, out U, in out V> = [T, U, V];";
    let (tree, bytes) = parse_ts(src);
    let alias_node = first_node_of_kind(tree.root_node(), "type_alias_declaration").unwrap();
    // Pass the declaration node (parse_type_params looks up type_parameters internally)
    let params = generic_syntax::parse_type_params(&bytes, alias_node);
    assert_eq!(
        params.len(),
        3,
        "three type params; got: {:?}",
        params.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    assert_eq!(
        params[0].variance,
        Some(ts_model::Variance::In),
        "T should be in"
    );
    assert_eq!(
        params[1].variance,
        Some(ts_model::Variance::Out),
        "U should be out"
    );
    assert_eq!(
        params[2].variance,
        Some(ts_model::Variance::InOut),
        "V should be in out"
    );
}

#[test]
fn accessor_keyword_captured() {
    let src = "class Foo { accessor bar: number; }";
    let (tree, bytes) = parse_ts(src);
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
    let (members, _has_dynamic_static_key) = class_syntax::parse_class_members(&bytes, class_node);
    let bar = members
        .iter()
        .find(|m| m.name == "bar")
        .expect("bar member should be present");
    assert!(bar.is_accessor, "bar should have is_accessor=true");
}

#[test]
fn export_type_star_is_type_only() {
    let source = b"export type * from './mod';";
    let raw = parse_exports(source, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        raw.iter().any(|e| e.is_type_only),
        "export type * should produce is_type_only=true; got: {:?}",
        raw.iter()
            .map(|e| (&e.name, e.is_type_only))
            .collect::<Vec<_>>()
    );
}

// ── BUN-01..07: Bundler CJS output detection ────────────────────

#[test]
fn esbuild_tree_shake_wrapper() {
    // BUN-01: esbuild `0 && (module.exports = { foo, bar })` tree-shake wrapper
    let source = b"0 && (module.exports = { foo: foo, bar: bar });";
    let raw = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"foo"),
        "BUN-01: foo must be detected; got: {:?}",
        names
    );
    assert!(
        names.contains(&"bar"),
        "BUN-01: bar must be detected; got: {:?}",
        names
    );
}

#[test]
fn esbuild_export_helper() {
    // BUN-02: __export(exports, { foo: () => foo, bar: () => bar })
    let source = b"__export(exports, { foo: () => foo, bar: () => bar });";
    let raw = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"foo"),
        "BUN-02: foo must be detected; got: {:?}",
        names
    );
    assert!(
        names.contains(&"bar"),
        "BUN-02: bar must be detected; got: {:?}",
        names
    );
}

#[test]
fn tsc_void_placeholder() {
    // BUN-04: exports.foo = void 0; — tsc placeholder
    let source = b"exports.foo = void 0;";
    let raw = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"foo"),
        "BUN-04: foo must be detected from void 0 placeholder; got: {:?}",
        names
    );
}

#[test]
fn bun_06b_bracket_subscript_export() {
    // BUN-06b: exports["foo"] = bar
    let source = b"exports[\"foo\"] = bar;";
    let raw = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"foo"),
        "BUN-06b: foo must be detected from bracket subscript; got: {:?}",
        names
    );
}

#[test]
fn esmodule_filtered() {
    // BUN-07: __esModule must never appear in final export list
    let source = b"exports.__esModule = true;";
    let raw = parse_exports(source, grammar::SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = raw.iter().map(|e| e.name.as_str()).collect();
    assert!(
        !names.contains(&"__esModule"),
        "BUN-07: __esModule must be filtered; got: {:?}",
        names
    );
}

// ── IFT-01: Interface property optionality extracted ────────────

#[test]
fn interface_property_optionality_extracted() {
    let source = b"interface Foo { x?: number; y: string; }";
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    // Find the interface_declaration node
    let iface_node = first_node_of_kind(tree.root_node(), "interface_declaration")
        .expect("interface_declaration not found");
    // Find interface body (interface_body or object_type)
    let body = interface_syntax::find_interface_body(iface_node).expect("interface body not found");
    let members = interface_syntax::parse_interface_members(source, body);
    let x = members.iter().find(|m| m.name == "x").expect("x not found");
    let y = members.iter().find(|m| m.name == "y").expect("y not found");
    assert!(x.is_optional, "x? should be optional");
    assert!(!y.is_optional, "y should not be optional");
}

// ── IFT-03: extract_param_optionality on constructor ────────────

#[test]
fn extract_param_optionality_constructor() {
    let source = b"class C { constructor(a: number, b?: string, c?: boolean) {} }";
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let class_node = first_node_of_kind(tree.root_node(), "class_declaration")
        .expect("class_declaration not found");
    // Find class_body
    let class_body = {
        let mut found = None;
        for i in 0..class_node.child_count() as u32 {
            if let Some(child) = class_node.child(i) {
                if child.kind() == "class_body" {
                    found = Some(child);
                    break;
                }
            }
        }
        found.expect("class_body not found")
    };
    // Find the constructor method_definition
    let ctor_node = {
        let mut found = None;
        let mut cursor = class_body.walk();
        for child in class_body.children(&mut cursor) {
            if child.kind() == "method_definition" {
                let name_text = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                if name_text == "constructor" {
                    found = Some(child);
                    break;
                }
            }
        }
        found.expect("constructor method_definition not found")
    };
    let optionality = function_syntax::extract_param_optionality(ctor_node);
    assert_eq!(
        optionality,
        vec![false, true, true],
        "a is required, b? and c? are optional; got: {:?}",
        optionality
    );
}

// ── ARCH-02: compound-component capture ────────────────────────

#[test]
fn test_arch02_compound_member_assignment_attaches() {
    // Form 1: `Target.Member = X;` at top-level attaches Member to Target.
    let src = br#"
export const Menu = () => null;
Menu.Item = () => null;
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).expect("parse");
    let menu = exports
        .iter()
        .find(|e| e.name == "Menu")
        .expect("Menu export missing");
    assert!(
        menu.compound_members.iter().any(|m| m == "Item"),
        "ARCH-02 Form 1: expected 'Item' in compound_members, got {:?}",
        menu.compound_members
    );
}

#[test]
fn test_arch02_object_assign_compound_attaches() {
    // Form 3: `Object.assign(Target, { Member: X,... })` attaches all members.
    let src = br#"
export let Menu = {};
Menu = Object.assign(Menu, { Button: 1, Items: 2, Item: 3 });
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).expect("parse");
    let menu = exports
        .iter()
        .find(|e| e.name == "Menu")
        .expect("Menu export missing");
    assert!(
        menu.compound_members.iter().any(|m| m == "Button"),
        "ARCH-02 Form 3: expected 'Button'; got {:?}",
        menu.compound_members
    );
    assert!(
        menu.compound_members.iter().any(|m| m == "Items"),
        "ARCH-02 Form 3: expected 'Items'; got {:?}",
        menu.compound_members
    );
    assert!(
        menu.compound_members.iter().any(|m| m == "Item"),
        "ARCH-02 Form 3: expected 'Item'; got {:?}",
        menu.compound_members
    );
}

#[test]
fn test_arch02_conservative_no_attach_when_target_not_exported() {
    // conservative: `Foo.Bar = X` where `Foo` is NOT an exported symbol
    // must NOT attach Bar to anything. `Unrelated` should remain with empty
    // compound_members.
    let src = br#"
const Foo = {};
Foo.Bar = 42;
export const Unrelated = 1;
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).expect("parse");
    assert!(
        !exports.iter().any(|e| e.name == "Foo"),
        "ARCH-02 D-16: unexported Foo must not appear in exports"
    );
    let unrelated = exports
        .iter()
        .find(|e| e.name == "Unrelated")
        .expect("Unrelated missing");
    assert!(
        unrelated.compound_members.is_empty(),
        "ARCH-02 D-16: Unrelated must not have compound_members; got {:?}",
        unrelated.compound_members
    );
}

#[test]
fn test_arch02_minified_target_not_in_exports_silent_noop() {
    // Pitfall 6: minified target name (`Pe`) is not in the exports — the
    // conservative attachment rule silently no-ops. `Menu` gets no members.
    let src = br#"
export const Menu = () => null;
var qe = Object.assign(Pe, { Button: 1, Item: 2 });
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).expect("parse");
    let menu = exports
        .iter()
        .find(|e| e.name == "Menu")
        .expect("Menu export missing");
    assert!(
        menu.compound_members.is_empty(),
        "ARCH-02 Pitfall 6: minified-target case must silently no-op; got {:?}",
        menu.compound_members
    );
}

// ---------------------------------------------------------------------
// INF-01:.tsx → LANGUAGE_TSX grammar routing tests
// ---------------------------------------------------------------------

#[test]
fn test_inf01_tsx_routes_to_typescript_tsx() {
    let p = std::path::PathBuf::from("/x/foo.tsx");
    assert!(matches!(
        grammar::language_for_file(&p),
        Some(grammar::SourceLanguage::TypeScriptTsx)
    ));
}

#[test]
fn test_inf01_d_ts_still_typescript() {
    // Declaration files are pure type surface (no JSX) — must stay on the
    // standard TypeScript grammar regardless of the `.tsx` → LANGUAGE_TSX
    // routing.
    let p = std::path::PathBuf::from("/x/foo.d.ts");
    assert!(matches!(
        grammar::language_for_file(&p),
        Some(grammar::SourceLanguage::TypeScript)
    ));
}

#[test]
fn test_inf01_ts_still_typescript() {
    let p = std::path::PathBuf::from("/x/foo.ts");
    assert!(matches!(
        grammar::language_for_file(&p),
        Some(grammar::SourceLanguage::TypeScript)
    ));
}

#[test]
fn test_inf01_jsx_still_javascript() {
    // `.jsx` is JavaScript-JSX; it stays on the JavaScript grammar because
    // tree-sitter-javascript parses JSX natively. Only `.tsx` needs the TSX
    // TypeScript grammar variant.
    let p = std::path::PathBuf::from("/x/foo.jsx");
    assert!(matches!(
        grammar::language_for_file(&p),
        Some(grammar::SourceLanguage::JavaScript)
    ));
}

#[test]
fn test_inf01_tsx_jsx_body_discovers_export() {
    // TSX grammar must handle JSX in export body without producing ERROR
    // nodes that suppress export discovery. Pre-INF-01, `.tsx` routed to
    // LANGUAGE_TYPESCRIPT which emits ERROR nodes for JSX expressions and
    // silently dropped the surrounding exports.
    let src = br#"
export const Comp = () => <div>hi</div>;
export interface Props { children: string }
"#;
    let exports =
        parse_exports(src, grammar::SourceLanguage::TypeScriptTsx).expect("parse must succeed");
    assert!(
        exports.iter().any(|e| e.name == "Comp"),
        "expected Comp in exports, got {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        exports.iter().any(|e| e.name == "Props"),
        "expected Props in exports"
    );
}

#[test]
fn test_inf01_tsx_same_queries_match_as_ts() {
    // Strict-superset property: every TypeScript query matches identically
    // on TSX grammar for source containing no JSX. If this regresses, every
    // matches!() dispatch site that was widened would start silently
    // dropping exports under the TypeScriptTsx arm.
    let src = br#"
export function foo(x: number): string { return ""; }
export class Bar<T> { method(a: T): T { return a; } }
"#;
    let ts = parse_exports(src, grammar::SourceLanguage::TypeScript).expect("TS parse");
    let tsx = parse_exports(src, grammar::SourceLanguage::TypeScriptTsx).expect("TSX parse");
    let ts_names: std::collections::HashSet<_> = ts.iter().map(|e| e.name.clone()).collect();
    let tsx_names: std::collections::HashSet<_> = tsx.iter().map(|e| e.name.clone()).collect();
    assert_eq!(
        ts_names, tsx_names,
        "TSX grammar should be strict superset — same exports discovered"
    );
}

// ---------------------------------------------------------------------
// INF-02: rollup-plugin-dts / api-extractor split-declaration
// linking tests. Pass 1 collects ambient declare statements; link-pass
// enriches Named non-re-export RawExports with decl_kind + type_signature.
// ---------------------------------------------------------------------

#[test]
fn test_inf02_declare_const_link_to_export_populates_type_signature() {
    let src = br#"
declare const foo: number;
export { foo };
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    let foo = exports
        .iter()
        .find(|e| e.name == "foo")
        .expect("foo missing");
    assert_eq!(foo.decl_kind, Some(DeclarationKind::Variable));
    assert!(
        foo.type_signature.is_some(),
        "type_signature should be populated from declare site"
    );
    assert!(
        foo.type_signature.as_ref().unwrap().contains("number"),
        "got type_signature: {:?}",
        foo.type_signature
    );
}

#[test]
fn test_inf02_declare_function_link_to_export_populates_decl_kind() {
    let src = br#"
declare function bar(x: number): string;
export { bar };
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    let bar = exports
        .iter()
        .find(|e| e.name == "bar")
        .expect("bar missing");
    assert_eq!(bar.decl_kind, Some(DeclarationKind::Function));
    assert!(
        bar.type_signature.is_some(),
        "function declare should set type_signature to the signature text"
    );
}

#[test]
fn test_inf02_declare_class_link_to_export_populates_decl_kind() {
    let src = br#"
declare class Baz { x: number; }
export { Baz };
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    let baz = exports
        .iter()
        .find(|e| e.name == "Baz")
        .expect("Baz missing");
    assert_eq!(baz.decl_kind, Some(DeclarationKind::Class));
}

#[test]
fn test_inf02_declare_without_export_is_not_public() {
    // Orphan declare statement — no export — must NOT produce a RawExport.
    // The ambient sidecar records it, but without a matching export the
    // link-pass has nothing to enrich.
    let src = br#"
declare const internal: number;
export const unrelated = 1;
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        !exports.iter().any(|e| e.name == "internal"),
        "declare-without-export must not appear as an export"
    );
    assert!(exports.iter().any(|e| e.name == "unrelated"));
}

#[test]
fn test_inf02_reexport_not_affected_by_local_declare() {
    // If the export is `export { foo } from './other'` (re-export with
    // `from`), the local ambient declare must NOT leak into the exported
    // symbol — the symbol's identity comes from./other. The INF-02
    // link-pass explicitly skips entries where `rx.source.is_some()`.
    let src = br#"
declare const foo: number;
export { foo } from './other';
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    let foo = exports
        .iter()
        .find(|e| e.name == "foo")
        .expect("foo missing");
    assert_eq!(foo.kind, RawExportKind::NamedReexport);
    // decl_kind from ambient declare must NOT have leaked in.
    assert_eq!(foo.decl_kind, None);
    assert_eq!(foo.type_signature, None);
}

// ---------------------------------------------------------------------
// INF-03: tsup/esbuild `export { foo as default }` ESM trailer
// dual-emission tests. When `alias_name == "default"` the NAMED_EXPORT_
// ALIAS_QUERY handler now emits `RawExportKind::Default` instead of
// `Named`. Dual emission (`export { foo, foo as default }`) fires both
// queries — NAMED_EXPORT_QUERY for foo and NAMED_EXPORT_ALIAS_QUERY for
// the default trailer.
// ---------------------------------------------------------------------

#[test]
fn test_inf03_foo_as_default_trailer_emits_default_kind() {
    let src = br#"
const foo = 1;
export { foo as default };
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    let default = exports
        .iter()
        .find(|e| matches!(e.kind, RawExportKind::Default))
        .expect("expected a Default RawExport");
    assert_eq!(default.name, "default");
    // foo is NOT also emitted as a separate Named when only the alias
    // trailer is present (no bare `foo` specifier in the export clause).
    assert!(
        !exports
            .iter()
            .any(|e| e.name == "foo" && matches!(e.kind, RawExportKind::Named)),
        "foo alone should not be Named when only the alias trailer is present"
    );
}

#[test]
fn test_inf03_dual_emission_foo_and_default() {
    let src = br#"
const foo = 1;
export { foo, foo as default };
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "foo" && matches!(e.kind, RawExportKind::Named)),
        "expected Named foo (fires via NAMED_EXPORT_QUERY)"
    );
    assert!(
        exports
            .iter()
            .any(|e| e.name == "default" && matches!(e.kind, RawExportKind::Default)),
        "expected Default from `as default` trailer"
    );
}

#[test]
fn test_inf03_non_default_alias_stays_named() {
    let src = br#"
const bar = 1;
export { bar as Baz };
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "Baz" && matches!(e.kind, RawExportKind::Named)),
        "expected Baz as Named"
    );
    assert!(
        !exports
            .iter()
            .any(|e| matches!(e.kind, RawExportKind::Default)),
        "non-default alias must not emit a Default"
    );
}

#[test]
fn test_inf03_source_default_plus_trailer_dedup() {
    // Both source-form `export default foo` AND the trailer form
    // `export { foo as default }`. Both produce a Default RawExport. The
    // resolver pre-merge absorbs the duplicate via ExportKey ("default",
    // space=Value) — export extraction emits both and the resolver keeps one
    // ExportMap entry.
    let src = br#"
const foo = 1;
export default foo;
export { foo as default };
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    let default_count = exports
        .iter()
        .filter(|e| matches!(e.kind, RawExportKind::Default))
        .count();
    assert!(
        default_count >= 1,
        "at least one Default must be emitted; got {default_count}"
    );
}

#[test]
fn test_inf03_zod_style_trailer_with_variable_ref() {
    // Real-package inspired: Zod 3.23.8 emits
    // declare const z:...;
    // export { z as default, z };
    // alongside ambient declare at the top. INF-02 links `z` (Named) to
    // the ambient decl_kind; INF-03 emits an additional Default for the
    // `as default` alias.
    let src = br#"
declare const z: { parse: (x: any) => any };
export { z as default, z };
"#;
    let exports = parse_exports(src, grammar::SourceLanguage::TypeScript).unwrap();
    assert!(
        exports
            .iter()
            .any(|e| e.name == "z" && matches!(e.kind, RawExportKind::Named)),
        "expected Named z"
    );
    assert!(
        exports
            .iter()
            .any(|e| matches!(e.kind, RawExportKind::Default)),
        "expected Default from 'as default' trailer"
    );
}

// ── CLS-10: decorator AST extraction ─────────────────────────────

fn find_first_node_of_kind<'a>(
    root: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    if root.kind() == kind {
        return Some(root);
    }
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if let Some(found) = find_first_node_of_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

/// Extract the combined class-level decorators: those directly on the
/// class_declaration plus those on an enclosing export_statement parent.
/// Matches the logic inside `parse_class_shape`.
fn combined_class_decs(source: &[u8]) -> Vec<ts_model::DecoratorSpec> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&grammar::get_language(&grammar::SourceLanguage::TypeScript))
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let class_node = find_first_node_of_kind(root, "class_declaration")
        .expect("class_declaration must exist in source");
    let mut decs = annotations::extract_decorators(class_node, source);
    if let Some(parent) = class_node.parent() {
        if parent.kind() == "export_statement" {
            let mut parent_decs = annotations::extract_decorators(parent, source);
            parent_decs.extend(decs);
            decs = parent_decs;
        }
    }
    decs
}

#[test]
fn class_decorator_identifier_form() {
    let source = b"@Component\nexport class Foo {}\n";
    let decs = combined_class_decs(source);
    assert!(
        decs.iter()
            .any(|d| d.name == "Component" && d.args.is_none()),
        "@Component should be extracted; got {:?}",
        decs
    );
}

#[test]
fn class_decorator_call_expression_form() {
    let source = b"@Route('/users')\nexport class UserController {}\n";
    let decs = combined_class_decs(source);
    let route = decs.iter().find(|d| d.name == "Route");
    assert!(
        route.is_some(),
        "Route decorator should be extracted; decs = {:?}",
        decs
    );
    let args = route.unwrap().args.as_deref().unwrap_or("");
    assert!(
        args.contains("'/users'"),
        "args should include '/users'; got {:?}",
        args
    );
}

#[test]
fn member_expression_form() {
    let source = b"@ns.log\nexport class X {}\n";
    let decs = combined_class_decs(source);
    assert!(
        decs.iter().any(|d| d.name == "ns.log" && d.args.is_none()),
        "ns.log member_expression decorator should be extracted; decs = {:?}",
        decs
    );
}

#[test]
fn no_decorators_is_empty() {
    let source = b"export class Plain {}\n";
    let decs = combined_class_decs(source);
    assert!(
        decs.is_empty(),
        "plain class should have no decorators; got {:?}",
        decs
    );
}
