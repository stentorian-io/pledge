//! ECMAScript/TypeScript parsing using tree-sitter.
//!
//! This module provides parsing functionality for JavaScript and TypeScript
//! files using tree-sitter as the underlying parser.

use crate::fs::file::FileExt;
use crate::parser::{
    JsParser, NodeKind, ParseResult, ParsedNode, SourceParser, TsParser, TsxParser,
};
use crate::pkg::entries::PkgEntry;
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Parses a package entry file and returns the parsed AST.
pub fn parse_pkg_entry(entry: &PkgEntry) -> Result<ParsedNode> {
    let entry_path = &entry.path;

    let file_data = match entry.load_file()? {
        Some(data) => {
            let content = String::from_utf8(data)?;
            println!("    -> Successfully loaded file: {} ({} bytes)", entry_path.display(), content.len());
            content
        }
        None => bail!("Entry module '{}' does not exist.", entry_path.display()),
    };

    parse_source(&file_data, entry_path).with_context(|| {
        format!(
            "Failed to parse package entry module: {}",
            entry_path.display()
        )
    })
}

/// Parses an imported file and returns the parsed AST.
pub fn parse_import(file_path: &Path) -> Result<ParsedNode> {
    let source = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    parse_source(&source, file_path)
}

/// Parses source code with automatic language detection based on file extension.
pub fn parse_source(source: &str, file_path: &Path) -> Result<ParsedNode> {
    let file_ext = FileExt::from(file_path);

    let parsed = if file_ext.is_jsx() {
        // JSX/TSX files need the TSX parser
        let parser = TsxParser::new().map_err(|e| anyhow::anyhow!("{}", e))?;
        parser.parse_str(source).map_err(|e| anyhow::anyhow!("{}", e))?
    } else if file_ext.is_ts() {
        // TypeScript files
        let parser = TsParser::new().map_err(|e| anyhow::anyhow!("{}", e))?;
        parser.parse_str(source).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        // JavaScript files (default)
        let parser = JsParser::new().map_err(|e| anyhow::anyhow!("{}", e))?;
        parser.parse_str(source).map_err(|e| anyhow::anyhow!("{}", e))?
    };

    Ok(parsed)
}

/// Parses source code as JavaScript.
pub fn parse_js(source: &str) -> ParseResult<ParsedNode> {
    let parser = JsParser::new()?;
    parser.parse_str(source)
}

/// Parses source code as TypeScript.
pub fn parse_ts(source: &str) -> ParseResult<ParsedNode> {
    let parser = TsParser::new()?;
    parser.parse_str(source)
}

/// Parses source code as TSX.
pub fn parse_tsx(source: &str) -> ParseResult<ParsedNode> {
    let parser = TsxParser::new()?;
    parser.parse_str(source)
}

/// Extracts all exports from a parsed module.
pub fn get_exports(parsed: &ParsedNode) -> Vec<&ParsedNode> {
    parsed.exports()
}

/// Extracts all function declarations from a parsed module.
pub fn get_functions(parsed: &ParsedNode) -> Vec<&ParsedNode> {
    parsed.functions()
}

/// Extracts all class declarations from a parsed module.
pub fn get_classes(parsed: &ParsedNode) -> Vec<&ParsedNode> {
    parsed.classes()
}

/// Extracts all variable declarations from a parsed module.
pub fn get_variables(parsed: &ParsedNode) -> Vec<&ParsedNode> {
    parsed.variables()
}

/// Extracts all import statements from a parsed module.
pub fn get_imports(parsed: &ParsedNode) -> Vec<&ParsedNode> {
    parsed.imports()
}

/// Checks if the module has any parse errors.
pub fn has_errors(parsed: &ParsedNode) -> bool {
    parsed.find_first(|n| n.kind == NodeKind::Error).is_some()
}

/// Collects all error nodes from the parsed tree.
pub fn get_errors(parsed: &ParsedNode) -> Vec<&ParsedNode> {
    parsed.find_all(|n| n.kind == NodeKind::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_simple_js() {
        let source = "const x = 42;";
        let path = PathBuf::from("test.js");
        let result = parse_source(source, &path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_function() {
        let source = r#"
            function greet(name) {
                return `Hello, ${name}!`;
            }
        "#;
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();
        let functions = get_functions(&parsed);
        assert_eq!(functions.len(), 1);
    }

    #[test]
    fn test_parse_export() {
        let source = "export const PI = 3.14159;";
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();
        let exports = get_exports(&parsed);
        assert!(!exports.is_empty());
    }

    #[test]
    fn test_parse_typescript() {
        let source = r#"
            interface User {
                id: number;
                name: string;
            }
            const user: User = { id: 1, name: "Test" };
        "#;
        let path = PathBuf::from("test.ts");
        let result = parse_source(source, &path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_tsx() {
        let source = r#"
            function Button({ label }: { label: string }) {
                return <button>{label}</button>;
            }
        "#;
        let path = PathBuf::from("test.tsx");
        let result = parse_source(source, &path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_recovery() {
        // Tree-sitter handles partial/incomplete code gracefully
        let source = "const x = ";
        let path = PathBuf::from("test.js");
        let result = parse_source(source, &path);
        assert!(result.is_ok());
        // The tree should have error nodes
        let parsed = result.unwrap();
        assert!(has_errors(&parsed));
    }

    #[test]
    fn test_parse_class() {
        let source = r#"
            export class Calculator {
                add(a, b) {
                    return a + b;
                }
            }
        "#;
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();
        let classes = get_classes(&parsed);
        assert_eq!(classes.len(), 1);
    }

    #[test]
    fn test_parse_imports() {
        let source = r#"
            import { foo, bar } from './utils';
            import * as helpers from './helpers';
        "#;
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();
        let imports = get_imports(&parsed);
        assert_eq!(imports.len(), 2);
    }
}
