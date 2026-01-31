//! Parser traits for language-agnostic parsing.

use super::error::ParseError;
use super::node::ParsedNode;
use super::query::{QueryMatch, QueryPattern};
use std::path::Path;

/// Result type for parsing operations.
pub type ParseResult<T> = Result<T, ParseError>;

/// Trait for language-specific parsers.
pub trait LanguageParser: Send + Sync {
    /// Returns the language name (e.g., "javascript", "typescript").
    fn language_name(&self) -> &str;

    /// Returns the tree-sitter language configuration.
    fn tree_sitter_language(&self) -> tree_sitter::Language;

    /// Returns the file extensions supported by this parser.
    fn file_extensions(&self) -> &[&str];

    /// Checks if this parser supports a given file extension.
    fn supports_extension(&self, ext: &str) -> bool {
        self.file_extensions().iter().any(|e| *e == ext)
    }

    /// Checks if this parser supports a given file path.
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| self.supports_extension(ext))
            .unwrap_or(false)
    }
}

/// Trait for parsing source code.
pub trait SourceParser: LanguageParser {
    /// Parses source code from a string.
    fn parse_str(&self, source: &str) -> ParseResult<ParsedNode>;

    /// Parses source code from a file.
    fn parse_file(&self, path: &Path) -> ParseResult<ParsedNode> {
        let source = std::fs::read_to_string(path)?;
        let mut result = self.parse_str(&source)?;
        // Store file path info if needed
        result.text = path.display().to_string();
        Ok(result)
    }

    /// Parses source code from bytes.
    fn parse_bytes(&self, source: &[u8]) -> ParseResult<ParsedNode> {
        let source_str = std::str::from_utf8(source).map_err(|e| {
            ParseError::new(
                super::error::ParseErrorKind::ParseFailed,
                format!("Invalid UTF-8: {}", e),
            )
        })?;
        self.parse_str(source_str)
    }

    /// Executes a query pattern against parsed source code.
    fn query(&self, source: &str, pattern: &QueryPattern) -> ParseResult<Vec<QueryMatch>>;

    /// Executes a query pattern against a file.
    fn query_file(&self, path: &Path, pattern: &QueryPattern) -> ParseResult<Vec<QueryMatch>> {
        let source = std::fs::read_to_string(path)?;
        self.query(&source, pattern)
    }

    /// Incrementally updates a parse tree with changes.
    fn parse_incremental(
        &self,
        source: &str,
        old_tree: Option<&tree_sitter::Tree>,
    ) -> ParseResult<tree_sitter::Tree>;
}

/// A parser registry that selects the appropriate parser based on file extension.
pub struct ParserRegistry {
    parsers: Vec<Box<dyn SourceParser>>,
}

impl ParserRegistry {
    /// Creates a new empty parser registry.
    pub fn new() -> Self {
        Self {
            parsers: Vec::new(),
        }
    }

    /// Registers a parser.
    pub fn register<P: SourceParser + 'static>(mut self, parser: P) -> Self {
        self.parsers.push(Box::new(parser));
        self
    }

    /// Gets a parser for a given file path.
    pub fn get_parser(&self, path: &Path) -> Option<&dyn SourceParser> {
        self.parsers
            .iter()
            .find(|p| p.supports_path(path))
            .map(|p| p.as_ref())
    }

    /// Gets a parser for a given file extension.
    pub fn get_parser_for_extension(&self, ext: &str) -> Option<&dyn SourceParser> {
        self.parsers
            .iter()
            .find(|p| p.supports_extension(ext))
            .map(|p| p.as_ref())
    }

    /// Parses a file using the appropriate parser.
    pub fn parse_file(&self, path: &Path) -> ParseResult<ParsedNode> {
        self.get_parser(path)
            .ok_or_else(|| {
                ParseError::unsupported_language(
                    path.extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("unknown"),
                )
            })?
            .parse_file(path)
    }

    /// Lists all registered parsers.
    pub fn parsers(&self) -> impl Iterator<Item = &dyn SourceParser> {
        self.parsers.iter().map(|p| p.as_ref())
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Mock parser for testing
    struct MockParser {
        extensions: Vec<&'static str>,
    }

    impl LanguageParser for MockParser {
        fn language_name(&self) -> &str {
            "mock"
        }

        fn tree_sitter_language(&self) -> tree_sitter::Language {
            tree_sitter_javascript::LANGUAGE.into()
        }

        fn file_extensions(&self) -> &[&str] {
            &self.extensions
        }
    }

    impl SourceParser for MockParser {
        fn parse_str(&self, _source: &str) -> ParseResult<ParsedNode> {
            Ok(ParsedNode::new(
                super::super::node::NodeKind::Program,
                super::super::location::SourceRange::default(),
                String::new(),
            ))
        }

        fn query(&self, _source: &str, _pattern: &QueryPattern) -> ParseResult<Vec<QueryMatch>> {
            Ok(Vec::new())
        }

        fn parse_incremental(
            &self,
            source: &str,
            _old_tree: Option<&tree_sitter::Tree>,
        ) -> ParseResult<tree_sitter::Tree> {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&self.tree_sitter_language())
                .map_err(|e| {
                    ParseError::new(
                        super::super::error::ParseErrorKind::ParseFailed,
                        e.to_string(),
                    )
                })?;
            parser.parse(source, None).ok_or_else(|| {
                ParseError::new(
                    super::super::error::ParseErrorKind::ParseFailed,
                    "Failed to parse",
                )
            })
        }
    }

    #[test]
    fn test_parser_registry() {
        let registry = ParserRegistry::new().register(MockParser {
            extensions: vec!["js", "jsx"],
        });

        assert!(registry
            .get_parser(&PathBuf::from("test.js"))
            .is_some());
        assert!(registry
            .get_parser(&PathBuf::from("test.jsx"))
            .is_some());
        assert!(registry
            .get_parser(&PathBuf::from("test.ts"))
            .is_none());
    }

    #[test]
    fn test_supports_extension() {
        let parser = MockParser {
            extensions: vec!["js", "jsx"],
        };

        assert!(parser.supports_extension("js"));
        assert!(parser.supports_extension("jsx"));
        assert!(!parser.supports_extension("ts"));
    }
}
