//! Signature definitions for various code constructs.
//!
//! This module provides types for representing different kinds of exported
//! code constructs: functions, classes, interfaces, enums, type aliases,
//! variables, and namespaces.
//!
//! # Overview
//!
//! The central type is [`Signature`], an enum that wraps specific signature
//! types:
//!
//! - [`FunctionSig`] - Functions and methods
//! - [`ClassSig`] - Classes and structs
//! - [`InterfaceSig`] - Interfaces and traits
//! - [`VariableSig`] - Variables and constants
//! - [`EnumSig`] - Enumerations
//! - [`TypeAliasSig`] - Type aliases
//! - [`NamespaceSig`] - Nested namespaces
//!
//! # Breaking Change Detection
//!
//! Changes to signatures are the primary source of Library/Code Breaking Changes.
//! Each signature type documents which breaking change types it can trigger.
//!
//! # Examples
//!
//! ## Creating a Function Signature
//!
//! ```rust
//! use interlint_format::{Signature, FunctionSig, Parameter, TypeInfo, Visibility};
//!
//! let greet_fn = Signature::Function(FunctionSig {
//!     name: "greet".to_string(),
//!     type_parameters: vec![],
//!     parameters: vec![
//!         Parameter {
//!             name: "name".to_string(),
//!             type_info: TypeInfo::primitive("string"),
//!             optional: false,
//!             rest: false,
//!             default_value: None,
//!         },
//!     ],
//!     return_type: TypeInfo::primitive("string"),
//!     is_async: false,
//!     is_generator: false,
//!     visibility: Visibility::Public,
//!     doc: Some("Returns a greeting for the given name".to_string()),
//! });
//!
//! assert_eq!(greet_fn.name(), "greet");
//! ```

use serde::{Deserialize, Serialize};

use crate::types::{TypeInfo, TypeParameter};

/// Visibility/access level of a signature.
///
/// Visibility determines who can access a particular code element.
/// Changes to visibility can trigger breaking changes when public
/// items become less accessible.
///
/// # Breaking Change Detection
///
/// [`VisibilityChanged`](crate::BreakingChangeType::VisibilityChanged) is triggered
/// when an item's visibility changes in a way that restricts access (e.g., public to private).
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
///
/// Parameters define the inputs that a function accepts. Changes to parameters
/// are a common source of breaking changes since they affect how consumers
/// call functions.
///
/// # Breaking Change Detection
///
/// Parameter changes can trigger several breaking change types:
///
/// - [`ParameterAdded`](crate::BreakingChangeType::ParameterAdded) - Critical for required params
/// - [`ParameterRemoved`](crate::BreakingChangeType::ParameterRemoved) - Major breaking change
/// - [`ParameterTypeChanged`](crate::BreakingChangeType::ParameterTypeChanged) - Major breaking change
/// - [`ParameterBecameRequired`](crate::BreakingChangeType::ParameterBecameRequired) - Critical
/// - [`ParameterBecameOptional`](crate::BreakingChangeType::ParameterBecameOptional) - Warning (usually safe)
///
/// # Examples
///
/// ## Required Parameter
///
/// ```rust
/// use interlint_format::{Parameter, TypeInfo};
///
/// let name_param = Parameter {
///     name: "name".to_string(),
///     type_info: TypeInfo::primitive("string"),
///     optional: false,
///     rest: false,
///     default_value: None,
/// };
/// ```
///
/// ## Optional Parameter with Default
///
/// ```rust
/// use interlint_format::{Parameter, TypeInfo};
///
/// let options_param = Parameter {
///     name: "options".to_string(),
///     type_info: TypeInfo::reference("Options"),
///     optional: true,
///     rest: false,
///     default_value: Some("{}".to_string()),
/// };
/// ```
///
/// ## Rest Parameter
///
/// ```rust
/// use interlint_format::{Parameter, TypeInfo};
///
/// // function log(...messages: string[]): void
/// let rest_param = Parameter {
///     name: "messages".to_string(),
///     type_info: TypeInfo::array(TypeInfo::primitive("string")),
///     optional: false,
///     rest: true,
///     default_value: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    /// Parameter name.
    ///
    /// The identifier used to reference this parameter within the function body.
    pub name: String,

    /// Parameter type.
    ///
    /// The expected type of values passed to this parameter.
    pub type_info: TypeInfo,

    /// Whether the parameter is optional.
    ///
    /// Optional parameters can be omitted when calling the function.
    /// Usually indicated by `?` in TypeScript or default values.
    #[serde(default)]
    pub optional: bool,

    /// Whether this is a rest/variadic parameter.
    ///
    /// Rest parameters collect all remaining arguments into an array.
    /// Only the last parameter can be a rest parameter.
    #[serde(default)]
    pub rest: bool,

    /// Default value expression (as string).
    ///
    /// The default value used when the parameter is not provided.
    /// Stored as the source code representation of the expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

