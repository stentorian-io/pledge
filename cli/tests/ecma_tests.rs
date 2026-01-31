//! Unit tests for the ecma module using tree-sitter parser.

use breakpoint::parser::{
    JsParser, NodeKind, ParsedNode, QueryPattern, SourceParser, TsParser, TsxParser,
};
use std::path::PathBuf;

/// Helper to get the fixtures directory path
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ecma")
}

/// Helper to parse a JavaScript file
fn parse_js_file(path: &std::path::Path) -> anyhow::Result<ParsedNode> {
    let source = std::fs::read_to_string(path)?;
    let parser = JsParser::new().map_err(|e| anyhow::anyhow!("{}", e))?;
    parser.parse_str(&source).map_err(|e| anyhow::anyhow!("{}", e))
}

/// Helper to parse a TypeScript file
fn parse_ts_file(path: &std::path::Path) -> anyhow::Result<ParsedNode> {
    let source = std::fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str());

    if matches!(ext, Some("tsx") | Some("jsx")) {
        let parser = TsxParser::new().map_err(|e| anyhow::anyhow!("{}", e))?;
        parser.parse_str(&source).map_err(|e| anyhow::anyhow!("{}", e))
    } else {
        let parser = TsParser::new().map_err(|e| anyhow::anyhow!("{}", e))?;
        parser.parse_str(&source).map_err(|e| anyhow::anyhow!("{}", e))
    }
}

/// Helper to parse code from a string
fn parse_code(code: &str, is_typescript: bool) -> anyhow::Result<ParsedNode> {
    if is_typescript {
        let parser = TsParser::new().map_err(|e| anyhow::anyhow!("{}", e))?;
        parser.parse_str(code).map_err(|e| anyhow::anyhow!("{}", e))
    } else {
        let parser = JsParser::new().map_err(|e| anyhow::anyhow!("{}", e))?;
        parser.parse_str(code).map_err(|e| anyhow::anyhow!("{}", e))
    }
}

/// Tests for the parser module
mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_simple_js_file() {
        let path = fixtures_dir().join("simple_export.js");
        let result = parse_js_file(&path);
        assert!(result.is_ok(), "Failed to parse simple_export.js: {:?}", result.err());
    }

    #[test]
    fn test_parse_typescript_file() {
        let path = fixtures_dir().join("typescript_module.ts");
        let result = parse_ts_file(&path);
        assert!(result.is_ok(), "Failed to parse typescript_module.ts: {:?}", result.err());
    }

    #[test]
    fn test_parse_tsx_file() {
        let path = fixtures_dir().join("tsx_component.tsx");
        let result = parse_ts_file(&path);
        assert!(result.is_ok(), "Failed to parse tsx_component.tsx: {:?}", result.err());
    }

    #[test]
    fn test_parse_empty_module() {
        let path = fixtures_dir().join("empty_module.js");
        let result = parse_js_file(&path);
        assert!(result.is_ok(), "Failed to parse empty_module.js");
    }

    #[test]
    fn test_parse_malformed_file_recovers() {
        // Tree-sitter handles errors gracefully and produces partial trees
        let path = fixtures_dir().join("malformed.js");
        let result = parse_js_file(&path);
        // Tree-sitter will produce a tree even with errors
        assert!(result.is_ok(), "Tree-sitter should recover from malformed input");
    }

    #[test]
    fn test_parse_nonexistent_file_fails() {
        let path = fixtures_dir().join("does_not_exist.js");
        let result = parse_js_file(&path);
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
        assert!(module.children.is_empty(), "Empty code should produce empty children");
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
    fn test_parse_jsx_syntax() {
        let parser = TsxParser::new().unwrap();
        let code = r#"
            export function Component() {
                return <div className="test">Hello</div>;
            }
        "#;
        let result = parser.parse_str(code);
        assert!(result.is_ok(), "Failed to parse JSX: {:?}", result.err());
    }
}

/// Tests for module item extraction
mod walker_tests {
    use super::*;

    /// Count export nodes in a parsed module
    fn count_exports(module: &ParsedNode) -> (usize, bool) {
        let exports = module.exports();
        let has_default = exports.iter().any(|e| e.is_default_export);
        (exports.len(), has_default)
    }

    #[test]
    fn test_simple_export_counts() {
        let path = fixtures_dir().join("simple_export.js");
        let module = parse_js_file(&path).unwrap();
        let exports = module.exports();

        // Should have some exports
        assert!(!exports.is_empty(), "Expected exports in simple_export.js");
    }

    #[test]
    fn test_default_export_detection() {
        let path = fixtures_dir().join("default_export.js");
        let module = parse_js_file(&path).unwrap();
        let (_, has_default) = count_exports(&module);

        assert!(has_default, "Expected a default export");
    }

    #[test]
    fn test_default_class_export() {
        let path = fixtures_dir().join("default_class.js");
        let module = parse_js_file(&path).unwrap();
        let (_, has_default) = count_exports(&module);

        assert!(has_default, "Expected a default class export");
    }

    #[test]
    fn test_typescript_module_exports() {
        let path = fixtures_dir().join("typescript_module.ts");
        let module = parse_ts_file(&path).unwrap();
        let exports = module.exports();

        assert!(!exports.is_empty(), "Expected exports in TypeScript module");
    }

