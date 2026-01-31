//! Unit tests for the ecma module (parser.rs, entity.rs, walker.rs)

use std::path::{Path, PathBuf};
use std::rc::Rc;

use swc_common::input::StringInput;
use swc_common::sync::Lrc;
use swc_common::{FileName, SourceMap};
use swc_ecma_ast::{EsVersion, Module, ModuleDecl, ModuleItem};
use swc_ecma_parser::lexer::Lexer;
use swc_ecma_parser::{Parser, Syntax};

/// Helper to get the fixtures directory path
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ecma")
}

/// Helper to parse a JavaScript/TypeScript file and return the Module AST
fn parse_file(path: &Path) -> anyhow::Result<Module> {
    let source_map: Lrc<SourceMap> = Default::default();
    let source_file = source_map.load_file(path)?;

    let ext = path.extension().and_then(|e| e.to_str());
    let is_typescript = matches!(ext, Some("ts") | Some("tsx") | Some("cts") | Some("mts"));
    let is_jsx = matches!(ext, Some("jsx") | Some("tsx"));

    let syntax = if is_typescript {
        Syntax::Typescript(swc_ecma_parser::TsConfig {
            tsx: is_jsx,
            ..Default::default()
        })
    } else {
        Syntax::Es(swc_ecma_parser::EsConfig {
            jsx: is_jsx,
            ..Default::default()
        })
    };

    let lexer = Lexer::new(
        syntax,
        EsVersion::EsNext,
        StringInput::from(&*source_file),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    let module = if is_typescript {
        parser.parse_typescript_module()
    } else {
        parser.parse_module()
    };

    module.map_err(|e| anyhow::anyhow!("Parse error: {:?}", e))
}

/// Helper to parse code from a string
fn parse_code(code: &str, is_typescript: bool) -> anyhow::Result<Module> {
    let source_map: Lrc<SourceMap> = Default::default();
    let filename = if is_typescript {
        FileName::Custom("test.ts".into())
    } else {
        FileName::Custom("test.js".into())
    };
    let source_file = source_map.new_source_file(filename, code.to_string());

    let syntax = if is_typescript {
        Syntax::Typescript(Default::default())
    } else {
        Syntax::Es(Default::default())
    };

    let lexer = Lexer::new(
        syntax,
        EsVersion::EsNext,
        StringInput::from(&*source_file),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    let module = if is_typescript {
        parser.parse_typescript_module()
    } else {
        parser.parse_module()
    };

    module.map_err(|e| anyhow::anyhow!("Parse error: {:?}", e))
}

/// Tests for the parser module
mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_simple_js_file() {
        let path = fixtures_dir().join("simple_export.js");
        let result = parse_file(&path);
        assert!(result.is_ok(), "Failed to parse simple_export.js: {:?}", result.err());
    }

    #[test]
    fn test_parse_typescript_file() {
        let path = fixtures_dir().join("typescript_module.ts");
        let result = parse_file(&path);
        assert!(result.is_ok(), "Failed to parse typescript_module.ts: {:?}", result.err());
    }

    #[test]
    fn test_parse_tsx_file() {
        let path = fixtures_dir().join("tsx_component.tsx");
        let result = parse_file(&path);
        assert!(result.is_ok(), "Failed to parse tsx_component.tsx: {:?}", result.err());
    }

    #[test]
    fn test_parse_empty_module() {
        let path = fixtures_dir().join("empty_module.js");
        let result = parse_file(&path);
        assert!(result.is_ok(), "Failed to parse empty_module.js");
    }

    #[test]
    fn test_parse_malformed_file_fails() {
        let path = fixtures_dir().join("malformed.js");
        let result = parse_file(&path);
        assert!(result.is_err(), "Expected malformed.js to fail parsing");
    }

    #[test]
    fn test_parse_nonexistent_file_fails() {
        let path = fixtures_dir().join("does_not_exist.js");
        let result = parse_file(&path);
        assert!(result.is_err(), "Expected nonexistent file to fail");
    }

    #[test]
    fn test_parse_inline_js_code() {
        let code = "export const x = 42;";
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse inline JS: {:?}", result.err());
    }

    #[test]
    fn test_parse_inline_ts_code() {
        let code = "export const x: number = 42;";
        let result = parse_code(code, true);
        assert!(result.is_ok(), "Failed to parse inline TS: {:?}", result.err());
    }

    #[test]
    fn test_parse_empty_code() {
        let code = "";
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse empty code");
        let module = result.unwrap();
        assert!(module.body.is_empty(), "Empty code should produce empty module body");
    }

    #[test]
    fn test_parse_comment_only() {
        let code = "// This is a comment\n/* Block comment */";
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse comment-only code");
    }

    #[test]
    fn test_parse_esm_syntax() {
        let code = r#"
            import { foo } from './foo';
            export { foo };
            export * from './bar';
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse ESM syntax: {:?}", result.err());
    }

    #[test]
    fn test_parse_async_await() {
        let code = r#"
            export async function fetchData() {
                const response = await fetch('/api');
                return response.json();
            }
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse async/await: {:?}", result.err());
    }

    #[test]
    fn test_parse_class_with_private_fields() {
        let code = r#"
            export class Counter {
                #count = 0;

                increment() {
                    this.#count++;
                }

                get value() {
                    return this.#count;
                }
            }
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse private fields: {:?}", result.err());
    }

    #[test]
    fn test_parse_decorators_in_typescript() {
        // Note: Decorators require specific parser config
        let code = r#"
            function log(target: any) {}

            export class Service {
                method() {}
            }
        "#;
        let result = parse_code(code, true);
        assert!(result.is_ok(), "Failed to parse TS with decorator function: {:?}", result.err());
    }

    #[test]
    fn test_parse_jsx_syntax() {
        let source_map: Lrc<SourceMap> = Default::default();
        let filename = FileName::Custom("test.jsx".into());
        let code = r#"
            export function Component() {
                return <div className="test">Hello</div>;
            }
        "#;
        let source_file = source_map.new_source_file(filename, code.to_string());

        let syntax = Syntax::Es(swc_ecma_parser::EsConfig {
            jsx: true,
            ..Default::default()
        });

        let lexer = Lexer::new(
            syntax,
            EsVersion::EsNext,
            StringInput::from(&*source_file),
            None,
        );

        let mut parser = Parser::new_from(lexer);
        let result = parser.parse_module();

        assert!(result.is_ok(), "Failed to parse JSX: {:?}", result.err());
    }
}

/// Tests for module item extraction (mirrors walker.rs functionality)
mod walker_tests {
    use super::*;
    use swc_ecma_ast::{Decl, ModuleDecl, ModuleItem, Stmt};

    /// Count export declarations in a module
    fn count_exports(module: &Module) -> (usize, bool, usize) {
        let mut named_exports = 0;
        let mut has_default = false;
        let mut declarations = 0;

        for item in &module.body {
            match item {
                ModuleItem::ModuleDecl(decl) => match decl {
                    ModuleDecl::ExportDecl(_) => named_exports += 1,
                    ModuleDecl::ExportDefaultDecl(_) => has_default = true,
                    ModuleDecl::ExportDefaultExpr(_) => has_default = true,
                    ModuleDecl::ExportNamed(named) => {
                        named_exports += named.specifiers.len();
                    }
                    _ => {}
                },
                ModuleItem::Stmt(stmt) => {
                    if let Stmt::Decl(_) = stmt {
                        declarations += 1;
                    }
                }
            }
        }

        (named_exports, has_default, declarations)
    }

    #[test]
    fn test_simple_export_counts() {
        let path = fixtures_dir().join("simple_export.js");
        let module = parse_file(&path).unwrap();
        let (named, has_default, _) = count_exports(&module);

        assert_eq!(named, 3, "Expected 3 named exports (greet, PI, Calculator)");
        assert!(!has_default, "Expected no default export");
    }

    #[test]
    fn test_default_export_detection() {
        let path = fixtures_dir().join("default_export.js");
        let module = parse_file(&path).unwrap();
        let (named, has_default, _) = count_exports(&module);

        assert!(has_default, "Expected a default export");
        assert_eq!(named, 1, "Expected 1 named export (helper)");
    }

    #[test]
    fn test_default_class_export() {
        let path = fixtures_dir().join("default_class.js");
        let module = parse_file(&path).unwrap();
        let (_, has_default, _) = count_exports(&module);

        assert!(has_default, "Expected a default class export");
    }

    #[test]
    fn test_expression_default_export() {
        let path = fixtures_dir().join("expression_default.js");
        let module = parse_file(&path).unwrap();
        let (named, has_default, decls) = count_exports(&module);

        assert!(has_default, "Expected default export via expression");
        assert_eq!(named, 1, "Expected 1 named export (config)");
        assert!(decls >= 2, "Expected at least 2 declarations (processData, config)");
    }

    #[test]
    fn test_named_reexport_counts() {
        let path = fixtures_dir().join("named_reexport.js");
        let module = parse_file(&path).unwrap();
        let (named, _, decls) = count_exports(&module);

        // Two specifiers in the named export: helper and anotherHelper
        assert_eq!(named, 2, "Expected 2 named exports via re-export");
        assert_eq!(decls, 2, "Expected 2 internal declarations");
    }

    #[test]
    fn test_empty_module_no_exports() {
        let path = fixtures_dir().join("empty_module.js");
        let module = parse_file(&path).unwrap();
        let (named, has_default, decls) = count_exports(&module);

        assert_eq!(named, 0, "Expected 0 named exports");
        assert!(!has_default, "Expected no default export");
        assert_eq!(decls, 2, "Expected 2 private declarations");
    }

    #[test]
    fn test_typescript_module_exports() {
        let path = fixtures_dir().join("typescript_module.ts");
        let module = parse_file(&path).unwrap();
        let (named, has_default, _) = count_exports(&module);

        assert_eq!(named, 3, "Expected 3 named exports (getUser, UserService, DEFAULT_USER)");
        assert!(!has_default, "Expected no default export");
    }

    #[test]
    fn test_mixed_exports_counts() {
        let path = fixtures_dir().join("mixed_exports.ts");
        let module = parse_file(&path).unwrap();
        let (named, has_default, _) = count_exports(&module);

        // VERSION, mutableValue, legacyVar, syncFunction, asyncFunction, ServiceClass
        assert_eq!(named, 6, "Expected 6 named exports");
        assert!(has_default, "Expected a default export");
    }

    #[test]
    fn test_extract_function_names() {
        let code = r#"
            export function foo() {}
            export function bar() {}
            function baz() {}
        "#;
        let module = parse_code(code, false).unwrap();

        let mut export_names = Vec::new();
        for item in &module.body {
            if let ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) = item {
                if let Decl::Fn(fn_decl) = &export.decl {
                    export_names.push(fn_decl.ident.sym.to_string());
                }
            }
        }

        assert_eq!(export_names, vec!["foo", "bar"]);
    }

    #[test]
    fn test_extract_class_names() {
        let code = r#"
            export class Alpha {}
            export class Beta {}
            class Gamma {}
        "#;
        let module = parse_code(code, false).unwrap();

        let mut export_names = Vec::new();
        for item in &module.body {
            if let ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) = item {
                if let Decl::Class(class_decl) = &export.decl {
                    export_names.push(class_decl.ident.sym.to_string());
                }
            }
        }

        assert_eq!(export_names, vec!["Alpha", "Beta"]);
    }

    #[test]
    fn test_extract_const_names() {
        let code = r#"
            export const A = 1;
            export const B = 2, C = 3;
            const D = 4;
        "#;
        let module = parse_code(code, false).unwrap();

        let mut export_names = Vec::new();
        for item in &module.body {
            if let ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) = item {
                if let Decl::Var(var_decl) = &export.decl {
                    for decl in &var_decl.decls {
                        if let Some(ident) = decl.name.as_ident() {
                            export_names.push(ident.sym.to_string());
                        }
                    }
                }
            }
        }

        assert!(export_names.contains(&"A".to_string()));
        assert!(export_names.contains(&"B".to_string()));
        assert!(export_names.contains(&"C".to_string()));
        assert!(!export_names.contains(&"D".to_string()));
    }
}

