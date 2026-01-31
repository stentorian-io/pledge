//! Parsed node representation.

use super::location::SourceRange;

/// Kinds of parsed nodes in JavaScript/TypeScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    // Program structure
    Program,
    Module,

    // Declarations
    FunctionDeclaration,
    ClassDeclaration,
    VariableDeclaration,
    VariableDeclarator,
    LexicalDeclaration,

    // TypeScript declarations
    InterfaceDeclaration,
    TypeAliasDeclaration,
    EnumDeclaration,
    NamespaceDeclaration,

    // Exports
    ExportStatement,
    ExportClause,
    ExportSpecifier,
    NamespaceExport,

    // Imports
    ImportStatement,
    ImportClause,
    ImportSpecifier,
    NamespaceImport,

    // Expressions
    Identifier,
    PropertyIdentifier,
    StringLiteral,
    NumberLiteral,
    ArrowFunction,
    FunctionExpression,
    ClassExpression,
    CallExpression,
    MemberExpression,
    ObjectExpression,
    ArrayExpression,

    // Statements
    BlockStatement,
    ReturnStatement,
    IfStatement,
    ForStatement,
    WhileStatement,
    TryStatement,
    ThrowStatement,

    // Class members
    MethodDefinition,
    PropertyDefinition,
    Constructor,
    GetAccessor,
    SetAccessor,

    // JSX
    JsxElement,
    JsxSelfClosingElement,
    JsxOpeningElement,
    JsxClosingElement,
    JsxAttribute,
    JsxExpression,

    // Type annotations (TypeScript)
    TypeAnnotation,
    TypeParameter,
    GenericType,

    // Misc
    Comment,
    Error,
    Unknown,
}

impl NodeKind {
    /// Creates a NodeKind from a tree-sitter node kind string.
    pub fn from_ts_kind(kind: &str) -> Self {
        match kind {
            "program" => Self::Program,
            "module" => Self::Module,

            // Declarations
            "function_declaration" | "function" => Self::FunctionDeclaration,
            "generator_function_declaration" => Self::FunctionDeclaration,
            "class_declaration" | "class" => Self::ClassDeclaration,
            "variable_declaration" => Self::VariableDeclaration,
            "variable_declarator" => Self::VariableDeclarator,
            "lexical_declaration" => Self::LexicalDeclaration,

            // TypeScript declarations
            "interface_declaration" => Self::InterfaceDeclaration,
            "type_alias_declaration" => Self::TypeAliasDeclaration,
            "enum_declaration" => Self::EnumDeclaration,
            "namespace_declaration" => Self::NamespaceDeclaration,

            // Exports
            "export_statement" => Self::ExportStatement,
            "export_clause" => Self::ExportClause,
            "export_specifier" => Self::ExportSpecifier,
            "namespace_export" => Self::NamespaceExport,

            // Imports
            "import_statement" => Self::ImportStatement,
            "import_clause" => Self::ImportClause,
            "import_specifier" => Self::ImportSpecifier,
            "namespace_import" => Self::NamespaceImport,

            // Expressions
            "identifier" => Self::Identifier,
            "property_identifier" => Self::PropertyIdentifier,
            "string" | "string_fragment" => Self::StringLiteral,
            "number" => Self::NumberLiteral,
            "arrow_function" => Self::ArrowFunction,
            "function_expression" => Self::FunctionExpression,
            "class_expression" => Self::ClassExpression,
            "call_expression" => Self::CallExpression,
            "member_expression" => Self::MemberExpression,
            "object" => Self::ObjectExpression,
            "array" => Self::ArrayExpression,

            // Statements
            "statement_block" => Self::BlockStatement,
            "return_statement" => Self::ReturnStatement,
            "if_statement" => Self::IfStatement,
            "for_statement" | "for_in_statement" => Self::ForStatement,
            "while_statement" => Self::WhileStatement,
            "try_statement" => Self::TryStatement,
            "throw_statement" => Self::ThrowStatement,

            // Class members
            "method_definition" => Self::MethodDefinition,
            "public_field_definition" | "field_definition" => Self::PropertyDefinition,
            "constructor" => Self::Constructor,
            "get" => Self::GetAccessor,
            "set" => Self::SetAccessor,

            // JSX
            "jsx_element" => Self::JsxElement,
            "jsx_self_closing_element" => Self::JsxSelfClosingElement,
            "jsx_opening_element" => Self::JsxOpeningElement,
            "jsx_closing_element" => Self::JsxClosingElement,
            "jsx_attribute" => Self::JsxAttribute,
            "jsx_expression" => Self::JsxExpression,

            // Type annotations
            "type_annotation" => Self::TypeAnnotation,
            "type_parameter" => Self::TypeParameter,
            "generic_type" => Self::GenericType,

            // Comments
            "comment" | "line_comment" | "block_comment" => Self::Comment,

            // Errors
            "ERROR" => Self::Error,

            _ => Self::Unknown,
        }
    }