/// Function or method signature.
///
/// Represents the complete signature of a function, including its name,
/// parameters, return type, and modifiers like `async` or generator.
///
/// # Breaking Change Detection
///
/// Function signatures can trigger many breaking change types:
///
/// - [`SignatureChanged`](crate::BreakingChangeType::SignatureChanged) - General signature change
/// - [`ReturnTypeChanged`](crate::BreakingChangeType::ReturnTypeChanged) - Return type modified
/// - [`AsyncModifierChanged`](crate::BreakingChangeType::AsyncModifierChanged) - Async status changed
/// - Parameter-related changes (see [`Parameter`])
/// - Type parameter changes (see [`TypeParameter`])
///
/// # Examples
///
/// ## Simple Function
///
/// ```rust
/// use interlint_format::{FunctionSig, TypeInfo, Visibility};
///
/// // function greet(): string
/// let greet_fn = FunctionSig {
///     name: "greet".to_string(),
///     type_parameters: vec![],
///     parameters: vec![],
///     return_type: TypeInfo::primitive("string"),
///     is_async: false,
///     is_generator: false,
///     visibility: Visibility::Public,
///     doc: None,
/// };
/// ```
///
/// ## Async Function with Parameters
///
/// ```rust
/// use interlint_format::{FunctionSig, Parameter, TypeInfo, Visibility};
///
/// // async function fetchUser(id: string): Promise<User>
/// let fetch_user_fn = FunctionSig {
///     name: "fetchUser".to_string(),
///     type_parameters: vec![],
///     parameters: vec![
///         Parameter {
///             name: "id".to_string(),
///             type_info: TypeInfo::primitive("string"),
///             optional: false,
///             rest: false,
///             default_value: None,
///         },
///     ],
///     return_type: TypeInfo::reference("Promise<User>"),
///     is_async: true,
///     is_generator: false,
///     visibility: Visibility::Public,
///     doc: Some("Fetches a user by their ID".to_string()),
/// };
/// ```
///
/// ## Generic Function
///
/// ```rust
/// use interlint_format::{FunctionSig, Parameter, TypeInfo, TypeParameter, Visibility};
///
/// // function identity<T>(value: T): T
/// let identity_fn = FunctionSig {
///     name: "identity".to_string(),
///     type_parameters: vec![
///         TypeParameter {
///             name: "T".to_string(),
///             constraint: None,
///             default: None,
///         },
///     ],
///     parameters: vec![
///         Parameter {
///             name: "value".to_string(),
///             type_info: TypeInfo::reference("T"),
///             optional: false,
///             rest: false,
///             default_value: None,
///         },
///     ],
///     return_type: TypeInfo::reference("T"),
///     is_async: false,
///     is_generator: false,
///     visibility: Visibility::Public,
///     doc: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionSig {
    /// Function name.
    pub name: String,

    /// Type parameters (generics).
    ///
    /// Generic type parameters like `<T>`, `<K, V>`, etc.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_parameters: Vec<TypeParameter>,

    /// Function parameters.
    ///
    /// The list of parameters accepted by the function, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,

    /// Return type.
    ///
    /// The type of value returned by the function.
    pub return_type: TypeInfo,

    /// Whether this is an async function.
    ///
    /// Async functions return promises and can use `await`.
    #[serde(default)]
    pub is_async: bool,

    /// Whether this is a generator function.
    ///
    /// Generator functions use `yield` and return iterators.
    #[serde(default)]
    pub is_generator: bool,

    /// Visibility.
    ///
    /// The access level of this function.
    #[serde(default)]
    pub visibility: Visibility,

    /// Documentation comment.
    ///
    /// JSDoc, rustdoc, or other documentation extracted from source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// A property/field in a class or interface.