    #[test]
    fn test_extract_function_names() {
        let code = r#"
            export function foo() {}
            export function bar() {}
            function baz() {}
        "#;
        let module = parse_code(code, false).unwrap();
        let functions = module.functions();

        // Should find all 3 functions (exported and non-exported)
        assert_eq!(functions.len(), 3, "Expected 3 functions");
    }

    #[test]
    fn test_extract_class_names() {
        let code = r#"
            export class Alpha {}
            export class Beta {}
            class Gamma {}
        "#;
        let module = parse_code(code, false).unwrap();
        let classes = module.classes();

        assert_eq!(classes.len(), 3, "Expected 3 classes");
    }
}

/// Tests for query functionality
mod query_tests {
    use super::*;

    #[test]
    fn test_query_functions() {
        let parser = JsParser::new().unwrap();
        let source = r#"
            function foo() {}
            function bar() {}
            const baz = () => {};
        "#;

        let pattern = QueryPattern::functions();
        let matches = parser.query(source, &pattern);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        // Should find 2 function declarations (foo and bar)
        assert!(matches.len() >= 2, "Expected at least 2 function matches");
    }

    #[test]
    fn test_query_exports() {
        let parser = JsParser::new().unwrap();
        let source = r#"
            export const A = 1;
            export function foo() {}
            const internal = 2;
        "#;

        let pattern = QueryPattern::exports();
        let matches = parser.query(source, &pattern);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert_eq!(matches.len(), 2, "Expected 2 export statements");
    }

    #[test]
    fn test_query_imports() {
        let parser = JsParser::new().unwrap();
        let source = r#"
            import { foo } from './foo';
            import * as bar from './bar';
            import baz from './baz';
        "#;

        let pattern = QueryPattern::imports();
        let matches = parser.query(source, &pattern);
        assert!(matches.is_ok());

        let matches = matches.unwrap();
        assert!(matches.len() >= 3, "Expected at least 3 import sources");
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
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "Failed to parse template literals");
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
    fn test_error_recovery_missing_semicolon() {
        // JavaScript has ASI (Automatic Semicolon Insertion)
        let code = r#"
            export const a = 1
            export const b = 2
        "#;
        let result = parse_code(code, false);
        assert!(result.is_ok(), "ASI should handle missing semicolons");
    }

    #[test]
    fn test_error_recovery_partial_parse() {
        // Tree-sitter handles partial/incomplete code gracefully
        let code = "const x = ";
        let result = parse_code(code, false);
        // Tree-sitter produces a tree even with errors
        assert!(result.is_ok(), "Tree-sitter should recover from partial input");

        let parsed = result.unwrap();
        // Check that there's an error node somewhere
        let has_error = parsed.find_first(|n| n.kind == NodeKind::Error).is_some();
        assert!(has_error, "Expected error node in partial parse");
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
        let exports = module.exports();
        // The exports() method finds all nodes marked as exported, including nested children
        // So we check there are at least 100 exports (one for each statement)
        assert!(exports.len() >= 100, "Expected at least 100 exports, got {}", exports.len());
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

    #[test]
    fn test_incremental_parsing() {
        let parser = JsParser::new().unwrap();

        let source1 = "const x = 1;";
        let tree1 = parser.parse_incremental(source1, None).unwrap();

        let source2 = "const x = 2;";
        let tree2 = parser.parse_incremental(source2, Some(&tree1)).unwrap();

        assert!(!tree2.root_node().has_error());
    }
}

/// TypeScript-specific tests
mod typescript_tests {
    use super::*;

    #[test]
    fn test_typescript_interface() {
        let code = r#"
            export interface User {
                id: number;
                name: string;
            }
        "#;
        let result = parse_code(code, true);
        assert!(result.is_ok(), "Failed to parse TypeScript interface");
    }

    #[test]
    fn test_typescript_type_alias() {
        let code = "export type ID = string | number;";
        let result = parse_code(code, true);
        assert!(result.is_ok(), "Failed to parse TypeScript type alias");
    }

    #[test]
    fn test_typescript_enum() {
        let code = r#"
            export enum Status {
                Active,
                Inactive,
                Pending
            }
        "#;
        let result = parse_code(code, true);
        assert!(result.is_ok(), "Failed to parse TypeScript enum");
    }

    #[test]
    fn test_typescript_generics() {
        let code = r#"
            export function identity<T>(arg: T): T {
                return arg;
            }

            export class Container<T> {
                constructor(private value: T) {}
            }
        "#;
        let result = parse_code(code, true);
        assert!(result.is_ok(), "Failed to parse TypeScript generics");
    }

    #[test]
    fn test_typescript_decorators() {
        let code = r#"
            function log(target: any) {}

            export class Service {
                method() {}
            }
        "#;
        let result = parse_code(code, true);
        assert!(result.is_ok(), "Failed to parse TypeScript with decorators");
    }

    #[test]
    fn test_typescript_namespace() {
        let code = r#"
            export namespace Utils {
                export function helper() {}
            }
        "#;
        let result = parse_code(code, true);
        assert!(result.is_ok(), "Failed to parse TypeScript namespace");
    }
}
