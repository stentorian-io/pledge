//! Comprehensive tests for the tree-sitter based JavaScript/TypeScript parser
//!
//! This module contains unit tests, integration tests, edge case tests,
//! and regression tests for the tree-sitter parser implementation.
//!
//! Test categories:
//! 1. Unit tests for parser functionality
//! 2. Integration tests with real-world JS/TS files
//! 3. Edge cases (empty files, comments only, Unicode, large files)
//! 4. Regression tests for parser behavior
//! 5. Incremental parsing tests
//! 6. Query pattern tests
//! 7. AST node extraction tests

use std::path::{Path, PathBuf};
use std::time::Instant;

// Import the parser module from the crate
use breakpoint::parser::{
    JsParser, NodeKind, ParsedNode, QueryPattern, SourceParser, TsParser, TsxParser,
    TreeSitterParser, LanguageParser, create_js_ts_registry,
};

// =============================================================================
// Test Helpers
// =============================================================================

/// Get the path to the ecma fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ecma")
}

/// Parse a JavaScript file from the fixtures directory
fn parse_js_fixture(filename: &str) -> Result<ParsedNode, breakpoint::parser::ParseError> {
    let path = fixtures_dir().join(filename);
    let parser = JsParser::new()?;
    parser.parse_file(&path)
}

/// Parse a TypeScript file from the fixtures directory
fn parse_ts_fixture(filename: &str) -> Result<ParsedNode, breakpoint::parser::ParseError> {
    let path = fixtures_dir().join(filename);
    let parser = TsParser::new()?;
    parser.parse_file(&path)
}

/// Parse a TSX file from the fixtures directory
fn parse_tsx_fixture(filename: &str) -> Result<ParsedNode, breakpoint::parser::ParseError> {
    let path = fixtures_dir().join(filename);
    let parser = TsxParser::new()?;
    parser.parse_file(&path)
}

// =============================================================================
// Unit Tests - Parser Initialization
// =============================================================================

mod parser_initialization_tests {
    use super::*;

    #[test]
    fn test_js_parser_creation() {
        let result = JsParser::new();
        assert!(result.is_ok(), "Failed to create JavaScript parser");
    }

    #[test]
    fn test_ts_parser_creation() {
        let result = TsParser::new();
        assert!(result.is_ok(), "Failed to create TypeScript parser");
    }

    #[test]
    fn test_tsx_parser_creation() {
        let result = TsxParser::new();
        assert!(result.is_ok(), "Failed to create TSX parser");
    }

    #[test]
    fn test_js_parser_default() {
        let parser = JsParser::default();
        assert_eq!(parser.language_name(), "javascript");
    }

    #[test]
    fn test_ts_parser_default() {
        let parser = TsParser::default();
        assert_eq!(parser.language_name(), "typescript");
    }

    #[test]
    fn test_tsx_parser_default() {
        let parser = TsxParser::default();
        assert_eq!(parser.language_name(), "tsx");
    }

    #[test]
    fn test_js_parser_extensions() {
        let parser = JsParser::default();
        let extensions = parser.file_extensions();
        assert!(extensions.contains(&"js"));
        assert!(extensions.contains(&"mjs"));
        assert!(extensions.contains(&"cjs"));
    }

    #[test]
    fn test_ts_parser_extensions() {
        let parser = TsParser::default();
        let extensions = parser.file_extensions();
        assert!(extensions.contains(&"ts"));
        assert!(extensions.contains(&"mts"));
        assert!(extensions.contains(&"cts"));
    }

    #[test]
    fn test_tsx_parser_extensions() {
        let parser = TsxParser::default();
        let extensions = parser.file_extensions();
        assert!(extensions.contains(&"tsx"));
        assert!(extensions.contains(&"jsx"));
    }
}

// =============================================================================
// Unit Tests - Basic Parsing
// =============================================================================

mod basic_parsing_tests {
    use super::*;

