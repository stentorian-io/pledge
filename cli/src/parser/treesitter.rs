//! Tree-sitter based parser implementations for JavaScript and TypeScript.

use super::error::{ParseError, ParseErrorKind};
use super::location::SourceRange;
use super::node::{NodeKind, ParsedNode};
use super::query::{QueryCapture, QueryMatch, QueryPattern};
use super::traits::{LanguageParser, ParseResult, SourceParser};
use std::sync::RwLock;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, Query, QueryCursor, Tree};

/// Base tree-sitter parser implementation.
pub struct TreeSitterParser {
    language: tree_sitter::Language,
    language_name: String,
    extensions: Vec<&'static str>,
    parser: RwLock<Parser>,
}

impl TreeSitterParser {
    /// Creates a new tree-sitter parser for the given language.
    fn new(
        language: tree_sitter::Language,
        language_name: impl Into<String>,
        extensions: Vec<&'static str>,
    ) -> ParseResult<Self> {
        let mut parser = Parser::new();
        parser.set_language(&language).map_err(|e| {
            ParseError::new(
                ParseErrorKind::ParseFailed,
                format!("Failed to set language: {}", e),
            )
        })?;

        Ok(Self {
            language,
            language_name: language_name.into(),
            extensions,
            parser: RwLock::new(parser),
        })
    }

    /// Parses source code and returns the raw tree-sitter tree.
    fn parse_to_tree(&self, source: &str, old_tree: Option<&Tree>) -> ParseResult<Tree> {
        self.parser
            .write()
            .map_err(|_| ParseError::new(ParseErrorKind::ParseFailed, "Failed to acquire parser lock"))?
            .parse(source, old_tree)
            .ok_or_else(|| {
                ParseError::new(ParseErrorKind::ParseFailed, "Failed to parse source code")
            })
    }

    /// Converts a tree-sitter tree to our ParsedNode representation.
    fn tree_to_parsed_node(&self, tree: &Tree, source: &str) -> ParsedNode {
        self.node_to_parsed_node(&tree.root_node(), source, false, false)
    }

    /// Recursively converts a tree-sitter node to a ParsedNode.
    fn node_to_parsed_node(
        &self,
        node: &Node,
        source: &str,
        is_exported: bool,
        is_default: bool,
    ) -> ParsedNode {
        let kind = NodeKind::from_ts_kind(node.kind());
        let range = SourceRange::from_node(node);
        let text = self.get_node_text(node, source);

        let mut parsed = ParsedNode::new(kind, range, text);

        // Extract name from identifiers and declarations
        if let Some(name) = self.extract_name(node, source) {
            parsed = parsed.with_name(name);
        }

        // Check for export status
        let (node_is_exported, node_is_default) = if kind == NodeKind::ExportStatement {
            let has_default = self.has_default_keyword(node, source);
            (true, has_default)
        } else {
            (is_exported, is_default)
        };

        if node_is_exported {
            parsed = parsed.with_export(node_is_default);
        }

        // Extract modifiers (async, static, etc.)
        parsed.modifiers = self.extract_modifiers(node, source);

        // Process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // Skip anonymous nodes for cleaner output
            if child.is_named() {
                let child_parsed =
                    self.node_to_parsed_node(&child, source, node_is_exported, node_is_default);
                parsed = parsed.with_child(child_parsed);
            }
        }

