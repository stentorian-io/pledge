//! Type representation for language-agnostic code signatures.

use serde::{Deserialize, Serialize};

/// The kind of type being represented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    /// Primitive types (string, number, boolean, etc.)
    Primitive,
    /// Array/list types
    Array,
    /// Tuple types
    Tuple,
    /// Object/struct/record types
    Object,
    /// Function/callable types
    Function,
    /// Union types (A | B)
    Union,
    /// Intersection types (A & B)
    Intersection,
    /// Generic/parameterized types
    Generic,
    /// Reference to another type by name
    Reference,
    /// Literal types (specific values)
    Literal,
    /// Optional/nullable types
    Optional,
    /// Map/dictionary types
    Map,
    /// Set types
    Set,
    /// Promise/future/async types
    Async,
    /// Void/unit types
    Void,
    /// Never/bottom types
    Never,
    /// Any/dynamic types
    Any,
    /// Unknown types
    Unknown,
}

/// A type parameter (generic parameter) definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeParameter {
    /// The name of the type parameter (e.g., "T", "K", "V")
    pub name: String,
    /// Constraint/bound on the type parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<Box<TypeInfo>>,
    /// Default type if not specified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Box<TypeInfo>>,
}

/// Complete type information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeInfo {
    /// The kind of type
    pub kind: TypeKind,
    /// String representation of the type
    pub repr: String,
    /// For generic types, the type arguments
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_arguments: Vec<TypeInfo>,
    /// For union/intersection types, the constituent types
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constituents: Vec<TypeInfo>,
    /// For array types, the element type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_type: Option<Box<TypeInfo>>,
    /// For map types, the key type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_type: Option<Box<TypeInfo>>,
    /// For map types, the value type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_type: Option<Box<TypeInfo>>,
    /// For function types, parameter types
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_types: Vec<TypeInfo>,
    /// For function types, return type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<Box<TypeInfo>>,
    /// Whether this type is nullable/optional
    #[serde(default)]
    pub nullable: bool,
}

impl TypeInfo {
    /// Create a primitive type.
    pub fn primitive(repr: impl Into<String>) -> Self {
        Self {
            kind: TypeKind::Primitive,
            repr: repr.into(),
            type_arguments: Vec::new(),
            constituents: Vec::new(),
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable: false,
        }
    }

    /// Create a reference type.
    pub fn reference(name: impl Into<String>) -> Self {
        Self {
            kind: TypeKind::Reference,
            repr: name.into(),
            type_arguments: Vec::new(),
            constituents: Vec::new(),
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable: false,
        }
    }

    /// Create an array type.
    pub fn array(element: TypeInfo) -> Self {
        let repr = format!("{}[]", element.repr);
        Self {
            kind: TypeKind::Array,
            repr,
            type_arguments: Vec::new(),
            constituents: Vec::new(),
            element_type: Some(Box::new(element)),
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable: false,
        }
    }

    /// Create a void type.
    pub fn void() -> Self {
        Self {
            kind: TypeKind::Void,
            repr: "void".into(),
            type_arguments: Vec::new(),
            constituents: Vec::new(),
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable: false,
        }
    }

    /// Create an any type.
    pub fn any() -> Self {
        Self {
            kind: TypeKind::Any,
            repr: "any".into(),
            type_arguments: Vec::new(),
            constituents: Vec::new(),
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable: false,
        }
    }
}