    /// Returns true if this is an export-related node.
    pub fn is_export(&self) -> bool {
        matches!(
            self,
            Self::ExportStatement | Self::ExportClause | Self::ExportSpecifier | Self::NamespaceExport
        )
    }

    /// Returns true if this is an import-related node.
    pub fn is_import(&self) -> bool {
        matches!(
            self,
            Self::ImportStatement | Self::ImportClause | Self::ImportSpecifier | Self::NamespaceImport
        )
    }

    /// Returns true if this is a declaration node.
    pub fn is_declaration(&self) -> bool {
        matches!(
            self,
            Self::FunctionDeclaration
                | Self::ClassDeclaration
                | Self::VariableDeclaration
                | Self::VariableDeclarator
                | Self::LexicalDeclaration
                | Self::InterfaceDeclaration
                | Self::TypeAliasDeclaration
                | Self::EnumDeclaration
                | Self::NamespaceDeclaration
        )
    }

    /// Returns true if this is a TypeScript-specific node.
    pub fn is_typescript(&self) -> bool {
        matches!(
            self,
            Self::InterfaceDeclaration
                | Self::TypeAliasDeclaration
                | Self::EnumDeclaration
                | Self::NamespaceDeclaration
                | Self::TypeAnnotation
                | Self::TypeParameter
                | Self::GenericType
        )
    }
}

/// A parsed node with source location and extracted information.
#[derive(Debug, Clone)]
pub struct ParsedNode {
    /// The kind of node.
    pub kind: NodeKind,
    /// The source range of this node.
    pub range: SourceRange,
    /// The raw text content of this node.
    pub text: String,
    /// The name of this node (for declarations, identifiers, etc).
    pub name: Option<String>,
    /// Child nodes.
    pub children: Vec<ParsedNode>,
    /// Whether this node is exported.
    pub is_exported: bool,
    /// Whether this is a default export.
    pub is_default_export: bool,
    /// Additional metadata (e.g., modifiers like async, static, etc).
    pub modifiers: Vec<String>,
}

impl ParsedNode {
    /// Creates a new parsed node.
    pub fn new(kind: NodeKind, range: SourceRange, text: String) -> Self {
        Self {
            kind,
            range,
            text,
            name: None,
            children: Vec::new(),
            is_exported: false,
            is_default_export: false,
            modifiers: Vec::new(),
        }
    }

    /// Sets the name of this node.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Marks this node as exported.
    pub fn with_export(mut self, is_default: bool) -> Self {
        self.is_exported = true;
        self.is_default_export = is_default;
        self
    }

    /// Adds a modifier to this node.
    pub fn with_modifier(mut self, modifier: impl Into<String>) -> Self {
        self.modifiers.push(modifier.into());
        self
    }

    /// Adds a child node.
    pub fn with_child(mut self, child: ParsedNode) -> Self {
        self.children.push(child);
        self
    }

    /// Returns all descendant nodes matching a predicate.
    pub fn find_all<F>(&self, predicate: F) -> Vec<&ParsedNode>
    where
        F: Fn(&ParsedNode) -> bool + Copy,
    {
        let mut results = Vec::new();
        self.find_all_recursive(predicate, &mut results);
        results
    }

    fn find_all_recursive<'a, F>(&'a self, predicate: F, results: &mut Vec<&'a ParsedNode>)
    where
        F: Fn(&ParsedNode) -> bool + Copy,
    {
        if predicate(self) {
            results.push(self);
        }
        for child in &self.children {
            child.find_all_recursive(predicate, results);
        }
    }

    /// Returns the first descendant node matching a predicate.
    pub fn find_first<F>(&self, predicate: F) -> Option<&ParsedNode>
    where
        F: Fn(&ParsedNode) -> bool + Copy,
    {
        if predicate(self) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_first(predicate) {
                return Some(found);
            }
        }
        None
    }

    /// Returns all exported nodes.
    pub fn exports(&self) -> Vec<&ParsedNode> {
        self.find_all(|n| n.is_exported)
    }

    /// Returns all function declarations.
    pub fn functions(&self) -> Vec<&ParsedNode> {
        self.find_all(|n| n.kind == NodeKind::FunctionDeclaration)
    }

    /// Returns all class declarations.
    pub fn classes(&self) -> Vec<&ParsedNode> {
        self.find_all(|n| n.kind == NodeKind::ClassDeclaration)
    }

    /// Returns all variable declarations.
    pub fn variables(&self) -> Vec<&ParsedNode> {
        self.find_all(|n| {
            matches!(
                n.kind,
                NodeKind::VariableDeclaration
                    | NodeKind::VariableDeclarator
                    | NodeKind::LexicalDeclaration
            )
        })
    }

    /// Returns all import statements.
    pub fn imports(&self) -> Vec<&ParsedNode> {
        self.find_all(|n| n.kind == NodeKind::ImportStatement)
    }
}