        parsed
    }

    /// Extracts the text content of a node.
    fn get_node_text(&self, node: &Node, source: &str) -> String {
        source[node.start_byte()..node.end_byte()].to_string()
    }

    /// Extracts the name from a node if it has one.
    fn extract_name(&self, node: &Node, source: &str) -> Option<String> {
        let kind = node.kind();

        // Direct identifier nodes
        if kind == "identifier" || kind == "property_identifier" || kind == "type_identifier" {
            return Some(self.get_node_text(node, source));
        }

        // For declarations, find the name child
        if kind.ends_with("_declaration") || kind == "class" || kind == "function" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let child_kind = child.kind();
                if child_kind == "identifier"
                    || child_kind == "type_identifier"
                    || child_kind == "property_identifier"
                {
                    // Skip if it's the "name" field
                    if node.field_name_for_child(child.id() as u32) == Some("name") {
                        return Some(self.get_node_text(&child, source));
                    }
                }
            }

            // Try field access if no direct name found
            if let Some(name_node) = node.child_by_field_name("name") {
                return Some(self.get_node_text(&name_node, source));
            }
        }

        // For variable declarators
        if kind == "variable_declarator" {
            if let Some(name_node) = node.child_by_field_name("name") {
                if name_node.kind() == "identifier" {
                    return Some(self.get_node_text(&name_node, source));
                }
            }
        }

        None
    }

    /// Checks if an export statement has the "default" keyword.
    fn has_default_keyword(&self, node: &Node, source: &str) -> bool {
        let text = self.get_node_text(node, source);
        text.starts_with("export default")
    }

    /// Extracts modifiers from a node (async, static, etc.).
    fn extract_modifiers(&self, node: &Node, source: &str) -> Vec<String> {
        let mut modifiers = Vec::new();
        let text = self.get_node_text(node, source);

        // Check for common modifiers in the text
        if text.contains("async ") {
            modifiers.push("async".to_string());
        }
        if text.contains("static ") {
            modifiers.push("static".to_string());
        }
        if text.contains("readonly ") {
            modifiers.push("readonly".to_string());
        }
        if text.contains("private ") {
            modifiers.push("private".to_string());
        }
        if text.contains("protected ") {
            modifiers.push("protected".to_string());
        }
        if text.contains("public ") {
            modifiers.push("public".to_string());
        }

        modifiers
    }

    /// Checks if the parsed tree has errors.
    fn has_errors(&self, tree: &Tree) -> bool {
        tree.root_node().has_error()
    }

    /// Collects all error nodes from the tree.
    fn collect_errors(&self, tree: &Tree, source: &str) -> Vec<ParseError> {
        let mut errors = Vec::new();
        self.collect_errors_recursive(&tree.root_node(), source, &mut errors);
        errors
    }

    fn collect_errors_recursive(&self, node: &Node, source: &str, errors: &mut Vec<ParseError>) {
        if node.is_error() {
            let range = SourceRange::from_node(node);
            let text = self.get_node_text(node, source);
            let error = ParseError::syntax_error(format!("Unexpected token: '{}'", text))
                .with_location(range);
            errors.push(error);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_errors_recursive(&child, source, errors);
        }
    }
}

impl LanguageParser for TreeSitterParser {
    fn language_name(&self) -> &str {
        &self.language_name
    }

    fn tree_sitter_language(&self) -> tree_sitter::Language {
        self.language.clone()
    }

    fn file_extensions(&self) -> &[&str] {
        &self.extensions
    }
}

impl SourceParser for TreeSitterParser {
    fn parse_str(&self, source: &str) -> ParseResult<ParsedNode> {
        let tree = self.parse_to_tree(source, None)?;
        Ok(self.tree_to_parsed_node(&tree, source))
    }

    fn query(&self, source: &str, pattern: &QueryPattern) -> ParseResult<Vec<QueryMatch>> {
        let tree = self.parse_to_tree(source, None)?;

        let query =
            Query::new(&self.language, &pattern.query_string).map_err(|e| {
                ParseError::invalid_query(format!("Invalid query pattern: {}", e))
            })?;

        let mut cursor = QueryCursor::new();
        let source_bytes = source.as_bytes();

        let mut results = Vec::new();
        // tree-sitter 0.24: use captures with streaming iterator
        let mut captures = cursor.captures(&query, tree.root_node(), source_bytes);
        while let Some((qm, _idx)) = captures.next() {
            for capture in qm.captures.iter() {
                let node = capture.node;
                let kind = NodeKind::from_ts_kind(node.kind());
                let range = SourceRange::from_node(&node);
                let text = source[node.start_byte()..node.end_byte()].to_string();
                let capture_name = query.capture_names()[capture.index as usize].to_string();

                let query_capture = QueryCapture::new(&capture_name, kind, range, text.clone());
                let query_match = QueryMatch::new(kind, range, text).with_capture(query_capture);
                results.push(query_match);
            }
        }

        Ok(results)
    }

    fn parse_incremental(
        &self,
        source: &str,
        old_tree: Option<&Tree>,
    ) -> ParseResult<Tree> {
        self.parse_to_tree(source, old_tree)
    }
}

/// JavaScript parser using tree-sitter.
pub struct JsParser(TreeSitterParser);

impl JsParser {
    /// Creates a new JavaScript parser.
    pub fn new() -> ParseResult<Self> {
        let parser = TreeSitterParser::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            vec!["js", "mjs", "cjs"],
        )?;
        Ok(Self(parser))
    }
}

impl Default for JsParser {
    fn default() -> Self {
        Self::new().expect("Failed to create JavaScript parser")
    }
}

impl LanguageParser for JsParser {
    fn language_name(&self) -> &str {
        self.0.language_name()
    }

    fn tree_sitter_language(&self) -> tree_sitter::Language {
        self.0.tree_sitter_language()
    }

    fn file_extensions(&self) -> &[&str] {
        self.0.file_extensions()
    }
}

impl SourceParser for JsParser {
    fn parse_str(&self, source: &str) -> ParseResult<ParsedNode> {
        self.0.parse_str(source)
    }

    fn query(&self, source: &str, pattern: &QueryPattern) -> ParseResult<Vec<QueryMatch>> {
        self.0.query(source, pattern)
    }

