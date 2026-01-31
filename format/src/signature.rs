//! Signature definitions for various code constructs.

use serde::{Deserialize, Serialize};

use crate::types::{TypeInfo, TypeParameter};

/// Visibility/access level of a signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Public/exported
    #[default]
    Public,
    /// Protected (accessible in subclasses)
    Protected,
    /// Private (internal only)
    Private,
    /// Package/module-internal
    Internal,
}

/// A function/method parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub type_info: TypeInfo,
    /// Whether the parameter is optional
    #[serde(default)]
    pub optional: bool,
    /// Whether this is a rest/variadic parameter
    #[serde(default)]
    pub rest: bool,
    /// Default value expression (as string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

/// Function or method signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionSig {
    /// Function name
    pub name: String,
    /// Type parameters (generics)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_parameters: Vec<TypeParameter>,
    /// Function parameters
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    /// Return type
    pub return_type: TypeInfo,
    /// Whether this is an async function
    #[serde(default)]
    pub is_async: bool,
    /// Whether this is a generator function
    #[serde(default)]
    pub is_generator: bool,
    /// Visibility
    #[serde(default)]
    pub visibility: Visibility,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// A property/field in a class or interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    /// Property name
    pub name: String,
    /// Property type
    pub type_info: TypeInfo,
    /// Whether the property is optional
    #[serde(default)]
    pub optional: bool,
    /// Whether the property is readonly/immutable
    #[serde(default)]
    pub readonly: bool,
    /// Whether this is a static property
    #[serde(default)]
    pub is_static: bool,
    /// Visibility
    #[serde(default)]
    pub visibility: Visibility,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Class/struct signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassSig {
    /// Class name
    pub name: String,
    /// Type parameters (generics)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_parameters: Vec<TypeParameter>,
    /// Parent class/struct (extends)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<TypeInfo>,
    /// Implemented interfaces/traits
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<TypeInfo>,
    /// Properties/fields
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<Property>,
    /// Methods
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<FunctionSig>,
    /// Constructor signature (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constructor: Option<Box<FunctionSig>>,
    /// Whether this is an abstract class
    #[serde(default)]
    pub is_abstract: bool,
    /// Visibility
    #[serde(default)]
    pub visibility: Visibility,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Variable/constant signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableSig {
    /// Variable name
    pub name: String,
    /// Variable type
    pub type_info: TypeInfo,
    /// Whether this is a constant
    #[serde(default)]
    pub is_const: bool,
    /// Visibility
    #[serde(default)]
    pub visibility: Visibility,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Interface/trait/protocol signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterfaceSig {
    /// Interface name
    pub name: String,
    /// Type parameters (generics)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_parameters: Vec<TypeParameter>,
    /// Extended interfaces
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<TypeInfo>,
    /// Properties
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<Property>,
    /// Methods
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<FunctionSig>,
    /// Index signature type (for map-like interfaces)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_signature: Option<IndexSignature>,
    /// Visibility
    #[serde(default)]
    pub visibility: Visibility,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Index signature for map-like types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSignature {
    /// Key type
    pub key_type: TypeInfo,
    /// Value type
    pub value_type: TypeInfo,
    /// Whether the index is readonly
    #[serde(default)]
    pub readonly: bool,
}

/// Enum member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumMember {
    /// Member name
    pub name: String,
    /// Member value (as string representation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Enum signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumSig {
    /// Enum name
    pub name: String,
    /// Enum members
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<EnumMember>,
    /// Whether this is a const enum (inlined at compile time)
    #[serde(default)]
    pub is_const: bool,
    /// Visibility
    #[serde(default)]
    pub visibility: Visibility,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Type alias signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeAliasSig {
    /// Alias name
    pub name: String,
    /// Type parameters (generics)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_parameters: Vec<TypeParameter>,
    /// The aliased type
    pub aliased_type: TypeInfo,
    /// Visibility
    #[serde(default)]
    pub visibility: Visibility,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Namespace/module signature (for nested namespaces).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceSig {
    /// Namespace name
    pub name: String,
    /// Exported signatures within this namespace
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<Signature>,
    /// Visibility
    #[serde(default)]
    pub visibility: Visibility,
    /// Documentation comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// A signature representing any exportable code construct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Signature {
    /// Function signature
    Function(FunctionSig),
    /// Class signature
    Class(ClassSig),
    /// Variable signature
    Variable(VariableSig),
    /// Interface signature
    Interface(InterfaceSig),
    /// Enum signature
    Enum(EnumSig),
    /// Type alias signature
    TypeAlias(TypeAliasSig),
    /// Namespace signature
    Namespace(NamespaceSig),
}

impl Signature {
    /// Get the name of this signature.
    pub fn name(&self) -> &str {
        match self {
            Signature::Function(s) => &s.name,
            Signature::Class(s) => &s.name,
            Signature::Variable(s) => &s.name,
            Signature::Interface(s) => &s.name,
            Signature::Enum(s) => &s.name,
            Signature::TypeAlias(s) => &s.name,
            Signature::Namespace(s) => &s.name,
        }
    }

    /// Get the visibility of this signature.
    pub fn visibility(&self) -> &Visibility {
        match self {
            Signature::Function(s) => &s.visibility,
            Signature::Class(s) => &s.visibility,
            Signature::Variable(s) => &s.visibility,
            Signature::Interface(s) => &s.visibility,
            Signature::Enum(s) => &s.visibility,
            Signature::TypeAlias(s) => &s.visibility,
            Signature::Namespace(s) => &s.visibility,
        }
    }

    /// Get the documentation comment if any.
    pub fn doc(&self) -> Option<&str> {
        match self {
            Signature::Function(s) => s.doc.as_deref(),
            Signature::Class(s) => s.doc.as_deref(),
            Signature::Variable(s) => s.doc.as_deref(),
            Signature::Interface(s) => s.doc.as_deref(),
            Signature::Enum(s) => s.doc.as_deref(),
            Signature::TypeAlias(s) => s.doc.as_deref(),
            Signature::Namespace(s) => s.doc.as_deref(),
        }
    }
}
