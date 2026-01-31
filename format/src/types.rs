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

// =============================================================================
// Variance - Position-based type compatibility
// =============================================================================

/// Variance describes how a type parameter behaves in subtyping relationships.
///
/// Understanding variance is essential for detecting breaking changes in generic types:
///
/// - **Covariant**: Subtyping follows the type parameter direction (output positions)
/// - **Contravariant**: Subtyping reverses direction (input positions)
/// - **Invariant**: No subtyping allowed (both read and write)
/// - **Bivariant**: Subtyping in both directions (loose, often method parameters)
///
/// # Breaking Change Detection
///
/// Variance affects which type changes are breaking:
///
/// | Variance | Narrowing Type | Widening Type |
/// |----------|----------------|---------------|
/// | Covariant | Breaking | Safe |
/// | Contravariant | Safe | Breaking |
/// | Invariant | Breaking | Breaking |
/// | Bivariant | Safe | Safe |
///
/// # Examples
///
/// ```rust
/// use interlint_format::Variance;
///
/// // Return types are covariant
/// let return_variance = Variance::Covariant;
/// assert!(return_variance.is_safe_to_widen()); // Can return more general type
///
/// // Parameters are contravariant
/// let param_variance = Variance::Contravariant;
/// assert!(param_variance.is_safe_to_narrow()); // Can accept more specific type
///
/// // Mutable properties are invariant
/// let prop_variance = Variance::Invariant;
/// assert!(!prop_variance.is_safe_to_widen());
/// assert!(!prop_variance.is_safe_to_narrow());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Variance {
    /// Covariant - subtyping follows type parameter direction.
    ///
    /// Used in output positions (return types, readonly properties).
    /// `T` can be replaced with a subtype of `T`.
    #[default]
    Covariant,

    /// Contravariant - subtyping reverses direction.
    ///
    /// Used in input positions (function parameters).
    /// `T` can be replaced with a supertype of `T`.
    Contravariant,

    /// Invariant - no subtyping relationship.
    ///
    /// Used when type appears in both input and output positions
    /// (mutable properties, class type parameters with methods).
    Invariant,

    /// Bivariant - subtyping in both directions (loose).
    ///
    /// Used for method parameters in some type systems (TypeScript methods).
    /// Less type-safe but allows more flexibility.
    Bivariant,
}

impl Variance {
    /// Returns true if widening the type is safe (won't break consumers).
    ///
    /// Widening means replacing `T` with a supertype of `T`.
    /// This is safe for covariant and bivariant positions.
    pub fn is_safe_to_widen(&self) -> bool {
        matches!(self, Variance::Covariant | Variance::Bivariant)
    }

    /// Returns true if narrowing the type is safe (won't break consumers).
    ///
    /// Narrowing means replacing `T` with a subtype of `T`.
    /// This is safe for contravariant and bivariant positions.
    pub fn is_safe_to_narrow(&self) -> bool {
        matches!(self, Variance::Contravariant | Variance::Bivariant)
    }

    /// Combine two variances (for nested type positions).
    ///
    /// When a type parameter appears inside another generic type,
    /// the variances are combined:
    /// - Covariant * Covariant = Covariant
    /// - Covariant * Contravariant = Contravariant
    /// - Contravariant * Contravariant = Covariant
    /// - Anything with Invariant = Invariant
    /// - Bivariant with anything = Bivariant (in TypeScript)
    pub fn combine(&self, other: Variance) -> Variance {
        match (self, other) {
            (Variance::Invariant, _) | (_, Variance::Invariant) => Variance::Invariant,
            (Variance::Bivariant, _) | (_, Variance::Bivariant) => Variance::Bivariant,
            (Variance::Covariant, Variance::Covariant) => Variance::Covariant,
            (Variance::Contravariant, Variance::Contravariant) => Variance::Covariant,
            (Variance::Covariant, Variance::Contravariant)
            | (Variance::Contravariant, Variance::Covariant) => Variance::Contravariant,
        }
    }

    /// Get the variance for a function return type.
    pub fn for_return_type() -> Self {
        Variance::Covariant
    }

    /// Get the variance for a function parameter.
    pub fn for_parameter() -> Self {
        Variance::Contravariant
    }

    /// Get the variance for a readonly property.
    pub fn for_readonly_property() -> Self {
        Variance::Covariant
    }

    /// Get the variance for a mutable property.
    pub fn for_mutable_property() -> Self {
        Variance::Invariant
    }

    /// Get the variance for a method parameter (TypeScript-style bivariance).
    pub fn for_method_parameter() -> Self {
        Variance::Bivariant
    }
}

