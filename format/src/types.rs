//! Type representation for language-agnostic code signatures.
//!
//! This module provides the fundamental type system used to describe
//! code signatures across different programming languages. The type
//! system is designed to be comprehensive enough to represent types
//! from JavaScript/TypeScript, Rust, Python, Go, and other languages.
//!
//! # Overview
//!
//! The type system consists of two main components:
//!
//! - [`TypeKind`] - An enum representing the category of a type
//! - [`TypeInfo`] - A complete type description with all metadata
//!
//! # Examples
//!
//! ## Creating Primitive Types
//!
//! ```rust
//! use interlint_format::TypeInfo;
//!
//! let string_type = TypeInfo::primitive("string");
//! let number_type = TypeInfo::primitive("number");
//! let boolean_type = TypeInfo::primitive("boolean");
//! ```
//!
//! ## Creating Complex Types
//!
//! ```rust
//! use interlint_format::TypeInfo;
//!
//! // Array type: string[]
//! let string_array = TypeInfo::array(TypeInfo::primitive("string"));
//!
//! // Reference to a named type
//! let user_ref = TypeInfo::reference("User");
//! ```
//!
//! ## Type Kind Classification
//!
//! Each type has a [`TypeKind`] that classifies what category of type it is:
//!
//! ```rust
//! use interlint_format::{TypeInfo, TypeKind};
//!
//! let array_type = TypeInfo::array(TypeInfo::primitive("number"));
//! assert_eq!(array_type.kind, TypeKind::Array);
//!
//! let void_type = TypeInfo::void();
//! assert_eq!(void_type.kind, TypeKind::Void);
//! ```

use serde::{Deserialize, Serialize};

/// The kind of type being represented.
///
/// This enum categorizes types into 18 distinct categories, enabling
/// language-agnostic type representation. Each variant corresponds to
/// a common type system concept found across programming languages.
///
/// # Breaking Change Detection
///
/// When comparing types between versions, a change in [`TypeKind`] typically
/// indicates a breaking change. For example, changing a function's return
/// type from `Primitive` to `Array` would be a [`TypeChanged`](crate::BreakingChangeType::TypeChanged).
///
/// # Examples
///
/// ```rust
/// use interlint_format::TypeKind;
///
/// // Matching on type kinds
/// fn describe_type(kind: &TypeKind) -> &'static str {
///     match kind {
///         TypeKind::Primitive => "a basic type like string or number",
///         TypeKind::Array => "a list of elements",
///         TypeKind::Function => "a callable",
///         TypeKind::Union => "one of several types",
///         _ => "some other type",
///     }
/// }
/// ```
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
///
/// Type parameters represent generic placeholders in parameterized types,
/// functions, classes, and interfaces. They support optional constraints
/// (bounds) and default values.
///
/// # Breaking Change Detection
///
/// Changes to type parameters can trigger several breaking change types:
///
/// - [`TypeParameterAdded`](crate::BreakingChangeType::TypeParameterAdded) - Adding a new type parameter
/// - [`TypeParameterRemoved`](crate::BreakingChangeType::TypeParameterRemoved) - Removing an existing type parameter
/// - [`TypeParameterConstraintChanged`](crate::BreakingChangeType::TypeParameterConstraintChanged) - Changing constraints
///
/// # Examples
///
/// ## Simple Type Parameter
///
/// ```rust
/// use interlint_format::TypeParameter;
///
/// // TypeScript: function identity<T>(value: T): T
/// let type_param = TypeParameter {
///     name: "T".to_string(),
///     constraint: None,
///     default: None,
/// };
/// ```
///
/// ## Constrained Type Parameter
///
/// ```rust
/// use interlint_format::{TypeParameter, TypeInfo};
///
/// // TypeScript: function compare<T extends Comparable>(a: T, b: T): number
/// let type_param = TypeParameter {
///     name: "T".to_string(),
///     constraint: Some(Box::new(TypeInfo::reference("Comparable"))),
///     default: None,
/// };
/// ```
///
/// ## Type Parameter with Default
///
/// ```rust
/// use interlint_format::{TypeParameter, TypeInfo};
///
/// // TypeScript: function create<T = object>(): T
/// let type_param = TypeParameter {
///     name: "T".to_string(),
///     constraint: None,
///     default: Some(Box::new(TypeInfo::reference("object"))),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeParameter {
    /// The name of the type parameter (e.g., "T", "K", "V").
    ///
    /// By convention, type parameter names are usually single uppercase letters,
    /// but any valid identifier can be used.
    pub name: String,

    /// Constraint/bound on the type parameter.
    ///
    /// When present, the type argument must be assignable to this constraint.
    /// For example, `T extends Serializable` means T must implement Serializable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<Box<TypeInfo>>,

    /// Default type if not specified.
    ///
    /// When present and no type argument is provided, this type is used.
    /// For example, `T = string` means T defaults to string if omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Box<TypeInfo>>,
}

