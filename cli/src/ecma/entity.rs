//! Entity declarations extracted from parsed AST.
//!
//! This module provides types for representing declarations found in
//! JavaScript/TypeScript source code.

use crate::parser::{NodeKind, ParsedNode};
use anyhow::{bail, Result};

/// Represents a declaration entity extracted from parsed source.
#[derive(Clone, Debug)]
pub enum EntityDeclaration {
    /// A variable declaration (const, let, var).
    Var(String, VarInfo),
    /// A class declaration.
    Class(String, ClassInfo),
    /// A function declaration.
    Func(String, FuncInfo),
    /// A TypeScript interface declaration.
    Interface(String, InterfaceInfo),
    /// A TypeScript enum declaration.
    Enum(String, EnumInfo),
    /// A TypeScript type alias declaration.
    TypeAlias(String, TypeAliasInfo),
}

/// Information about a variable declaration.
#[derive(Clone, Debug, Default)]
pub struct VarInfo {
    /// The kind of variable (const, let, var).
    pub kind: VarKind,
    /// Whether it has a type annotation.
    pub has_type_annotation: bool,
    /// Whether it's exported.
    pub is_exported: bool,
    /// Source text of the declaration.
    pub source: String,
}

/// Kind of variable declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum VarKind {
    #[default]
    Const,
    Let,
    Var,
}

/// Information about a class declaration.
#[derive(Clone, Debug, Default)]
pub struct ClassInfo {
    /// Method names.
    pub methods: Vec<String>,
    /// Property names.
    pub properties: Vec<String>,
    /// Whether the class is abstract.
    pub is_abstract: bool,
    /// Whether it's exported.
    pub is_exported: bool,
    /// Source text of the declaration.
    pub source: String,
}

/// Information about a function declaration.
#[derive(Clone, Debug, Default)]
pub struct FuncInfo {
    /// Parameter names.
    pub params: Vec<String>,
    /// Whether it's async.
    pub is_async: bool,
    /// Whether it's a generator.
    pub is_generator: bool,
    /// Whether it's exported.
    pub is_exported: bool,
    /// Source text of the declaration.
    pub source: String,
}

/// Information about a TypeScript interface declaration.
#[derive(Clone, Debug, Default)]
pub struct InterfaceInfo {
    /// Property names.
    pub properties: Vec<String>,
    /// Method names.
    pub methods: Vec<String>,
    /// Whether it's exported.
    pub is_exported: bool,
    /// Source text of the declaration.
    pub source: String,
}

/// Information about a TypeScript enum declaration.
#[derive(Clone, Debug, Default)]
pub struct EnumInfo {
    /// Member names.
    pub members: Vec<String>,
    /// Whether it's exported.
    pub is_exported: bool,
    /// Source text of the declaration.
    pub source: String,
}

/// Information about a TypeScript type alias declaration.
#[derive(Clone, Debug, Default)]
pub struct TypeAliasInfo {
    /// Whether it's exported.
    pub is_exported: bool,
    /// Source text of the declaration.
    pub source: String,
}

impl EntityDeclaration {
    /// Creates an EntityDeclaration from a ParsedNode.
    pub fn from_node(node: &ParsedNode) -> Result<Self> {
        let name = node
            .name
            .clone()
            .or_else(|| Self::extract_name_from_children(node))
            .unwrap_or_else(|| "<anonymous>".to_string());

        match node.kind {
            NodeKind::FunctionDeclaration => {
                let info = FuncInfo {
                    is_async: node.modifiers.contains(&"async".to_string()),
                    is_generator: node.text.contains("function*"),
                    is_exported: node.is_exported,
                    source: node.text.clone(),
                    params: Self::extract_param_names(node),
                };
                Ok(Self::Func(name, info))
            }
            NodeKind::ClassDeclaration => {
                let info = ClassInfo {
                    methods: Self::extract_method_names(node),
                    properties: Self::extract_property_names(node),
                    is_abstract: node.modifiers.contains(&"abstract".to_string()),
                    is_exported: node.is_exported,
                    source: node.text.clone(),
                };
                Ok(Self::Class(name, info))
            }
            NodeKind::VariableDeclaration
            | NodeKind::VariableDeclarator
            | NodeKind::LexicalDeclaration => {
                let kind = if node.text.starts_with("const") {
                    VarKind::Const
                } else if node.text.starts_with("let") {
                    VarKind::Let
                } else {
                    VarKind::Var
                };
                let info = VarInfo {
                    kind,
                    has_type_annotation: node.text.contains(':'),
                    is_exported: node.is_exported,
                    source: node.text.clone(),
                };
                Ok(Self::Var(name, info))
            }
            NodeKind::InterfaceDeclaration => {
                let info = InterfaceInfo {
                    properties: Self::extract_property_names(node),
                    methods: Self::extract_method_names(node),
                    is_exported: node.is_exported,
                    source: node.text.clone(),
                };
                Ok(Self::Interface(name, info))
            }
            NodeKind::EnumDeclaration => {
                let info = EnumInfo {
                    members: Self::extract_enum_members(node),
                    is_exported: node.is_exported,
                    source: node.text.clone(),
                };
                Ok(Self::Enum(name, info))
            }
            NodeKind::TypeAnnotation | NodeKind::TypeAliasDeclaration => {
                let info = TypeAliasInfo {
                    is_exported: node.is_exported,
                    source: node.text.clone(),
                };
                Ok(Self::TypeAlias(name, info))
            }
            _ => bail!(
                "Cannot create EntityDeclaration from node kind: {:?}",
                node.kind
            ),
        }
    }