// =============================================================================
// TypeKind - Classification of types
// =============================================================================

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

    /// For object types, the properties with their names and types.
    ///
    /// Used for structural comparison of object types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<ObjectProperty>,
}

/// A property within an object type.
///
/// Used for structural type comparison to detect breaking changes
/// in object shapes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectProperty {
    /// The name of the property.
    pub name: String,

    /// The type of the property.
    pub type_info: Box<TypeInfo>,

    /// Whether the property is optional (can be undefined).
    #[serde(default)]
    pub optional: bool,

    /// Whether the property is readonly.
    #[serde(default)]
    pub readonly: bool,
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
            properties: Vec::new(),
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
            properties: Vec::new(),
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
            properties: Vec::new(),
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
            properties: Vec::new(),
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
            properties: Vec::new(),
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
            properties: Vec::new(),
        }
    }

    /// Create a literal type from a value.
    ///
    /// Literal types represent specific values like `"hello"`, `42`, `true`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind};
    ///
    /// let hello = TypeInfo::literal("'hello'");
    /// assert_eq!(hello.kind, TypeKind::Literal);
    /// assert_eq!(hello.repr, "'hello'");
    /// ```
    pub fn literal(value: impl Into<String>) -> Self {
        Self {
            kind: TypeKind::Literal,
            repr: value.into(),
            type_arguments: Vec::new(),
            constituents: Vec::new(),
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable: false,
            properties: Vec::new(),
        }
    }

    /// Create a union type from multiple constituent types.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind};
    ///
    /// let union = TypeInfo::union(vec![
    ///     TypeInfo::primitive("string"),
    ///     TypeInfo::primitive("number"),
    /// ]);
    /// assert_eq!(union.kind, TypeKind::Union);
    /// assert_eq!(union.repr, "string | number");
    /// ```
    pub fn union(constituents: Vec<TypeInfo>) -> Self {
        let repr = constituents
            .iter()
            .map(|t| t.repr.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        let nullable = constituents.iter().any(|t| {
            t.repr == "null" || t.repr == "undefined" || t.nullable
        });
        Self {
            kind: TypeKind::Union,
            repr,
            type_arguments: Vec::new(),
            constituents,
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable,
            properties: Vec::new(),
        }
    }

    /// Create an intersection type from multiple constituent types.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind};
    ///
    /// let intersection = TypeInfo::intersection(vec![
    ///     TypeInfo::reference("HasName"),
    ///     TypeInfo::reference("HasAge"),
    /// ]);
    /// assert_eq!(intersection.kind, TypeKind::Intersection);
    /// assert_eq!(intersection.repr, "HasName & HasAge");
    /// ```
    pub fn intersection(constituents: Vec<TypeInfo>) -> Self {
        let repr = constituents
            .iter()
            .map(|t| t.repr.as_str())
            .collect::<Vec<_>>()
            .join(" & ");
        Self {
            kind: TypeKind::Intersection,
            repr,
            type_arguments: Vec::new(),
            constituents,
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable: false,
            properties: Vec::new(),
        }
    }

    /// Create an object type with properties.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use interlint_format::{TypeInfo, TypeKind, ObjectProperty};
    ///
    /// let obj = TypeInfo::object(vec![
    ///     ObjectProperty {
    ///         name: "name".to_string(),
    ///         type_info: Box::new(TypeInfo::primitive("string")),
    ///         optional: false,
    ///         readonly: false,
    ///     },
    /// ]);
    /// assert_eq!(obj.kind, TypeKind::Object);
    /// ```
    pub fn object(properties: Vec<ObjectProperty>) -> Self {
        let repr = format!(
            "{{ {} }}",
            properties
                .iter()
                .map(|p| format!(
                    "{}{}{}",
                    if p.readonly { "readonly " } else { "" },
                    p.name,
                    if p.optional { "?" } else { "" }
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Self {
            kind: TypeKind::Object,
            repr,
            type_arguments: Vec::new(),
            constituents: Vec::new(),
            element_type: None,
            key_type: None,
            value_type: None,
            param_types: Vec::new(),
            return_type: None,
            nullable: false,
            properties,
        }
    }

    /// Check if this type is structurally compatible with another type
    /// in a given variance position.
    ///
    /// Returns true if replacing `self` with `other` would be safe.
    pub fn is_compatible_with(&self, other: &TypeInfo, variance: Variance) -> bool {
        // Same type is always compatible
        if self == other {
            return true;
        }

        // Bivariant is always compatible
        if variance == Variance::Bivariant {
            return true;
        }

        // Any type is compatible with anything
        if self.kind == TypeKind::Any || other.kind == TypeKind::Any {
            return true;
        }

        // Unknown is only compatible with unknown or any
        if self.kind == TypeKind::Unknown {
            return other.kind == TypeKind::Unknown || other.kind == TypeKind::Any;
        }

        // Never is subtype of everything
        if self.kind == TypeKind::Never {
            return variance.is_safe_to_narrow();
        }

        // Everything is subtype of never in contravariant position
        if other.kind == TypeKind::Never {
            return variance.is_safe_to_widen();
        }

        // For structural comparison, we need to check based on kind
        match (&self.kind, &other.kind) {
            // Primitives must match exactly (or be in a subtyping relationship)
            (TypeKind::Primitive, TypeKind::Primitive) => {
                self.repr == other.repr || self.is_primitive_subtype_of(other)
            }

            // Arrays are covariant in their element type
            (TypeKind::Array, TypeKind::Array) => {
                if let (Some(self_elem), Some(other_elem)) =
                    (&self.element_type, &other.element_type)
                {
                    self_elem.is_compatible_with(other_elem, variance)
                } else {
                    false
                }
            }

            // Unions: in covariant position, self must be subset of other
            // In contravariant position, other must be subset of self
            (TypeKind::Union, TypeKind::Union) => {
                match variance {
                    Variance::Covariant => {
                        // All self constituents must be in other
                        self.constituents.iter().all(|s| {
                            other.constituents.iter().any(|o| s.is_compatible_with(o, variance))
                        })
                    }
                    Variance::Contravariant => {
                        // All other constituents must be in self
                        other.constituents.iter().all(|o| {
                            self.constituents.iter().any(|s| s.is_compatible_with(o, Variance::Covariant))
                        })
                    }
                    Variance::Invariant => {
                        // Must be exactly the same
                        self.constituents.len() == other.constituents.len()
                            && self.constituents.iter().all(|s| {
                                other.constituents.iter().any(|o| s == o)
                            })
                    }
                    Variance::Bivariant => true,
                }
            }

            // Objects need structural comparison
            (TypeKind::Object, TypeKind::Object) => {
                self.is_object_compatible_with(other, variance)
            }

            // Different kinds are generally not compatible
            _ => false,
        }
    }

    /// Check if this primitive type is a subtype of another primitive.
    fn is_primitive_subtype_of(&self, other: &TypeInfo) -> bool {
        // Handle common subtyping relationships
        match (self.repr.as_str(), other.repr.as_str()) {
            // number literal subtypes
            ("integer", "number") => true,
            ("float", "number") => true,
            // string literal subtypes
            (s, "string") if s.starts_with('"') && s.ends_with('"') => true,
            // boolean literal subtypes
            ("true", "boolean") | ("false", "boolean") => true,
            // void to undefined (TypeScript)
            ("void", "undefined") => true,
            _ => false,
        }
    }

    /// Check object structural compatibility.
    fn is_object_compatible_with(&self, other: &TypeInfo, variance: Variance) -> bool {
        match variance {
            Variance::Covariant => {
                // Self must have all required properties of other with compatible types
                for other_prop in &other.properties {
                    if other_prop.optional {
                        continue; // Optional properties don't need to exist
                    }
                    let self_prop = self.properties.iter().find(|p| p.name == other_prop.name);
                    match self_prop {
                        Some(sp) => {
                            // Check type compatibility (property read is covariant)
                            let prop_variance = if sp.readonly && other_prop.readonly {
                                Variance::Covariant
                            } else {
                                Variance::Invariant
                            };
                            if !sp.type_info.is_compatible_with(&other_prop.type_info, prop_variance) {
                                return false;
                            }
                        }
                        None => return false, // Missing required property
                    }
                }
                true
            }
            Variance::Contravariant => {
                // Other must have all required properties of self
                other.is_object_compatible_with(self, Variance::Covariant)
            }
            Variance::Invariant => {
                // Must have same properties with same types
                if self.properties.len() != other.properties.len() {
                    return false;
                }
                for self_prop in &self.properties {
                    let other_prop = other.properties.iter().find(|p| p.name == self_prop.name);
                    match other_prop {
                        Some(op) => {
                            if self_prop.optional != op.optional
                                || self_prop.readonly != op.readonly
                                || !self_prop.type_info.is_compatible_with(&op.type_info, Variance::Invariant)
                            {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                true
            }
            Variance::Bivariant => true,
        }
    }
}