    #[test]
    fn test_parse_simple_variable() {
        let parser = JsParser::default();
        let source = "const x = 42;";
        let result = parser.parse_str(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_let_variable() {
        let parser = JsParser::default();
        let source = "let mutable = 'hello';";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_var_variable() {
        let parser = JsParser::default();
        let source = "var legacy = true;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_function_declaration() {
        let parser = JsParser::default();
        let source = "function greet(name) { return `Hello, ${name}!`; }";
        let result = parser.parse_str(source);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        let functions = parsed.functions();
        assert!(!functions.is_empty(), "Should find at least one function");
    }

    #[test]
    fn test_parse_arrow_function() {
        let parser = JsParser::default();
        let source = "const add = (a, b) => a + b;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_class_declaration() {
        let parser = JsParser::default();
        let source = "class MyClass { constructor() {} }";
        let result = parser.parse_str(source);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        let classes = parsed.classes();
        assert!(!classes.is_empty(), "Should find at least one class");
    }

    #[test]
    fn test_parse_export_const() {
        let parser = JsParser::default();
        let source = "export const PI = 3.14159;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        let exports = parsed.exports();
        assert!(!exports.is_empty(), "Should find exports");
    }

    #[test]
    fn test_parse_export_function() {
        let parser = JsParser::default();
        let source = "export function exported() { return 'hello'; }";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_export_class() {
        let parser = JsParser::default();
        let source = "export class ExportedClass {}";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_export_default_function() {
        let parser = JsParser::default();
        let source = "export default function() { return 'default'; }";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_export_default_class() {
        let parser = JsParser::default();
        let source = "export default class {}";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_export_default_expression() {
        let parser = JsParser::default();
        let source = "const x = 42; export default x;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_named_exports() {
        let parser = JsParser::default();
        let source = "const a = 1; const b = 2; export { a, b };";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_export_rename() {
        let parser = JsParser::default();
        let source = "const internal = 'value'; export { internal as external };";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_import_named() {
        let parser = JsParser::default();
        let source = "import { foo } from './foo';";
        let result = parser.parse_str(source);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        let imports = parsed.imports();
        assert!(!imports.is_empty(), "Should find imports");
    }

    #[test]
    fn test_parse_import_default() {
        let parser = JsParser::default();
        let source = "import bar from './bar';";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_import_namespace() {
        let parser = JsParser::default();
        let source = "import * as ns from './module';";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_import_side_effect() {
        let parser = JsParser::default();
        let source = "import './side-effect';";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Unit Tests - TypeScript Parsing
// =============================================================================

mod typescript_parsing_tests {
    use super::*;

    #[test]
    fn test_parse_typed_variable() {
        let parser = TsParser::default();
        let source = "const x: number = 42;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_interface() {
        let parser = TsParser::default();
        let source = r#"
            interface User {
                id: number;
                name: string;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_type_alias() {
        let parser = TsParser::default();
        let source = "type ID = string | number;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_enum() {
        let parser = TsParser::default();
        let source = r#"
            enum Status {
                Active = 'ACTIVE',
                Inactive = 'INACTIVE'
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_generic_function() {
        let parser = TsParser::default();
        let source = "function identity<T>(arg: T): T { return arg; }";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_generic_class() {
        let parser = TsParser::default();
        let source = r#"
            class Container<T> {
                constructor(private value: T) {}
                getValue(): T { return this.value; }
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_type_guard() {
        let parser = TsParser::default();
        let source = r#"
            function isString(value: unknown): value is string {
                return typeof value === 'string';
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_conditional_type() {
        let parser = TsParser::default();
        let source = "type IsString<T> = T extends string ? true : false;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_mapped_type() {
        let parser = TsParser::default();
        let source = "type Readonly<T> = { readonly [P in keyof T]: T[P] };";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_namespace() {
        let parser = TsParser::default();
        let source = r#"
            namespace Utils {
                export function helper(): void {}
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_abstract_class() {
        let parser = TsParser::default();
        let source = r#"
            abstract class Base {
                abstract method(): void;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_as_const() {
        let parser = TsParser::default();
        let source = "const arr = [1, 2, 3] as const;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_satisfies() {
        let parser = TsParser::default();
        let source = "const config = { port: 3000 } satisfies Record<string, number>;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Unit Tests - JSX/TSX Parsing
// =============================================================================

mod jsx_parsing_tests {
    use super::*;

    #[test]
    fn test_parse_simple_jsx() {
        let parser = TsxParser::default();
        let source = r#"
            function Component() {
                return <div>Hello</div>;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_jsx_with_props() {
        let parser = TsxParser::default();
        let source = r#"
            function Component({ name }) {
                return <span className="greeting">Hello, {name}!</span>;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_jsx_fragment() {
        let parser = TsxParser::default();
        let source = r#"
            function Component() {
                return (
                    <>
                        <h1>Title</h1>
                        <p>Content</p>
                    </>
                );
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_jsx_self_closing() {
        let parser = TsxParser::default();
        let source = r#"
            function Component() {
                return <input type="text" />;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_jsx_spread_props() {
        let parser = TsxParser::default();
        let source = r#"
            function Component(props) {
                return <input {...props} />;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_jsx_nested() {
        let parser = TsxParser::default();
        let source = r#"
            function Component() {
                return (
                    <div>
                        <header>
                            <nav>
                                <ul>
                                    <li><a href="/">Home</a></li>
                                </ul>
                            </nav>
                        </header>
                    </div>
                );
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_tsx_with_types() {
        let parser = TsxParser::default();
        let source = r#"
            interface Props {
                name: string;
            }

            function Component({ name }: Props) {
                return <div>{name}</div>;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_jsx_expressions() {
        let parser = TsxParser::default();
        let source = r#"
            function Component({ items, show }) {
                return (
                    <div>
                        {show && <span>Visible</span>}
                        {items.map(item => <li key={item}>{item}</li>)}
                    </div>
                );
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Unit Tests - Modern JavaScript Syntax
// =============================================================================

mod modern_syntax_tests {
    use super::*;

    #[test]
    fn test_parse_optional_chaining() {
        let parser = JsParser::default();
        let source = "const value = obj?.nested?.prop;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_nullish_coalescing() {
        let parser = JsParser::default();
        let source = "const value = input ?? 'default';";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_logical_assignment_or() {
        let parser = JsParser::default();
        let source = "let a = null; a ||= 'fallback';";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_logical_assignment_and() {
        let parser = JsParser::default();
        let source = "let a = true; a &&= false;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_logical_assignment_nullish() {
        let parser = JsParser::default();
        let source = "let a = null; a ??= 'default';";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_numeric_separators() {
        let parser = JsParser::default();
        let source = "const billion = 1_000_000_000;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_bigint() {
        let parser = JsParser::default();
        let source = "const big = 9007199254740991n;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_private_fields() {
        let parser = JsParser::default();
        let source = r#"
            class Counter {
                #count = 0;
                increment() { return ++this.#count; }
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_static_blocks() {
        let parser = JsParser::default();
        let source = r#"
            class Initialized {
                static data;
                static { this.data = 42; }
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_async_function() {
        let parser = JsParser::default();
        let source = "async function fetchData() { return await fetch('/api'); }";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_generator_function() {
        let parser = JsParser::default();
        let source = "function* numberGen() { yield 1; yield 2; }";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_async_generator() {
        let parser = JsParser::default();
        let source = "async function* asyncGen() { yield await Promise.resolve(1); }";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_destructuring_object() {
        let parser = JsParser::default();
        let source = "const { a, b: renamed, c = 10 } = obj;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_destructuring_array() {
        let parser = JsParser::default();
        let source = "const [first, , third, ...rest] = arr;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_spread_operator() {
        let parser = JsParser::default();
        let source = "const merged = { ...obj1, ...obj2 };";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_template_literal() {
        let parser = JsParser::default();
        let source = r#"const greeting = `Hello, ${name}!`;"#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_tagged_template() {
        let parser = JsParser::default();
        let source = r#"const result = tag`template ${value}`;"#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Edge Case Tests - Empty and Minimal
// =============================================================================

mod edge_case_empty_tests {
    use super::*;

    #[test]
    fn test_parse_empty_string() {
        let parser = JsParser::default();
        let result = parser.parse_str("");
        assert!(result.is_ok(), "Should parse empty string");
    }

    #[test]
    fn test_parse_whitespace_only() {
        let parser = JsParser::default();
        let result = parser.parse_str("   \n\t\r\n   ");
        assert!(result.is_ok(), "Should parse whitespace-only");
    }

    #[test]
    fn test_parse_single_semicolon() {
        let parser = JsParser::default();
        let result = parser.parse_str(";");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_multiple_semicolons() {
        let parser = JsParser::default();
        let result = parser.parse_str(";;;");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_empty_block() {
        let parser = JsParser::default();
        let result = parser.parse_str("{}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_empty_export() {
        let parser = JsParser::default();
        let result = parser.parse_str("export {};");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_line_comment_only() {
        let parser = JsParser::default();
        let result = parser.parse_str("// just a comment");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_block_comment_only() {
        let parser = JsParser::default();
        let result = parser.parse_str("/* block comment */");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_jsdoc_comment_only() {
        let parser = JsParser::default();
        let result = parser.parse_str("/** JSDoc comment */");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_multiple_comments() {
        let parser = JsParser::default();
        let source = r#"
            // Line comment
            /* Block comment */
            /**
             * JSDoc
             */
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Edge Case Tests - Unicode
// =============================================================================

mod edge_case_unicode_tests {
    use super::*;

    #[test]
    fn test_parse_unicode_string() {
        let parser = JsParser::default();
        let source = r#"const greeting = "\u4f60\u597d";"#;  // "Hello" in Chinese
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_unicode_identifier() {
        let parser = JsParser::default();
        let source = "const cafe = 'coffee';";  // Unicode identifier
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_emoji_in_string() {
        let parser = JsParser::default();
        let source = r#"const emoji = "Hello World";"#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_unicode_escape_sequence() {
        let parser = JsParser::default();
        let source = r#"const hello = '\u0048\u0065\u006c\u006c\u006f';"#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_unicode_regex() {
        let parser = JsParser::default();
        let source = r#"const pattern = /[\u4e00-\u9fff]/u;"#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Edge Case Tests - Syntax Errors and Error Recovery
// =============================================================================

mod edge_case_error_tests {
    use super::*;

    #[test]
    fn test_parse_incomplete_statement() {
        let parser = JsParser::default();
        // Tree-sitter produces a tree even with errors
        let source = "const x = ";
        let result = parser.parse_str(source);
        assert!(result.is_ok(), "Tree-sitter should recover from incomplete code");
    }

    #[test]
    fn test_parse_unclosed_brace() {
        let parser = JsParser::default();
        let source = "function broken() {";
        let result = parser.parse_str(source);
        // Tree-sitter should still produce a tree
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_unclosed_bracket() {
        let parser = JsParser::default();
        let source = "const arr = [1, 2, 3";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_unclosed_paren() {
        let parser = JsParser::default();
        let source = "function test(a, b {";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_missing_semicolon_ok() {
        let parser = JsParser::default();
        // JavaScript has ASI (Automatic Semicolon Insertion)
        let source = "const a = 1\nconst b = 2";
        let result = parser.parse_str(source);
        assert!(result.is_ok(), "ASI should handle missing semicolons");
    }

    #[test]
    fn test_parse_extra_semicolons() {
        let parser = JsParser::default();
        let source = "const x = 1;;; const y = 2;;;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Edge Case Tests - Large Inputs
// =============================================================================

mod edge_case_large_tests {
    use super::*;

    #[test]
    fn test_parse_many_variables() {
        let parser = JsParser::default();
        let mut code = String::new();
        for i in 0..100 {
            code.push_str(&format!("const var_{} = {};\n", i, i));
        }
        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_very_long_identifier() {
        let parser = JsParser::default();
        let long_name = "a".repeat(1000);
        let code = format!("const {} = 1;", long_name);
        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_very_long_string() {
        let parser = JsParser::default();
        let long_string = "x".repeat(10000);
        let code = format!("const s = \"{}\";", long_string);
        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_large_array() {
        let parser = JsParser::default();
        let elements: Vec<String> = (0..500).map(|i| i.to_string()).collect();
        let code = format!("const arr = [{}];", elements.join(", "));
        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_large_object() {
        let parser = JsParser::default();
        let props: Vec<String> = (0..200)
            .map(|i| format!("prop{}: {}", i, i))
            .collect();
        let code = format!("const obj = {{ {} }};", props.join(", "));
        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_many_functions() {
        let parser = JsParser::default();
        let mut code = String::new();
        for i in 0..50 {
            code.push_str(&format!("function fn{}() {{ return {}; }}\n", i, i));
        }
        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_many_class_methods() {
        let parser = JsParser::default();
        let mut methods = String::new();
        for i in 0..50 {
            methods.push_str(&format!("method{}() {{ return {}; }}\n", i, i));
        }
        let code = format!("class ManyMethods {{\n{}\n}}", methods);
        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Edge Case Tests - Deep Nesting
// =============================================================================

mod edge_case_nesting_tests {
    use super::*;

    #[test]
    fn test_parse_deeply_nested_objects() {
        let parser = JsParser::default();
        let mut code = "const deep = ".to_string();
        for _ in 0..15 {
            code.push_str("{ nested: ");
        }
        code.push_str("'value'");
        for _ in 0..15 {
            code.push_str(" }");
        }
        code.push(';');

        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_deeply_nested_arrays() {
        let parser = JsParser::default();
        let mut code = "const deep = ".to_string();
        for _ in 0..15 {
            code.push('[');
        }
        code.push_str("'value'");
        for _ in 0..15 {
            code.push(']');
        }
        code.push(';');

        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_deeply_nested_functions() {
        let parser = JsParser::default();
        let mut code = "const deep = ".to_string();
        for _ in 0..10 {
            code.push_str("function() { return ");
        }
        code.push_str("'value'");
        for _ in 0..10 {
            code.push_str("; }");
        }
        code.push(';');

        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_deeply_nested_ternary() {
        let parser = JsParser::default();
        let mut code = "const result = ".to_string();
        for i in 0..10 {
            code.push_str(&format!("true ? {} : ", i));
        }
        code.push_str("'final'");
        code.push(';');

        let result = parser.parse_str(&code);
        assert!(result.is_ok());
    }
}

// =============================================================================
// Integration Tests - Fixture Files
// =============================================================================

mod integration_fixture_tests {
    use super::*;

    #[test]
    fn test_parse_simple_export_fixture() {
        let result = parse_js_fixture("simple_export.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());

        let parsed = result.unwrap();
        let functions = parsed.functions();
        assert!(!functions.is_empty());
    }

    #[test]
    fn test_parse_typescript_module_fixture() {
        let result = parse_ts_fixture("typescript_module.ts");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_tsx_component_fixture() {
        let result = parse_tsx_fixture("tsx_component.tsx");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_mixed_exports_fixture() {
        let result = parse_ts_fixture("mixed_exports.ts");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_default_export_fixture() {
        let result = parse_js_fixture("default_export.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_default_class_fixture() {
        let result = parse_js_fixture("default_class.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_expression_default_fixture() {
        let result = parse_js_fixture("expression_default.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_empty_module_fixture() {
        let result = parse_js_fixture("empty_module.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_named_reexport_fixture() {
        let result = parse_js_fixture("named_reexport.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_modern_syntax_fixture() {
        let result = parse_js_fixture("modern_syntax.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_async_generators_fixture() {
        let result = parse_js_fixture("async_generators.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_destructuring_fixture() {
        let result = parse_js_fixture("destructuring.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_classes_advanced_fixture() {
        let result = parse_js_fixture("classes_advanced.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_template_literals_fixture() {
        let result = parse_js_fixture("template_literals.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_typescript_advanced_fixture() {
        let result = parse_ts_fixture("typescript_advanced.ts");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_jsx_advanced_fixture() {
        let result = parse_tsx_fixture("jsx_advanced.jsx");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_syntax_variations_fixture() {
        let result = parse_js_fixture("syntax_variations.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_deeply_nested_fixture() {
        let result = parse_js_fixture("deeply_nested.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_unicode_identifiers_fixture() {
        let result = parse_js_fixture("unicode_identifiers.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_comments_only_fixture() {
        let result = parse_js_fixture("comments_only.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_parse_empty_fixture() {
        let result = parse_js_fixture("empty.js");
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }
}

// =============================================================================
// Query Pattern Tests
// =============================================================================

mod query_pattern_tests {
    use super::*;

    #[test]
    fn test_query_functions() {
        let parser = JsParser::default();
        let source = r#"
            function foo() {}
            function bar() {}
            const baz = () => {};
        "#;

        let pattern = QueryPattern::functions();
        let result = parser.query(source, &pattern);
        assert!(result.is_ok());

        let matches = result.unwrap();
        // Should find function declarations
        assert!(matches.len() >= 2);
    }

    #[test]
    fn test_query_classes() {
        let parser = JsParser::default();
        let source = r#"
            class Alpha {}
            class Beta {}
        "#;

        let pattern = QueryPattern::classes();
        let result = parser.query(source, &pattern);
        assert!(result.is_ok());

        let matches = result.unwrap();
        assert!(matches.len() >= 2);
    }

    #[test]
    fn test_query_exports() {
        let parser = JsParser::default();
        let source = r#"
            export const A = 1;
            export function foo() {}
            const internal = 2;
        "#;

        let pattern = QueryPattern::exports();
        let result = parser.query(source, &pattern);
        assert!(result.is_ok());

        let matches = result.unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_query_imports() {
        let parser = JsParser::default();
        let source = r#"
            import { foo } from './foo';
            import bar from './bar';
        "#;

        let pattern = QueryPattern::imports();
        let result = parser.query(source, &pattern);
        assert!(result.is_ok());

        let matches = result.unwrap();
        assert!(matches.len() >= 2);
    }

    #[test]
    fn test_query_variables() {
        let parser = JsParser::default();
        let source = r#"
            const a = 1;
            let b = 2;
            var c = 3;
        "#;

        let pattern = QueryPattern::variables();
        let result = parser.query(source, &pattern);
        assert!(result.is_ok());

        let matches = result.unwrap();
        assert!(matches.len() >= 3);
    }

    #[test]
    fn test_query_interfaces_ts() {
        let parser = TsParser::default();
        let source = r#"
            interface User { id: number; }
            interface Product { name: string; }
        "#;

        let pattern = QueryPattern::interfaces();
        let result = parser.query(source, &pattern);
        assert!(result.is_ok());

        let matches = result.unwrap();
        assert!(matches.len() >= 2);
    }

    #[test]
    fn test_query_type_aliases_ts() {
        let parser = TsParser::default();
        let source = r#"
            type ID = string;
            type Result = Success | Error;
        "#;

        let pattern = QueryPattern::type_aliases();
        let result = parser.query(source, &pattern);
        assert!(result.is_ok());

        let matches = result.unwrap();
        assert!(matches.len() >= 2);
    }

    #[test]
    fn test_query_enums_ts() {
        let parser = TsParser::default();
        let source = r#"
            enum Status { Active, Inactive }
            enum Color { Red, Green, Blue }
        "#;

        let pattern = QueryPattern::enums();
        let result = parser.query(source, &pattern);
        assert!(result.is_ok());

        let matches = result.unwrap();
        assert!(matches.len() >= 2);
    }
}

// =============================================================================
// Incremental Parsing Tests
// =============================================================================

mod incremental_parsing_tests {
    use super::*;

    #[test]
    fn test_incremental_basic() {
        let parser = JsParser::default();

        let source1 = "const x = 1;";
        let tree1 = parser.parse_incremental(source1, None);
        assert!(tree1.is_ok());

        let tree1 = tree1.unwrap();

        let source2 = "const x = 2;";
        let tree2 = parser.parse_incremental(source2, Some(&tree1));
        assert!(tree2.is_ok());

        let tree2 = tree2.unwrap();
        assert!(!tree2.root_node().has_error());
    }

    #[test]
    fn test_incremental_add_line() {
        let parser = JsParser::default();

        let source1 = "const x = 1;";
        let tree1 = parser.parse_incremental(source1, None).unwrap();

        let source2 = "const x = 1;\nconst y = 2;";
        let tree2 = parser.parse_incremental(source2, Some(&tree1)).unwrap();

        assert!(!tree2.root_node().has_error());
    }

    #[test]
    fn test_incremental_remove_line() {
        let parser = JsParser::default();

        let source1 = "const x = 1;\nconst y = 2;";
        let tree1 = parser.parse_incremental(source1, None).unwrap();

        let source2 = "const x = 1;";
        let tree2 = parser.parse_incremental(source2, Some(&tree1)).unwrap();

        assert!(!tree2.root_node().has_error());
    }

    #[test]
    fn test_incremental_multiple_changes() {
        let parser = JsParser::default();

        let mut source = "const x = 1;".to_string();
        let mut tree = parser.parse_incremental(&source, None).unwrap();

        for i in 2..10 {
            source = format!("{}\nconst var{} = {};", source, i, i);
            tree = parser.parse_incremental(&source, Some(&tree)).unwrap();
            assert!(!tree.root_node().has_error());
        }
    }
}

// =============================================================================
// Parser Registry Tests
// =============================================================================

mod parser_registry_tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = create_js_ts_registry();
        assert!(registry.get_parser_for_extension("js").is_some());
        assert!(registry.get_parser_for_extension("ts").is_some());
        assert!(registry.get_parser_for_extension("tsx").is_some());
    }

    #[test]
    fn test_registry_unknown_extension() {
        let registry = create_js_ts_registry();
        assert!(registry.get_parser_for_extension("rs").is_none());
        assert!(registry.get_parser_for_extension("py").is_none());
    }

    #[test]
    fn test_registry_all_js_extensions() {
        let registry = create_js_ts_registry();
        for ext in &["js", "mjs", "cjs"] {
            assert!(
                registry.get_parser_for_extension(ext).is_some(),
                "Should support .{}", ext
            );
        }
    }

    #[test]
    fn test_registry_all_ts_extensions() {
        let registry = create_js_ts_registry();
        for ext in &["ts", "mts", "cts"] {
            assert!(
                registry.get_parser_for_extension(ext).is_some(),
                "Should support .{}", ext
            );
        }
    }

    #[test]
    fn test_registry_jsx_tsx_extensions() {
        let registry = create_js_ts_registry();
        assert!(registry.get_parser_for_extension("jsx").is_some());
        assert!(registry.get_parser_for_extension("tsx").is_some());
    }
}

// =============================================================================
// Performance Tests
// =============================================================================

mod performance_tests {
    use super::*;

    #[test]
    fn test_parse_performance_small() {
        let parser = JsParser::default();
        let code = "const x = 42;";

        let start = Instant::now();
        for _ in 0..100 {
            let _ = parser.parse_str(code);
        }
        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 1000,
            "100 small parses should be under 1 second, took {:?}",
            duration
        );
    }

    #[test]
    fn test_parse_performance_medium() {
        let parser = JsParser::default();
        let mut code = String::new();
        for i in 0..50 {
            code.push_str(&format!("const var_{} = {};\n", i, i));
        }

        let start = Instant::now();
        for _ in 0..50 {
            let _ = parser.parse_str(&code);
        }
        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 2000,
            "50 medium parses should be under 2 seconds, took {:?}",
            duration
        );
    }

    #[test]
    fn test_parse_performance_large() {
        let parser = JsParser::default();
        let mut code = String::new();
        for i in 0..200 {
            code.push_str(&format!("const var_{} = {};\n", i, i));
            code.push_str(&format!("function fn_{}() {{ return {}; }}\n", i, i));
        }

        let start = Instant::now();
        for _ in 0..10 {
            let _ = parser.parse_str(&code);
        }
        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 5000,
            "10 large parses should be under 5 seconds, took {:?}",
            duration
        );
    }
}

// =============================================================================
// Regression Tests
// =============================================================================

mod regression_tests {
    use super::*;

    /// Tests that all fixture files in the ecma directory parse successfully
    /// (except for known malformed files)
    #[test]
    fn test_all_fixtures_parse() {
        let malformed_files = vec!["malformed.js", "syntax_error.js"];

        let fixtures = std::fs::read_dir(fixtures_dir())
            .expect("Failed to read fixtures directory");

        let js_parser = JsParser::default();
        let ts_parser = TsParser::default();
        let tsx_parser = TsxParser::default();

        for entry in fixtures {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let filename = path.file_name().unwrap().to_str().unwrap();

            // Skip known malformed files
            if malformed_files.iter().any(|f| filename == *f) {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let result = match ext {
                "ts" => ts_parser.parse_file(&path),
                "tsx" | "jsx" => tsx_parser.parse_file(&path),
                _ => js_parser.parse_file(&path),
            };

            assert!(
                result.is_ok(),
                "Failed to parse {}: {:?}",
                filename,
                result.err()
            );
        }
    }

    /// Tests that parsing is consistent across multiple runs
    #[test]
    fn test_consistent_parsing() {
        let parser = JsParser::default();
        let code = r#"
            export const a = 1;
            export function foo() {}
        "#;

        // Parse multiple times and verify consistency
        for _ in 0..5 {
            let result = parser.parse_str(code);
            assert!(result.is_ok());

            let parsed = result.unwrap();
            let exports = parsed.exports();
            let functions = parsed.functions();

            assert!(!exports.is_empty());
            assert!(!functions.is_empty());
        }
    }

    /// Tests backward compatibility with various ES versions
    #[test]
    fn test_es_version_compatibility() {
        let parser = JsParser::default();

        // ES5-style code
        let es5 = "var x = 1; function fn() { return arguments[0]; }";
        assert!(parser.parse_str(es5).is_ok());

        // ES6+ code
        let es6 = "const x = 1; const fn = (...args) => args[0]; class C {}";
        assert!(parser.parse_str(es6).is_ok());

        // ESNext code
        let esnext = "const x = 1n; const y = obj?.prop ?? 'default'; class C { #private = 1; }";
        assert!(parser.parse_str(esnext).is_ok());
    }
}

// =============================================================================
// NodeKind Tests
// =============================================================================

mod node_kind_tests {
    use super::*;

    #[test]
    fn test_node_kind_is_export() {
        assert!(NodeKind::ExportStatement.is_export());
        assert!(NodeKind::ExportClause.is_export());
        assert!(NodeKind::ExportSpecifier.is_export());
        assert!(!NodeKind::FunctionDeclaration.is_export());
    }

    #[test]
    fn test_node_kind_is_import() {
        assert!(NodeKind::ImportStatement.is_import());
        assert!(NodeKind::ImportClause.is_import());
        assert!(NodeKind::ImportSpecifier.is_import());
        assert!(!NodeKind::FunctionDeclaration.is_import());
    }

    #[test]
    fn test_node_kind_is_declaration() {
        assert!(NodeKind::FunctionDeclaration.is_declaration());
        assert!(NodeKind::ClassDeclaration.is_declaration());
        assert!(NodeKind::VariableDeclaration.is_declaration());
        assert!(NodeKind::InterfaceDeclaration.is_declaration());
        assert!(!NodeKind::Identifier.is_declaration());
    }

    #[test]
    fn test_node_kind_is_typescript() {
        assert!(NodeKind::InterfaceDeclaration.is_typescript());
        assert!(NodeKind::TypeAliasDeclaration.is_typescript());
        assert!(NodeKind::EnumDeclaration.is_typescript());
        assert!(NodeKind::TypeAnnotation.is_typescript());
        assert!(!NodeKind::FunctionDeclaration.is_typescript());
    }
}

// =============================================================================
// ParsedNode Helper Method Tests
// =============================================================================

mod parsed_node_tests {
    use super::*;

    #[test]
    fn test_find_all() {
        let parser = JsParser::default();
        let source = r#"
            function foo() {}
            function bar() {}
            const baz = 1;
        "#;

        let parsed = parser.parse_str(source).unwrap();
        let functions = parsed.find_all(|n| n.kind == NodeKind::FunctionDeclaration);
        assert!(functions.len() >= 2);
    }

    #[test]
    fn test_find_first() {
        let parser = JsParser::default();
        let source = "function first() {} function second() {}";

        let parsed = parser.parse_str(source).unwrap();
        let first = parsed.find_first(|n| n.kind == NodeKind::FunctionDeclaration);
        assert!(first.is_some());
    }

    #[test]
    fn test_exports_method() {
        let parser = JsParser::default();
        let source = "export const a = 1; export const b = 2;";

        let parsed = parser.parse_str(source).unwrap();
        let exports = parsed.exports();
        assert!(!exports.is_empty());
    }

    #[test]
    fn test_functions_method() {
        let parser = JsParser::default();
        let source = "function foo() {} function bar() {}";

        let parsed = parser.parse_str(source).unwrap();
        let functions = parsed.functions();
        assert!(!functions.is_empty());
    }

    #[test]
    fn test_classes_method() {
        let parser = JsParser::default();
        let source = "class A {} class B {}";

        let parsed = parser.parse_str(source).unwrap();
        let classes = parsed.classes();
        assert!(!classes.is_empty());
    }

    #[test]
    fn test_variables_method() {
        let parser = JsParser::default();
        let source = "const a = 1; let b = 2; var c = 3;";

        let parsed = parser.parse_str(source).unwrap();
        let variables = parsed.variables();
        assert!(!variables.is_empty());
    }

    #[test]
    fn test_imports_method() {
        let parser = JsParser::default();
        let source = "import { a } from './a'; import b from './b';";

        let parsed = parser.parse_str(source).unwrap();
        let imports = parsed.imports();
        assert!(!imports.is_empty());
    }
}