/// Complete type information.
///
/// `TypeInfo` provides a comprehensive description of a type, including
/// its category ([`TypeKind`]), string representation, and structural
/// details. This is the primary type used throughout the interlint
/// format for representing types in signatures.
///
/// # Breaking Change Detection
///
/// Type changes are detected by comparing the `kind` and `repr` fields.
/// When either changes between versions, it may trigger:
///
/// - [`TypeChanged`](crate::BreakingChangeType::TypeChanged) - For general type changes
/// - [`ParameterTypeChanged`](crate::BreakingChangeType::ParameterTypeChanged) - For function parameters
/// - [`PropertyTypeChanged`](crate::BreakingChangeType::PropertyTypeChanged) - For class/interface properties
/// - [`ReturnTypeChanged`](crate::BreakingChangeType::ReturnTypeChanged) - For function return types
///
/// # Field Usage by TypeKind
///
/// Different fields are relevant for different type kinds:
///
/// | TypeKind | Primary Fields |
/// |----------|----------------|
/// | `Array` | `element_type` |
/// | `Map` | `key_type`, `value_type` |
/// | `Generic` | `type_arguments` |
/// | `Union`, `Intersection` | `constituents` |
/// | `Function` | `param_types`, `return_type` |
/// | `Tuple` | `type_arguments` |
/// | Others | `repr` only |
///
/// # Examples
///
/// ## Creating Various Types
///
/// ```rust
/// use interlint_format::{TypeInfo, TypeKind};
///
/// // Primitive: string
/// let string_type = TypeInfo::primitive("string");
/// assert_eq!(string_type.kind, TypeKind::Primitive);
/// assert_eq!(string_type.repr, "string");
///
/// // Array: string[]
/// let array_type = TypeInfo::array(TypeInfo::primitive("string"));
/// assert_eq!(array_type.kind, TypeKind::Array);
/// assert!(array_type.element_type.is_some());
///
/// // Reference: User
/// let user_type = TypeInfo::reference("User");
/// assert_eq!(user_type.kind, TypeKind::Reference);
/// ```
///
/// ## Complex Union Type
///
/// ```rust
/// use interlint_format::{TypeInfo, TypeKind};
///
/// // Union: string | number | null
/// let union_type = TypeInfo {
///     kind: TypeKind::Union,
///     repr: "string | number | null".to_string(),
///     constituents: vec![
///         TypeInfo::primitive("string"),
///         TypeInfo::primitive("number"),
///     ],
///     nullable: true,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeInfo {
    /// The kind/category of this type.
    ///
    /// This determines how the type should be interpreted and which
    /// other fields are relevant for this type instance.
    pub kind: TypeKind,

    /// String representation of the type.
    ///
    /// This is a human-readable representation of the type as it would
    /// appear in source code. Used for display and comparison purposes.
    pub repr: String,

    /// For generic types, the type arguments.
    ///
    /// Example: For `Promise<string>`, this contains `[TypeInfo::primitive("string")]`.
    /// Also used for tuple element types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_arguments: Vec<TypeInfo>,

    /// For union/intersection types, the constituent types.
    ///
    /// Example: For `A | B | C`, this contains `[A, B, C]`.
    /// The actual union/intersection is determined by `kind`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constituents: Vec<TypeInfo>,

    /// For array types, the element type.
    ///
    /// Example: For `number[]`, this is `TypeInfo::primitive("number")`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_type: Option<Box<TypeInfo>>,

    /// For map types, the key type.
    ///
    /// Example: For `Map<string, User>`, this is `TypeInfo::primitive("string")`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_type: Option<Box<TypeInfo>>,

    /// For map types, the value type.
    ///
    /// Example: For `Map<string, User>`, this is `TypeInfo::reference("User")`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_type: Option<Box<TypeInfo>>,

    /// For function types, parameter types (in order).
    ///
    /// Example: For `(x: string, y: number) => void`, this is `[string, number]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_types: Vec<TypeInfo>,

    /// For function types, the return type.
    ///
    /// Example: For `() => string`, this is `TypeInfo::primitive("string")`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<Box<TypeInfo>>,

    /// Whether this type is nullable/optional.
    ///
    /// When true, the type can be null/undefined/None in addition to
    /// its declared type. Used for union types containing null.
    #[serde(default)]
    pub nullable: bool,
}

impl Default for TypeInfo {
    fn default() -> Self {
        Self {
            kind: TypeKind::Unknown,
            repr: String::new(),
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

impl TypeInfo {
    /// Create a primitive type.
    ///
    /// Primitive types represent basic built-in types like `string`, `number`,
    /// `boolean`, `int`, `float`, etc.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind};
    ///
    /// let string_type = TypeInfo::primitive("string");
    /// assert_eq!(string_type.kind, TypeKind::Primitive);
    /// assert_eq!(string_type.repr, "string");
    /// ```
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
    ///
    /// Reference types point to named types defined elsewhere, such as
    /// classes, interfaces, type aliases, or imported types.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind};
    ///
    /// let user_type = TypeInfo::reference("User");
    /// assert_eq!(user_type.kind, TypeKind::Reference);
    /// assert_eq!(user_type.repr, "User");
    /// ```
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
    ///
    /// Array types represent ordered collections of elements, all of the
    /// same type. The element type is stored in [`element_type`](Self::element_type).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind};
    ///
    /// // string[]
    /// let string_array = TypeInfo::array(TypeInfo::primitive("string"));
    /// assert_eq!(string_array.kind, TypeKind::Array);
    /// assert_eq!(string_array.repr, "string[]");
    ///
    /// // Nested: number[][]
    /// let nested_array = TypeInfo::array(TypeInfo::array(TypeInfo::primitive("number")));
    /// assert_eq!(nested_array.repr, "number[][]");
    /// ```
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
    ///
    /// Void types represent the absence of a return value. Commonly used
    /// for functions that don't return anything meaningful.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind};
    ///
    /// let void_type = TypeInfo::void();
    /// assert_eq!(void_type.kind, TypeKind::Void);
    /// assert_eq!(void_type.repr, "void");
    /// ```
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
    ///
    /// Any types represent a value that can be any type. This is often
    /// used for dynamic typing or when type information is unavailable.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind};
    ///
    /// let any_type = TypeInfo::any();
    /// assert_eq!(any_type.kind, TypeKind::Any);
    /// assert_eq!(any_type.repr, "any");
    /// ```
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