    /// Returns the name of this entity.
    pub fn name(&self) -> &String {
        match self {
            Self::Var(name, _) => name,
            Self::Class(name, _) => name,
            Self::Func(name, _) => name,
            Self::Interface(name, _) => name,
            Self::Enum(name, _) => name,
            Self::TypeAlias(name, _) => name,
        }
    }

    /// Returns whether this entity is exported.
    pub fn is_exported(&self) -> bool {
        match self {
            Self::Var(_, info) => info.is_exported,
            Self::Class(_, info) => info.is_exported,
            Self::Func(_, info) => info.is_exported,
            Self::Interface(_, info) => info.is_exported,
            Self::Enum(_, info) => info.is_exported,
            Self::TypeAlias(_, info) => info.is_exported,
        }
    }

    /// Extracts name from child nodes.
    fn extract_name_from_children(node: &ParsedNode) -> Option<String> {
        node.children
            .iter()
            .find(|c| matches!(c.kind, NodeKind::Identifier | NodeKind::PropertyIdentifier))
            .and_then(|c| c.name.clone().or_else(|| Some(c.text.clone())))
    }

    /// Extracts parameter names from a function node.
    fn extract_param_names(node: &ParsedNode) -> Vec<String> {
        node.find_all(|n| n.kind == NodeKind::Identifier)
            .iter()
            .filter_map(|n| n.name.clone())
            .collect()
    }

    /// Extracts method names from a class node.
    fn extract_method_names(node: &ParsedNode) -> Vec<String> {
        node.find_all(|n| n.kind == NodeKind::MethodDefinition)
            .iter()
            .filter_map(|n| n.name.clone())
            .collect()
    }

    /// Extracts property names from a class or interface node.
    fn extract_property_names(node: &ParsedNode) -> Vec<String> {
        node.find_all(|n| n.kind == NodeKind::PropertyDefinition)
            .iter()
            .filter_map(|n| n.name.clone())
            .collect()
    }

    /// Extracts enum member names.
    fn extract_enum_members(node: &ParsedNode) -> Vec<String> {
        node.find_all(|n| n.kind == NodeKind::Identifier)
            .iter()
            .filter_map(|n| n.name.clone())
            .collect()
    }
}

/// Trait for converting types to EntityDeclaration.
pub trait AsEntityDeclaration {
    fn to_entity(&self) -> Result<EntityDeclaration>;
}

impl AsEntityDeclaration for ParsedNode {
    fn to_entity(&self) -> Result<EntityDeclaration> {
        EntityDeclaration::from_node(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecma::parser::parse_source;
    use std::path::PathBuf;

    #[test]
    fn test_function_entity() {
        let source = "function greet(name) { return `Hello, ${name}!`; }";
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();
        let functions = parsed.functions();
        assert_eq!(functions.len(), 1);

        let entity = EntityDeclaration::from_node(functions[0]).unwrap();
        assert!(matches!(entity, EntityDeclaration::Func(_, _)));
        assert_eq!(entity.name(), "greet");
    }

    #[test]
    fn test_class_entity() {
        let source = r#"
            class Calculator {
                add(a, b) { return a + b; }
            }
        "#;
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();
        let classes = parsed.classes();
        assert_eq!(classes.len(), 1);

        let entity = EntityDeclaration::from_node(classes[0]).unwrap();
        assert!(matches!(entity, EntityDeclaration::Class(_, _)));
        assert_eq!(entity.name(), "Calculator");
    }

    #[test]
    fn test_async_function() {
        let source = "async function fetchData() { return await fetch('/api'); }";
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();
        let functions = parsed.functions();
        assert_eq!(functions.len(), 1);

        let entity = EntityDeclaration::from_node(functions[0]).unwrap();
        if let EntityDeclaration::Func(_, info) = entity {
            assert!(info.is_async);
        } else {
            panic!("Expected function entity");
        }
    }
}