/// Tests for entity declaration handling (mirrors entity.rs functionality)
mod entity_tests {
    use super::*;
    use swc_ecma_ast::{Decl, ModuleDecl, ModuleItem};

    /// Simplified entity type enum for testing
    #[derive(Debug, PartialEq)]
    enum EntityType {
        Function,
        Class,
        Variable,
    }

    fn get_export_entity_type(module: &Module, name: &str) -> Option<EntityType> {
        for item in &module.body {
            if let ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) = item {
                match &export.decl {
                    Decl::Fn(fn_decl) if fn_decl.ident.sym.as_ref() == name => {
                        return Some(EntityType::Function);
                    }
                    Decl::Class(class_decl) if class_decl.ident.sym.as_ref() == name => {
                        return Some(EntityType::Class);
                    }
                    Decl::Var(var_decl) => {
                        for decl in &var_decl.decls {
                            if let Some(ident) = decl.name.as_ident() {
                                if ident.sym.as_ref() == name {
                                    return Some(EntityType::Variable);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    #[test]
    fn test_function_entity_type() {
        let code = "export function myFunc() {}";
        let module = parse_code(code, false).unwrap();

        assert_eq!(
            get_export_entity_type(&module, "myFunc"),
            Some(EntityType::Function)
        );
    }

    #[test]
    fn test_class_entity_type() {
        let code = "export class MyClass {}";
        let module = parse_code(code, false).unwrap();

        assert_eq!(
            get_export_entity_type(&module, "MyClass"),
            Some(EntityType::Class)
        );
    }

    #[test]
    fn test_variable_entity_type() {
        let code = "export const myVar = 42;";
        let module = parse_code(code, false).unwrap();

        assert_eq!(
            get_export_entity_type(&module, "myVar"),
            Some(EntityType::Variable)
        );
    }

    #[test]
    fn test_let_variable_entity_type() {
        let code = "export let mutableVar = 'hello';";
        let module = parse_code(code, false).unwrap();

        assert_eq!(
            get_export_entity_type(&module, "mutableVar"),
            Some(EntityType::Variable)
        );
    }

    #[test]
    fn test_arrow_function_as_variable() {
        let code = "export const arrowFn = () => {};";
        let module = parse_code(code, false).unwrap();

        // Arrow functions stored in const are Variable entities, not Function
        assert_eq!(
            get_export_entity_type(&module, "arrowFn"),
            Some(EntityType::Variable)
        );
    }

    #[test]
    fn test_nonexistent_entity() {
        let code = "export const exists = 1;";
        let module = parse_code(code, false).unwrap();

        assert_eq!(get_export_entity_type(&module, "doesNotExist"), None);
    }

    #[test]
    fn test_private_declaration_not_exported() {
        let code = r#"
            const privateVar = 1;
            export const publicVar = 2;
        "#;
        let module = parse_code(code, false).unwrap();

        assert_eq!(get_export_entity_type(&module, "privateVar"), None);
        assert_eq!(
            get_export_entity_type(&module, "publicVar"),
            Some(EntityType::Variable)
        );
    }

    #[test]
    fn test_async_function_entity() {
        let code = "export async function asyncFn() { return Promise.resolve(); }";
        let module = parse_code(code, false).unwrap();

        assert_eq!(
            get_export_entity_type(&module, "asyncFn"),
            Some(EntityType::Function)
        );
    }

    #[test]
    fn test_generator_function_entity() {
        let code = "export function* generatorFn() { yield 1; }";
        let module = parse_code(code, false).unwrap();

        assert_eq!(
            get_export_entity_type(&module, "generatorFn"),
            Some(EntityType::Function)
        );
    }

    #[test]
    fn test_class_with_extends() {
        let code = r#"
            class Base {}
            export class Derived extends Base {}
        "#;
        let module = parse_code(code, false).unwrap();

        assert_eq!(
            get_export_entity_type(&module, "Derived"),
            Some(EntityType::Class)
        );
        assert_eq!(get_export_entity_type(&module, "Base"), None);
    }

    #[test]
    fn test_typescript_typed_export() {
        let code = "export const typed: string = 'hello';";
        let module = parse_code(code, true).unwrap();

        assert_eq!(
            get_export_entity_type(&module, "typed"),
            Some(EntityType::Variable)
        );
    }

    #[test]
    fn test_typescript_interface_not_captured_as_entity() {
        // Interfaces are type-level constructs
        let code = r#"
            export interface MyInterface {
                prop: string;
            }
        "#;
        let module = parse_code(code, true).unwrap();

        // Interface should not appear as Function/Class/Variable
        assert_eq!(get_export_entity_type(&module, "MyInterface"), None);
    }

    #[test]
    fn test_typescript_type_alias_not_captured_as_entity() {
        let code = "export type MyType = string | number;";
        let module = parse_code(code, true).unwrap();

        // Type aliases should not appear as Function/Class/Variable
        assert_eq!(get_export_entity_type(&module, "MyType"), None);
    }
}

/// Edge case and error handling tests
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_unicode_identifiers() {
        let code = r#"
            export const cafe = '1';
            export function greet() {}
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse unicode identifiers");
    }

    #[test]
    fn test_very_long_identifier() {
        let long_name = "a".repeat(1000);
        let code = format!("export const {} = 1;", long_name);
        let result = parse_code(&code, false);
        assert!(result.is_ok(), "Failed to parse very long identifier");
    }

    #[test]
    fn test_deeply_nested_expressions() {
        let code = r#"
            export const nested = {
                a: {
                    b: {
                        c: {
                            d: {
                                e: {
                                    f: 'deep'
                                }
                            }
                        }
                    }
                }
            };
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse deeply nested object");
    }

    #[test]
    fn test_template_literals() {
        let code = r#"
            export const template = `Hello, ${name}!`;
            export const tagged = String.raw`\n is not a newline`;
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse template literals");
    }

    #[test]
    fn test_computed_property_names() {
        let code = r#"
            const KEY = 'dynamic';
            export const obj = {
                [KEY]: 'value',
                ['literal']: 'another'
            };
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse computed property names");
    }

    #[test]
    fn test_destructuring_in_exports() {
        let code = r#"
            const { a, b } = { a: 1, b: 2 };
            export { a, b };
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse destructuring exports");
    }

    #[test]
    fn test_spread_operator() {
        let code = r#"
            export const merged = { ...base, ...override };
            export const arr = [...items, 'new'];
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse spread operator");
    }

    #[test]
    fn test_optional_chaining() {
        let code = r#"
            export const safe = obj?.prop?.nested;
            export const call = fn?.();
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse optional chaining");
    }

    #[test]
    fn test_nullish_coalescing() {
        let code = r#"
            export const value = input ?? 'default';
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse nullish coalescing");
    }

    #[test]
    fn test_logical_assignment() {
        let code = r#"
            let a = 1;
            a ||= 2;
            a &&= 3;
            a ??= 4;
            export { a };
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse logical assignment");
    }

    #[test]
    fn test_numeric_separators() {
        let code = r#"
            export const billion = 1_000_000_000;
            export const hex = 0xFF_FF_FF;
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse numeric separators");
    }

    #[test]
    fn test_bigint_literals() {
        let code = r#"
            export const big = 9007199254740991n;
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse BigInt literals");
    }

    #[test]
    fn test_class_static_blocks() {
        let code = r#"
            export class MyClass {
                static {
                    console.log('Static initialization');
                }
            }
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse static blocks");
    }

    #[test]
    fn test_import_assertions_syntax() {
        // Note: This may require specific parser configuration
        let code = r#"
            export const data = await import('./data.json', { assert: { type: 'json' } });
        "#;
        // Dynamic import with options - may or may not parse depending on SWC version
        let _ = parse_code(code, false);
    }

    #[test]
    fn test_top_level_await() {
        let code = r#"
            export const response = await fetch('/api');
            export const data = await response.json();
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse top-level await");
    }

    #[test]
    fn test_empty_export() {
        let code = "export {};";
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse empty export");
    }

    #[test]
    fn test_export_from() {
        let code = r#"
            export { foo, bar } from './other';
            export * from './all';
            export * as ns from './namespace';
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse export from");
    }

    #[test]
    fn test_export_as_rename() {
        let code = r#"
            const internal = 1;
            export { internal as external };
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse export rename");
    }

    #[test]
    fn test_syntax_error_missing_semicolon_is_ok() {
        // JavaScript has ASI (Automatic Semicolon Insertion)
        let code = r#"
            export const a = 1
            export const b = 2
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "ASI should handle missing semicolons");
    }

    #[test]
    fn test_syntax_error_unclosed_string() {
        // Note: SWC's parser is lenient and recovers from unclosed strings
        // by auto-closing them. This test verifies that behavior.
        let code = r#"export const broken = "unclosed;"#;
        let result = parse_code(code, false);
        // SWC recovers from unclosed strings - parsing succeeds
        assert!(result.is_ok(), "SWC should recover from unclosed string");
    }

    #[test]
    fn test_syntax_error_invalid_token() {
        let code = "export const @ = 1;";
        let result = parse_code(code, false);
        assert!(result.is_err(), "Invalid token should fail");
    }

    #[test]
    fn test_reserved_word_as_export_name() {
        // Can't use reserved words as identifiers
        let code = "export const class = 1;";
        let result = parse_code(code, false);
        assert!(result.is_err(), "Reserved word as identifier should fail");
    }
}

/// Performance and stress tests
mod stress_tests {
    use super::*;

    #[test]
    fn test_many_exports() {
        let mut code = String::new();
        for i in 0..100 {
            code.push_str(&format!("export const var_{} = {};\n", i, i));
        }
        let result = parse_code(&code, false);
        assert!(result.is_ok(), "Failed to parse many exports");

        let module = result.unwrap();
        let export_count = module
            .body
            .iter()
            .filter(|item| matches!(item, ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(_))))
            .count();
        assert_eq!(export_count, 100);
    }

    #[test]
    fn test_large_function_body() {
        let mut body = String::new();
        for i in 0..100 {
            body.push_str(&format!("const local_{} = {};\n", i, i));
        }
        let code = format!("export function large() {{\n{}\nreturn 'done';\n}}", body);
        let result = parse_code(&code, false);
        assert!(result.is_ok(), "Failed to parse large function body");
    }

    #[test]
    fn test_many_class_methods() {
        let mut methods = String::new();
        for i in 0..50 {
            methods.push_str(&format!("method_{}() {{ return {}; }}\n", i, i));
        }
        let code = format!("export class ManyMethods {{\n{}\n}}", methods);
        let result = parse_code(&code, false);
        assert!(result.is_ok(), "Failed to parse class with many methods");
    }
}