///
/// Properties represent data members of classes and interfaces.
/// Changes to properties can affect consumers who access these members.
///
/// # Breaking Change Detection
///
/// Property changes can trigger:
///
/// - [`PropertyAdded`](crate::BreakingChangeType::PropertyAdded) - Warning for optional, Major for required
/// - [`PropertyRemoved`](crate::BreakingChangeType::PropertyRemoved) - Major breaking change
/// - [`PropertyTypeChanged`](crate::BreakingChangeType::PropertyTypeChanged) - Major breaking change
/// - [`PropertyBecameRequired`](crate::BreakingChangeType::PropertyBecameRequired) - Critical
/// - [`PropertyBecameOptional`](crate::BreakingChangeType::PropertyBecameOptional) - Warning
///
/// # Examples
///
/// ## Required Property
///
/// ```rust
/// use interlint_format::{Property, TypeInfo, Visibility};
///
/// let name_prop = Property {
///     name: "name".to_string(),
///     type_info: TypeInfo::primitive("string"),
///     optional: false,
///     readonly: false,
///     is_static: false,
///     visibility: Visibility::Public,
///     doc: None,
/// };
/// ```
///
/// ## Readonly Static Property
///
/// ```rust
/// use interlint_format::{Property, TypeInfo, Visibility};
///
/// // static readonly VERSION: string
/// let version_prop = Property {
///     name: "VERSION".to_string(),
///     type_info: TypeInfo::primitive("string"),
///     optional: false,
///     readonly: true,
///     is_static: true,
///     visibility: Visibility::Public,
///     doc: Some("Library version number".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    /// Property name.
    pub name: String,

    /// Property type.
    pub type_info: TypeInfo,

    /// Whether the property is optional.
    ///
    /// Optional properties may not be present on all instances.
    #[serde(default)]
    pub optional: bool,

    /// Whether the property is readonly/immutable.
    ///
    /// Readonly properties cannot be reassigned after initialization.
    #[serde(default)]
    pub readonly: bool,

    /// Whether this is a static property.
    ///
    /// Static properties belong to the class itself, not instances.
    #[serde(default)]
    pub is_static: bool,

    /// Visibility.
    #[serde(default)]
    pub visibility: Visibility,

    /// Documentation comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Class/struct signature.
///
/// Represents a class or struct with its properties, methods, constructor,
/// inheritance, and interface implementations.
///
/// # Breaking Change Detection
///
/// Class changes can trigger:
///
/// - [`TypeChanged`](crate::BreakingChangeType::TypeChanged) - Base class or abstract modifier changed
/// - Property and method changes (see [`Property`], [`FunctionSig`])
/// - Type parameter changes (see [`TypeParameter`])
///
/// # Examples
///
/// ## Simple Class
///
/// ```rust
/// use interlint_format::{ClassSig, Property, TypeInfo, Visibility};
///
/// let user_class = ClassSig {
///     name: "User".to_string(),
///     type_parameters: vec![],
///     extends: None,
///     implements: vec![],
///     properties: vec![
///         Property {
///             name: "name".to_string(),
///             type_info: TypeInfo::primitive("string"),
///             optional: false,
///             readonly: false,
///             is_static: false,
///             visibility: Visibility::Public,
///             doc: None,
///         },
///     ],
///     methods: vec![],
///     constructor: None,
///     is_abstract: false,
///     visibility: Visibility::Public,
///     doc: Some("Represents a user in the system".to_string()),
/// };
/// ```
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
///
/// `Signature` is the central enum that wraps all possible types of exports.
/// Each variant corresponds to a different kind of code construct that can
/// be exported from a module.
///
/// # Breaking Change Detection
///
/// If an export's signature variant changes (e.g., from Function to Class),
/// it triggers [`ExportKindChanged`](crate::BreakingChangeType::ExportKindChanged),
/// which is a Critical breaking change.
///
/// # Serialization
///
/// Signatures are serialized with a `kind` discriminator tag:
///
/// ```json
/// {
///   "kind": "function",
///   "name": "greet",
///   "parameters": [...],
///   ...
/// }
/// ```
///
/// # Examples
///
/// ## Pattern Matching on Signatures
///
/// ```rust
/// use interlint_format::Signature;
///
/// fn describe_signature(sig: &Signature) -> &'static str {
///     match sig {
///         Signature::Function(_) => "a callable function",
///         Signature::Class(_) => "an instantiable class",
///         Signature::Interface(_) => "a type contract",
///         Signature::Enum(_) => "an enumeration",
///         Signature::TypeAlias(_) => "a type alias",
///         Signature::Variable(_) => "a variable or constant",
///         Signature::Namespace(_) => "a namespace container",
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Signature {
    /// Function signature.
    ///
    /// Represents standalone functions and methods.
    Function(FunctionSig),

    /// Class signature.
    ///
    /// Represents classes, structs, and similar instantiable types.
    Class(ClassSig),

    /// Variable signature.
    ///
    /// Represents exported variables, constants, and module-level values.
    Variable(VariableSig),

    /// Interface signature.
    ///
    /// Represents interfaces, traits, protocols, and similar type contracts.
    Interface(InterfaceSig),

    /// Enum signature.
    ///
    /// Represents enumerations and similar sum types.
    Enum(EnumSig),

    /// Type alias signature.
    ///
    /// Represents type aliases and type definitions.
    TypeAlias(TypeAliasSig),

    /// Namespace signature.
    ///
    /// Represents nested namespaces and module containers.
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