    fn parse_incremental(
        &self,
        source: &str,
        old_tree: Option<&Tree>,
    ) -> ParseResult<Tree> {
        self.0.parse_incremental(source, old_tree)
    }
}

/// TypeScript parser using tree-sitter.
pub struct TsParser(TreeSitterParser);

impl TsParser {
    /// Creates a new TypeScript parser.
    pub fn new() -> ParseResult<Self> {
        let parser = TreeSitterParser::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            vec!["ts", "mts", "cts"],
        )?;
        Ok(Self(parser))
    }
}

impl Default for TsParser {
    fn default() -> Self {
        Self::new().expect("Failed to create TypeScript parser")
    }
}

impl LanguageParser for TsParser {
    fn language_name(&self) -> &str {
        self.0.language_name()
    }

    fn tree_sitter_language(&self) -> tree_sitter::Language {
        self.0.tree_sitter_language()
    }

    fn file_extensions(&self) -> &[&str] {
        self.0.file_extensions()
    }
}

impl SourceParser for TsParser {
    fn parse_str(&self, source: &str) -> ParseResult<ParsedNode> {
        self.0.parse_str(source)
    }

    fn query(&self, source: &str, pattern: &QueryPattern) -> ParseResult<Vec<QueryMatch>> {
        self.0.query(source, pattern)
    }

    fn parse_incremental(
        &self,
        source: &str,
        old_tree: Option<&Tree>,
    ) -> ParseResult<Tree> {
        self.0.parse_incremental(source, old_tree)
    }
}

/// TSX parser using tree-sitter.
pub struct TsxParser(TreeSitterParser);

impl TsxParser {
    /// Creates a new TSX parser.
    pub fn new() -> ParseResult<Self> {
        let parser = TreeSitterParser::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            vec!["tsx", "jsx"],
        )?;
        Ok(Self(parser))
    }
}

impl Default for TsxParser {
    fn default() -> Self {
        Self::new().expect("Failed to create TSX parser")
    }
}

impl LanguageParser for TsxParser {
    fn language_name(&self) -> &str {
        self.0.language_name()
    }

    fn tree_sitter_language(&self) -> tree_sitter::Language {
        self.0.tree_sitter_language()
    }

    fn file_extensions(&self) -> &[&str] {
        self.0.file_extensions()
    }
}

impl SourceParser for TsxParser {
    fn parse_str(&self, source: &str) -> ParseResult<ParsedNode> {
        self.0.parse_str(source)
    }

    fn query(&self, source: &str, pattern: &QueryPattern) -> ParseResult<Vec<QueryMatch>> {
        self.0.query(source, pattern)
    }

    fn parse_incremental(
        &self,
        source: &str,
        old_tree: Option<&Tree>,
    ) -> ParseResult<Tree> {
        self.0.parse_incremental(source, old_tree)
    }
}

/// Creates a parser registry with all JavaScript/TypeScript parsers registered.
pub fn create_js_ts_registry() -> super::traits::ParserRegistry {
    super::traits::ParserRegistry::new()
        .register(JsParser::default())
        .register(TsParser::default())
        .register(TsxParser::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_js_parser_simple() {
        let parser = JsParser::new().unwrap();
        let source = "const x = 42;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_js_parser_function() {
        let parser = JsParser::new().unwrap();
        let source = r#"
            function greet(name) {
                return `Hello, ${name}!`;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        let functions = parsed.functions();
        assert_eq!(functions.len(), 1);
    }

    #[test]
    fn test_js_parser_export() {
        let parser = JsParser::new().unwrap();
        let source = "export const PI = 3.14159;";
        let result = parser.parse_str(source);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        let exports = parsed.exports();
        assert!(!exports.is_empty());
    }

    #[test]
    fn test_ts_parser_types() {
        let parser = TsParser::new().unwrap();
        let source = r#"
            interface User {
                id: number;
                name: string;
            }

            const user: User = { id: 1, name: "Test" };
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tsx_parser_jsx() {
        let parser = TsxParser::new().unwrap();
        let source = r#"
            function Button({ label }: { label: string }) {
                return <button>{label}</button>;
            }
        "#;
        let result = parser.parse_str(source);
        assert!(result.is_ok());
    }

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
        // Arrow functions are not captured by this query
        assert!(matches.len() >= 2);
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
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_parser_error_recovery() {
        let parser = JsParser::new().unwrap();
        // Partial/incomplete code - tree-sitter handles this gracefully
        let source = "const x = ";
        let result = parser.parse_str(source);
        // Tree-sitter produces a tree even with errors
        assert!(result.is_ok());
    }

    #[test]
    fn test_parser_registry() {
        let registry = create_js_ts_registry();

        assert!(registry.get_parser_for_extension("js").is_some());
        assert!(registry.get_parser_for_extension("ts").is_some());
        assert!(registry.get_parser_for_extension("tsx").is_some());
        assert!(registry.get_parser_for_extension("rs").is_none());
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
