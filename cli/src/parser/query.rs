//! Query patterns for extracting specific nodes from parsed trees.

use super::location::SourceRange;
use super::node::NodeKind;

/// A pattern for querying nodes in a parsed tree.
#[derive(Debug, Clone)]
pub struct QueryPattern {
    /// The tree-sitter query string.
    pub query_string: String,
    /// Names of captures to extract.
    pub capture_names: Vec<String>,
}

impl QueryPattern {
    /// Creates a new query pattern.
    pub fn new(query_string: impl Into<String>) -> Self {
        Self {
            query_string: query_string.into(),
            capture_names: Vec::new(),
        }
    }

    /// Adds a capture name to track.
    pub fn with_capture(mut self, name: impl Into<String>) -> Self {
        self.capture_names.push(name.into());
        self
    }

    /// Creates a query for all function declarations.
    pub fn functions() -> Self {
        Self::new(
            r#"
            (function_declaration
                name: (identifier) @name) @function
            "#,
        )
        .with_capture("name")
        .with_capture("function")
    }

    /// Creates a query for all class declarations.
    pub fn classes() -> Self {
        Self::new(
            r#"
            (class_declaration
                name: (identifier) @name) @class
            "#,
        )
        .with_capture("name")
        .with_capture("class")
    }

    /// Creates a query for exported declarations.
    pub fn exports() -> Self {
        Self::new(
            r#"
            (export_statement) @export
            "#,
        )
        .with_capture("export")
    }

    /// Creates a query for import statements.
    pub fn imports() -> Self {
        Self::new(
            r#"
            (import_statement
                source: (string) @source) @import
            "#,
        )
        .with_capture("source")
        .with_capture("import")
    }

    /// Creates a query for variable declarations.
    pub fn variables() -> Self {
        Self::new(
            r#"
            [
                (variable_declaration
                    (variable_declarator
                        name: (identifier) @name)) @var
                (lexical_declaration
                    (variable_declarator
                        name: (identifier) @name)) @var
            ]
            "#,
        )
        .with_capture("name")
        .with_capture("var")
    }

    /// Creates a query for TypeScript interfaces.
    pub fn interfaces() -> Self {
        Self::new(
            r#"
            (interface_declaration
                name: (type_identifier) @name) @interface
            "#,
        )
        .with_capture("name")
        .with_capture("interface")
    }

    /// Creates a query for TypeScript type aliases.
    pub fn type_aliases() -> Self {
        Self::new(
            r#"
            (type_alias_declaration
                name: (type_identifier) @name) @type_alias
            "#,
        )
        .with_capture("name")
        .with_capture("type_alias")
    }

    /// Creates a query for TypeScript enums.
    pub fn enums() -> Self {
        Self::new(
            r#"
            (enum_declaration
                name: (identifier) @name) @enum
            "#,
        )
        .with_capture("name")
        .with_capture("enum")
    }
}

/// A single match from a query execution.
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// The kind of matched node.
    pub kind: NodeKind,
    /// The source range of the match.
    pub range: SourceRange,
    /// The text content of the match.
    pub text: String,
    /// Named captures from the query.
    pub captures: Vec<QueryCapture>,
}

impl QueryMatch {
    /// Creates a new query match.
    pub fn new(kind: NodeKind, range: SourceRange, text: String) -> Self {
        Self {
            kind,
            range,
            text,
            captures: Vec::new(),
        }
    }

    /// Adds a capture to this match.
    pub fn with_capture(mut self, capture: QueryCapture) -> Self {
        self.captures.push(capture);
        self
    }

    /// Gets a capture by name.
    pub fn get_capture(&self, name: &str) -> Option<&QueryCapture> {
        self.captures.iter().find(|c| c.name == name)
    }

    /// Gets the text of a capture by name.
    pub fn get_capture_text(&self, name: &str) -> Option<&str> {
        self.get_capture(name).map(|c| c.text.as_str())
    }
}

/// A named capture from a query match.
#[derive(Debug, Clone)]
pub struct QueryCapture {
    /// The capture name from the query.
    pub name: String,
    /// The kind of captured node.
    pub kind: NodeKind,
    /// The source range of the capture.
    pub range: SourceRange,
    /// The text content of the capture.
    pub text: String,
}

impl QueryCapture {
    /// Creates a new query capture.
    pub fn new(name: impl Into<String>, kind: NodeKind, range: SourceRange, text: String) -> Self {
        Self {
            name: name.into(),
            kind,
            range,
            text,
        }
    }
}

/// A builder for constructing complex queries.
#[derive(Debug, Clone)]
pub struct NodeQuery {
    patterns: Vec<QueryPattern>,
}

impl NodeQuery {
    /// Creates a new node query.
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Adds a pattern to this query.
    pub fn with_pattern(mut self, pattern: QueryPattern) -> Self {
        self.patterns.push(pattern);
        self
    }

    /// Builds the combined query string.
    pub fn build(&self) -> String {
        self.patterns
            .iter()
            .map(|p| p.query_string.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Returns all capture names.
    pub fn capture_names(&self) -> Vec<&str> {
        self.patterns
            .iter()
            .flat_map(|p| p.capture_names.iter().map(|s| s.as_str()))
            .collect()
    }
}

impl Default for NodeQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_pattern_functions() {
        let pattern = QueryPattern::functions();
        assert!(pattern.query_string.contains("function_declaration"));
        assert!(pattern.capture_names.contains(&"name".to_string()));
        assert!(pattern.capture_names.contains(&"function".to_string()));
    }

    #[test]
    fn test_query_pattern_exports() {
        let pattern = QueryPattern::exports();
        assert!(pattern.query_string.contains("export_statement"));
    }

    #[test]
    fn test_node_query_builder() {
        let query = NodeQuery::new()
            .with_pattern(QueryPattern::functions())
            .with_pattern(QueryPattern::classes());

        let query_string = query.build();
        assert!(query_string.contains("function_declaration"));
        assert!(query_string.contains("class_declaration"));
    }
}
