use super::*;
use crate::languages::javascript::model as js_model;
use crate::languages::javascript::syntax::grammar::SourceLanguage;
#[test]
fn test_ambient_class_declaration_found() {
    // export declare class Foo should be found by extract_declaration
    let source = b"export declare class Foo { bar(): void; }";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find ambient class Foo"
    );
    let decl = result.unwrap();
    assert!(decl.is_class, "declaration should be identified as a class");
    assert!(
        decl.class_shape.is_some(),
        "class shape should be extracted"
    );
}
#[test]
fn test_ambient_function_declaration_found() {
    // export declare function foo should be found
    let source = b"export declare function foo(x: number): string;";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "foo", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find ambient function foo"
    );
    let decl = result.unwrap();
    assert!(
        decl.is_function,
        "declaration should be identified as a function"
    );
}
#[test]
fn test_ambient_enum_declaration_found() {
    // export declare enum Status should be found
    let source = b"export declare enum Status { Active, Inactive }";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "Status", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find ambient enum Status"
    );
    let decl = result.unwrap();
    assert!(decl.is_enum, "declaration should be identified as an enum");
    assert!(decl.enum_shape.is_some(), "enum shape should be extracted");
}
#[test]
fn test_ambient_interface_declaration_found() {
    // export declare interface Opts should be found
    let source = b"export declare interface Opts { debug: boolean; }";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "Opts", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find ambient interface Opts"
    );
    let decl = result.unwrap();
    assert!(
        decl.is_interface,
        "declaration should be identified as an interface"
    );
}
#[test]
fn test_find_decl_tolerates_line_distance() {
    // declaration far from hint_line should still be found
    // Simulates a file move where line numbers shift dramatically
    let source = b"\n\n\n\n\n\n\n\n\n\nclass Foo { bar() {} }"; // Foo at line 11
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
    assert!(
        result.is_some(),
        "should find Foo even when hint_line is 1 but actual line is 11"
    );
}
#[test]
fn test_find_decl_closest_match_with_multiple_same_name() {
    // when multiple declarations have the same name (overloads),
    // the closest to hint_row should be picked
    let source = b"function foo(x: string): void;\nfunction foo(x: number): void;\nfunction foo(x: any): void {}";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "foo", 2);
    assert!(result.is_some(), "should find function foo");
    // We can't easily check which overload was picked, but the function should not panic
}
#[test]
fn test_arrow_function_extract() {
    // arrow function exports should be found by extract_declaration
    let source = b"export const add = (x: number, y: number): number => x + y;";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "add", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find arrow function 'add'"
    );
    let decl = result.unwrap();
    assert!(
        decl.is_function,
        "arrow function should be identified as a function"
    );
    assert!(!decl.is_async, "non-async arrow should not be async");
}
#[test]
fn test_async_arrow_function_extract() {
    // async arrow function export
    let source =
        b"export const fetchData = async (url: string): Promise<string> => { return ''; };";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "fetchData", 1);
    assert!(result.is_some(), "should find async arrow function");
    let decl = result.unwrap();
    assert!(decl.is_function, "async arrow should be a function");
    assert!(decl.is_async, "async arrow should have is_async=true");
}
#[test]
fn test_anonymous_default_function_extract() {
    // anonymous default function should be found
    let source = b"export default function(x: number): string { return ''; }";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "default", 1);
    assert!(result.is_some(), "should find anonymous default function");
    let decl = result.unwrap();
    assert!(
        decl.is_function,
        "anonymous function should be identified as function"
    );
}
#[test]
fn test_anonymous_default_class_extract() {
    // anonymous default class should be found
    let source = b"export default class { foo(): void {} }";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "default", 1);
    assert!(result.is_some(), "should find anonymous default class");
    let decl = result.unwrap();
    assert!(
        decl.is_class,
        "anonymous class should be identified as class"
    );
    assert!(
        decl.class_shape.is_some(),
        "class shape should be extracted"
    );
}
#[test]
fn test_function_expression_extract() {
    // function expression in variable should be found
    let source =
        b"export const handler = function(req: Request): Response { return new Response(); };";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "handler", 1);
    assert!(
        result.is_some(),
        "should find function expression 'handler'"
    );
    let decl = result.unwrap();
    assert!(
        decl.is_function,
        "function expression should be identified as function"
    );
}
#[test]
fn test_arrow_function_overloads_extracted() {
    // Arrow functions should have overloads extracted via build_function_shape_from_node
    let source = b"export const greet = (name: string): string => `Hello ${name}`;";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "greet", 1);
    assert!(result.is_some());
    let decl = result.unwrap();
    assert!(
        !decl.overloads.is_empty(),
        "arrow function should have overload shape extracted"
    );
}
#[test]
fn test_interface_extract_found() {
    // interface should be found and classified
    let source = b"export interface Foo { bar(): void; baz: string; }";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
    assert!(result.is_some());
    let decl = result.unwrap();
    assert!(decl.is_interface);
}
#[test]
fn test_collect_interface_members_merged() {
    // two interface declarations with same name should union members
    let source = b"export interface Foo { a(): void; }\nexport interface Foo { b(): void; }";
    let members = collect_interface_members(source, &SourceLanguage::TypeScript, "Foo");
    let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
    assert!(
        names.contains(&"a"),
        "member 'a' from first declaration should be present"
    );
    assert!(
        names.contains(&"b"),
        "member 'b' from second declaration should be present"
    );
    assert_eq!(
        members.len(),
        2,
        "should have exactly 2 members from merged declarations"
    );
}
#[test]
fn test_collect_interface_members_dedup() {
    // Same member name in both declarations — first wins
    let source =
        b"export interface Foo { x(): void; }\nexport interface Foo { x(): void; y: number; }";
    let members = collect_interface_members(source, &SourceLanguage::TypeScript, "Foo");
    let x_count = members.iter().filter(|m| m.name == "x").count();
    assert_eq!(x_count, 1, "duplicate member 'x' should appear only once");
}
#[test]
fn test_find_decl_dotted_name_in_namespace() {
    // find_decl_in_node should find member inside namespace body
    let source = b"export namespace Foo { export function bar(x: number): void {} }";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo.bar", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find 'Foo.bar' inside namespace"
    );
    let decl = result.unwrap();
    assert!(
        decl.is_function,
        "Foo.bar should be identified as a function"
    );
}
#[test]
fn test_find_decl_dotted_name_class_in_namespace() {
    // class member inside namespace
    let source = b"export namespace NS { export class MyClass { foo(): void {} } }";
    let result = extract_declaration(source, &SourceLanguage::TypeScript, "NS.MyClass", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find 'NS.MyClass' inside namespace"
    );
    let decl = result.unwrap();
    assert!(decl.is_class, "NS.MyClass should be identified as a class");
}
#[test]
fn test_cjs_module_exports_class_extract() {
    // module.exports = class Foo { method {} } should be found as class
    let source = b"module.exports = class Foo { method() {} }";
    let result = extract_declaration(source, &SourceLanguage::JavaScript, "default", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find CJS inline class as 'default'"
    );
    let decl = result.unwrap();
    assert!(
        decl.is_class,
        "CJS inline class should be identified as class"
    );
    let shape = decl
        .class_shape
        .as_ref()
        .expect("class_shape should be extracted");
    assert!(
        shape.members.iter().any(|m| m.name == "method"),
        "class shape should contain 'method'; got: {:?}",
        shape.members.iter().map(|m| &m.name).collect::<Vec<_>>()
    );
}
#[test]
fn test_cjs_module_exports_function_extract() {
    // module.exports = function(a, b) {} should be found as function
    let source = b"module.exports = function(a, b) { return a + b; }";
    let result = extract_declaration(source, &SourceLanguage::JavaScript, "default", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find CJS inline function as 'default'"
    );
    let decl = result.unwrap();
    assert!(
        decl.is_function,
        "CJS inline function should be identified as function"
    );
}
#[test]
fn test_cjs_exports_property_function_extract() {
    // exports.handler = async (req, res) => {} should be found
    let source = b"exports.handler = async (req, res) => { return 'ok'; }";
    let result = extract_declaration(source, &SourceLanguage::JavaScript, "handler", 1);
    assert!(
        result.is_some(),
        "extract_declaration should find CJS exports.handler arrow function"
    );
    let decl = result.unwrap();
    assert!(
        decl.is_function,
        "CJS exports.handler arrow should be identified as function"
    );
    assert!(decl.is_async, "async arrow should have is_async=true");
}
#[test]
fn test_cjs_inline_class_structural_diff() {
    // module.exports = class { method {} } with method removed
    // should produce class-method-removed finding via compare_structural_diff
    use crate::languages::javascript::model::{DeclarationKind, ExportKind, ExportSymbol};

    let tmp = tempfile::tempdir().expect("tempdir");
    let old_dir = tmp.path().join("old");
    let new_dir = tmp.path().join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();

    std::fs::write(
        old_dir.join("index.js"),
        b"module.exports = class Foo { method() {} }",
    )
    .unwrap();
    std::fs::write(new_dir.join("index.js"), b"module.exports = class Foo { }").unwrap();

    let file = std::path::PathBuf::from("index.js");
    let sym = ExportSymbol {
        name: "default".to_string(),
        kind: ExportKind::Default,
        file: file.clone(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: vec![DeclarationKind::Class],
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

    let mut old_exports: ExportMap = ExportMap::new();
    old_exports.insert(js_model::ExportKey::new_root_value("default"), sym.clone());
    let mut new_exports: ExportMap = ExportMap::new();
    new_exports.insert(js_model::ExportKey::new_root_value("default"), sym);

    let findings = compare_structural_diff(&old_dir, &new_dir, &old_exports, &new_exports);
    assert!(
        findings.iter().any(|f| f.rule == "class-method-removed"),
        "removing a method from CJS inline class should produce class-method-removed; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}
#[test]
fn test_cjs_inline_function_async_diff() {
    // module.exports = function(a, b) {} changing to async function
    // should produce async-to-sync / sync-to-async finding
    use crate::languages::javascript::model::{DeclarationKind, ExportKind, ExportSymbol};

    let tmp = tempfile::tempdir().expect("tempdir");
    let old_dir = tmp.path().join("old");
    let new_dir = tmp.path().join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();

    std::fs::write(
        old_dir.join("index.js"),
        b"module.exports = function(a, b) { return a + b; }",
    )
    .unwrap();
    std::fs::write(
        new_dir.join("index.js"),
        b"module.exports = async function(a, b) { return a + b; }",
    )
    .unwrap();

    let file = std::path::PathBuf::from("index.js");
    let sym = ExportSymbol {
        name: "default".to_string(),
        kind: ExportKind::Default,
        file: file.clone(),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
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

    let mut old_exports: ExportMap = ExportMap::new();
    old_exports.insert(js_model::ExportKey::new_root_value("default"), sym.clone());
    let mut new_exports_map: ExportMap = ExportMap::new();
    new_exports_map.insert(js_model::ExportKey::new_root_value("default"), sym);

    let findings = compare_structural_diff(&old_dir, &new_dir, &old_exports, &new_exports_map);
    assert!(
        findings.iter().any(|f| f.rule == "sync-to-async"),
        "changing CJS function from sync to async should produce sync-to-async; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}
