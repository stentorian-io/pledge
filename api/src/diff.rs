//! Diff detection logic for comparing public interfaces.
//!
//! This module implements comprehensive breaking change detection including:
//!
//! - **Structural Type Comparison**: Deep comparison of types beyond string equality
//! - **Variance-aware Analysis**: Different rules for input vs output positions
//! - **Union/Intersection Tracking**: Detect member additions/removals
//! - **Nullable Type Changes**: Track changes to nullability
//! - **Object Shape Compatibility**: Structural subtyping checks

use std::collections::{HashMap, HashSet};

use interlint_format::{
    ClassSig, EnumSig, Export, FunctionSig, InterfaceSig, Module, Parameter, Property,
    PublicInterface, Signature, TypeAliasSig, TypeInfo, TypeKind, VariableSig, Variance,
};

use crate::response::{BreakingChange, BreakingChangeType, Severity};

// =============================================================================
// Structural Type Comparison
// =============================================================================

/// Result of a structural type comparison.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TypeComparisonResult {
    /// Whether the types are compatible
    pub compatible: bool,
    /// List of specific differences found
    pub differences: Vec<TypeDifference>,
}

/// A specific difference found between two types.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TypeDifference {
    /// Type of difference
    pub kind: TypeDifferenceKind,
    /// Path to the difference (e.g., "return_type", "param[0]")
    pub path: String,
    /// Description of the difference
    pub description: String,
    /// Previous value/type
    pub previous: Option<String>,
    /// Current value/type
    pub current: Option<String>,
}

/// Kind of type difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TypeDifferenceKind {
    /// Type kind changed (e.g., primitive -> object)
    KindChanged,
    /// Primitive type changed
    PrimitiveChanged,
    /// Array element type changed
    ArrayElementChanged,
    /// Union member added
    UnionMemberAdded,
    /// Union member removed
    UnionMemberRemoved,
    /// Intersection member added
    IntersectionMemberAdded,
    /// Intersection member removed
    IntersectionMemberRemoved,
    /// Object property added
    ObjectPropertyAdded,
    /// Object property removed
    ObjectPropertyRemoved,
    /// Object property type changed
    ObjectPropertyTypeChanged,
    /// Object property became required
    ObjectPropertyBecameRequired,
    /// Object property became optional
    ObjectPropertyBecameOptional,
    /// Type became nullable
    BecameNullable,
    /// Type became non-nullable
    BecameNonNullable,
    /// Generic type argument changed
    TypeArgumentChanged,
    /// Generic variance violation
    VarianceViolation,
    /// Function parameter type changed
    FunctionParamChanged,
    /// Function return type changed
    FunctionReturnChanged,
    /// Reference target changed
    ReferenceChanged,
}

/// Compare two types structurally with variance awareness.
///
/// This function performs deep structural comparison of types, considering:
/// - Type kinds (primitive, object, union, etc.)
/// - Variance positions (covariant for returns, contravariant for params)
/// - Structural subtyping for objects
/// - Union/intersection member tracking
///
/// # Arguments
///
/// * `prev` - The previous type
/// * `curr` - The current type
/// * `variance` - The variance position (covariant, contravariant, invariant)
/// * `path` - Path for error reporting (e.g., "MyFunction.return_type")
///
/// # Returns
///
/// A `TypeComparisonResult` containing compatibility status and detailed differences.
pub fn compare_types_structural(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
) -> TypeComparisonResult {
    let mut result = TypeComparisonResult {
        compatible: true,
        differences: Vec::new(),
    };

    // Fast path: identical types
    if prev == curr {
        return result;
    }

    // Check string representation equality first (handles most cases)
    if prev.repr == curr.repr && prev.kind == curr.kind {
        // Check nullability changes
        if prev.nullable != curr.nullable {
            if curr.nullable && !prev.nullable {
                // Type became nullable - safe in covariant position
                if !variance.is_safe_to_widen() {
                    result.compatible = false;
                }
                result.differences.push(TypeDifference {
                    kind: TypeDifferenceKind::BecameNullable,
                    path: path.to_string(),
                    description: format!("Type at '{}' became nullable", path),
                    previous: Some(prev.repr.clone()),
                    current: Some(format!("{} | null", curr.repr)),
                });
            } else {
                // Type became non-nullable - breaking in covariant position
                if !variance.is_safe_to_narrow() {
                    result.compatible = false;
                }
                result.differences.push(TypeDifference {
                    kind: TypeDifferenceKind::BecameNonNullable,
                    path: path.to_string(),
                    description: format!("Type at '{}' became non-nullable", path),
                    previous: Some(format!("{} | null", prev.repr)),
                    current: Some(curr.repr.clone()),
                });
            }
        }
        return result;
    }

    // Handle Any type - compatible with anything
    if prev.kind == TypeKind::Any || curr.kind == TypeKind::Any {
        return result;
    }

    // Handle Never type
    if prev.kind == TypeKind::Never || curr.kind == TypeKind::Never {
        // Never is subtype of everything, supertype of nothing
        if prev.kind == TypeKind::Never && variance.is_safe_to_narrow() {
            return result;
        }
        if curr.kind == TypeKind::Never && variance.is_safe_to_widen() {
            return result;
        }
        result.compatible = false;
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::KindChanged,
            path: path.to_string(),
            description: format!("Type at '{}' changed involving 'never' type", path),
            previous: Some(prev.repr.clone()),
            current: Some(curr.repr.clone()),
        });
        return result;
    }

    // Check if kinds match
    if prev.kind != curr.kind {
        // Special case: union containing the other type
        if curr.kind == TypeKind::Union && curr.constituents.iter().any(|c| types_equal(c, prev)) {
            // Type was widened to a union containing the old type
            if variance.is_safe_to_widen() {
                result.differences.push(TypeDifference {
                    kind: TypeDifferenceKind::UnionMemberAdded,
                    path: path.to_string(),
                    description: format!("Type at '{}' was widened to union", path),
                    previous: Some(prev.repr.clone()),
                    current: Some(curr.repr.clone()),
                });
                return result;
            }
        }

        if prev.kind == TypeKind::Union && prev.constituents.iter().any(|c| types_equal(c, curr)) {
            // Type was narrowed from a union
            if variance.is_safe_to_narrow() {
                result.differences.push(TypeDifference {
                    kind: TypeDifferenceKind::UnionMemberRemoved,
                    path: path.to_string(),
                    description: format!("Type at '{}' was narrowed from union", path),
                    previous: Some(prev.repr.clone()),
                    current: Some(curr.repr.clone()),
                });
                return result;
            }
        }

        result.compatible = false;
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::KindChanged,
            path: path.to_string(),
            description: format!(
                "Type kind at '{}' changed from {:?} to {:?}",
                path, prev.kind, curr.kind
            ),
            previous: Some(format!("{:?}", prev.kind)),
            current: Some(format!("{:?}", curr.kind)),
        });
        return result;
    }

    // Compare based on kind
    match prev.kind {
        TypeKind::Primitive => {
            compare_primitive_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Array => {
            compare_array_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Union => {
            compare_union_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Intersection => {
            compare_intersection_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Object => {
            compare_object_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Function => {
            compare_function_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Generic => {
            compare_generic_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Reference => {
            compare_reference_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Tuple => {
            compare_tuple_types(prev, curr, variance, path, &mut result);
        }
        TypeKind::Map => {
            compare_map_types(prev, curr, variance, path, &mut result);
        }
        _ => {
            // For other kinds, fall back to repr comparison
            if prev.repr != curr.repr {
                result.compatible = false;
                result.differences.push(TypeDifference {
                    kind: TypeDifferenceKind::KindChanged,
                    path: path.to_string(),
                    description: format!("Type at '{}' changed", path),
                    previous: Some(prev.repr.clone()),
                    current: Some(curr.repr.clone()),
                });
            }
        }
    }

    // Check nullability
    if prev.nullable != curr.nullable && result.compatible {
        if curr.nullable && !prev.nullable {
            if !variance.is_safe_to_widen() {
                result.compatible = false;
            }
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::BecameNullable,
                path: path.to_string(),
                description: format!("Type at '{}' became nullable", path),
                previous: Some(prev.repr.clone()),
                current: Some(curr.repr.clone()),
            });
        } else if !curr.nullable && prev.nullable {
            if !variance.is_safe_to_narrow() {
                result.compatible = false;
            }
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::BecameNonNullable,
                path: path.to_string(),
                description: format!("Type at '{}' became non-nullable", path),
                previous: Some(prev.repr.clone()),
                current: Some(curr.repr.clone()),
            });
        }
    }

    result
}

/// Compare primitive types.
fn compare_primitive_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    if prev.repr != curr.repr {
        // Check if it's a valid subtype relationship
        let is_subtype = is_primitive_subtype(&curr.repr, &prev.repr);
        let is_supertype = is_primitive_subtype(&prev.repr, &curr.repr);

        if is_subtype && variance.is_safe_to_narrow() {
            // Narrowing is safe in contravariant position
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::PrimitiveChanged,
                path: path.to_string(),
                description: format!("Primitive type at '{}' narrowed", path),
                previous: Some(prev.repr.clone()),
                current: Some(curr.repr.clone()),
            });
        } else if is_supertype && variance.is_safe_to_widen() {
            // Widening is safe in covariant position
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::PrimitiveChanged,
                path: path.to_string(),
                description: format!("Primitive type at '{}' widened", path),
                previous: Some(prev.repr.clone()),
                current: Some(curr.repr.clone()),
            });
        } else {
            result.compatible = false;
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::PrimitiveChanged,
                path: path.to_string(),
                description: format!(
                    "Primitive type at '{}' changed from '{}' to '{}'",
                    path, prev.repr, curr.repr
                ),
                previous: Some(prev.repr.clone()),
                current: Some(curr.repr.clone()),
            });
        }
    }
}

/// Check if one primitive type is a subtype of another.
fn is_primitive_subtype(subtype: &str, supertype: &str) -> bool {
    match (subtype, supertype) {
        // Number subtypes
        ("integer", "number") => true,
        ("float", "number") => true,
        ("bigint", "number") => false, // bigint is not a subtype of number
        // String literal subtypes
        (s, "string") if s.starts_with('"') && s.ends_with('"') => true,
        // Boolean literal subtypes
        ("true", "boolean") | ("false", "boolean") => true,
        // void/undefined relationship
        ("void", "undefined") => true,
        ("undefined", "void") => true,
        // null handling
        ("null", _) => false, // null is not a subtype of other types
        // Same type
        (a, b) if a == b => true,
        _ => false,
    }
}

/// Compare array types.
fn compare_array_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    match (&prev.element_type, &curr.element_type) {
        (Some(prev_elem), Some(curr_elem)) => {
            // Arrays are covariant in their element type (for reading)
            // but technically invariant if you consider both read and write
            let elem_path = format!("{}[]", path);
            let elem_result = compare_types_structural(prev_elem, curr_elem, variance, &elem_path);

            if !elem_result.compatible {
                result.compatible = false;
            }
            result.differences.extend(elem_result.differences);
        }
        _ => {
            if prev.repr != curr.repr {
                result.compatible = false;
                result.differences.push(TypeDifference {
                    kind: TypeDifferenceKind::ArrayElementChanged,
                    path: path.to_string(),
                    description: format!("Array type at '{}' changed", path),
                    previous: Some(prev.repr.clone()),
                    current: Some(curr.repr.clone()),
                });
            }
        }
    }
}

/// Compare union types.
fn compare_union_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    let prev_members: HashSet<&str> = prev.constituents.iter().map(|t| t.repr.as_str()).collect();
    let curr_members: HashSet<&str> = curr.constituents.iter().map(|t| t.repr.as_str()).collect();

    // Find added members
    for member in curr_members.difference(&prev_members) {
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::UnionMemberAdded,
            path: path.to_string(),
            description: format!("Union member '{}' was added at '{}'", member, path),
            previous: None,
            current: Some(member.to_string()),
        });

        // Adding union members widens the type
        // Breaking in contravariant position (parameters)
        if !variance.is_safe_to_widen() {
            result.compatible = false;
        }
    }

    // Find removed members
    for member in prev_members.difference(&curr_members) {
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::UnionMemberRemoved,
            path: path.to_string(),
            description: format!("Union member '{}' was removed from '{}'", member, path),
            previous: Some(member.to_string()),
            current: None,
        });

        // Removing union members narrows the type
        // Breaking in covariant position (return types)
        if !variance.is_safe_to_narrow() {
            result.compatible = false;
        }
    }
}

/// Compare intersection types.
fn compare_intersection_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    let prev_members: HashSet<&str> = prev.constituents.iter().map(|t| t.repr.as_str()).collect();
    let curr_members: HashSet<&str> = curr.constituents.iter().map(|t| t.repr.as_str()).collect();

    // Find added members - makes the type narrower (more constraints)
    for member in curr_members.difference(&prev_members) {
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::IntersectionMemberAdded,
            path: path.to_string(),
            description: format!("Intersection member '{}' was added at '{}'", member, path),
            previous: None,
            current: Some(member.to_string()),
        });

        // Adding intersection members narrows the type
        // Breaking in covariant position (return types)
        if !variance.is_safe_to_narrow() {
            result.compatible = false;
        }
    }

    // Find removed members - makes the type wider (fewer constraints)
    for member in prev_members.difference(&curr_members) {
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::IntersectionMemberRemoved,
            path: path.to_string(),
            description: format!("Intersection member '{}' was removed from '{}'", member, path),
            previous: Some(member.to_string()),
            current: None,
        });

        // Removing intersection members widens the type
        // Breaking in contravariant position (parameters)
        if !variance.is_safe_to_widen() {
            result.compatible = false;
        }
    }
}

/// Compare object types (structural comparison).
fn compare_object_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    let prev_props: HashMap<&str, _> = prev.properties.iter().map(|p| (p.name.as_str(), p)).collect();
    let curr_props: HashMap<&str, _> = curr.properties.iter().map(|p| (p.name.as_str(), p)).collect();

    // Check for removed properties
    for (name, prev_prop) in &prev_props {
        if !curr_props.contains_key(name) {
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::ObjectPropertyRemoved,
                path: format!("{}.{}", path, name),
                description: format!("Property '{}' was removed from object at '{}'", name, path),
                previous: Some(prev_prop.type_info.repr.clone()),
                current: None,
            });

            // Removing required property is breaking in covariant position
            if !prev_prop.optional && !variance.is_safe_to_narrow() {
                result.compatible = false;
            }
        }
    }

    // Check for added properties
    for (name, curr_prop) in &curr_props {
        if !prev_props.contains_key(name) {
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::ObjectPropertyAdded,
                path: format!("{}.{}", path, name),
                description: format!("Property '{}' was added to object at '{}'", name, path),
                previous: None,
                current: Some(curr_prop.type_info.repr.clone()),
            });

            // Adding required property is breaking in contravariant position
            if !curr_prop.optional && !variance.is_safe_to_widen() {
                result.compatible = false;
            }
        }
    }

    // Compare properties that exist in both
    for (name, prev_prop) in &prev_props {
        if let Some(curr_prop) = curr_props.get(name) {
            let prop_path = format!("{}.{}", path, name);

            // Determine property variance
            let prop_variance = if prev_prop.readonly && curr_prop.readonly {
                variance // readonly properties preserve variance
            } else {
                Variance::Invariant // mutable properties are invariant
            };

            // Compare types
            let prop_result = compare_types_structural(
                &prev_prop.type_info,
                &curr_prop.type_info,
                prop_variance,
                &prop_path,
            );

            if !prop_result.compatible {
                result.compatible = false;
            }
            result.differences.extend(prop_result.differences);

            // Check optional -> required
            if prev_prop.optional && !curr_prop.optional {
                result.differences.push(TypeDifference {
                    kind: TypeDifferenceKind::ObjectPropertyBecameRequired,
                    path: prop_path.clone(),
                    description: format!("Property '{}' became required", name),
                    previous: Some("optional".to_string()),
                    current: Some("required".to_string()),
                });

                // Property becoming required is breaking in contravariant position
                if !variance.is_safe_to_narrow() {
                    result.compatible = false;
                }
            }

            // Check required -> optional
            if !prev_prop.optional && curr_prop.optional {
                result.differences.push(TypeDifference {
                    kind: TypeDifferenceKind::ObjectPropertyBecameOptional,
                    path: prop_path,
                    description: format!("Property '{}' became optional", name),
                    previous: Some("required".to_string()),
                    current: Some("optional".to_string()),
                });

                // Property becoming optional is breaking in covariant position
                if !variance.is_safe_to_widen() {
                    result.compatible = false;
                }
            }
        }
    }
}

/// Compare function types.
fn compare_function_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    _variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    // Compare parameter types (contravariant)
    let prev_params = &prev.param_types;
    let curr_params = &curr.param_types;

    let min_len = prev_params.len().min(curr_params.len());

    for i in 0..min_len {
        let param_path = format!("{}(param[{}])", path, i);
        let param_result = compare_types_structural(
            &prev_params[i],
            &curr_params[i],
            Variance::Contravariant,
            &param_path,
        );

        if !param_result.compatible {
            result.compatible = false;
        }
        result.differences.extend(param_result.differences);
    }

    // Check for added parameters
    if curr_params.len() > prev_params.len() {
        for i in min_len..curr_params.len() {
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::FunctionParamChanged,
                path: format!("{}(param[{}])", path, i),
                description: format!("Parameter {} was added to function", i),
                previous: None,
                current: Some(curr_params[i].repr.clone()),
            });
        }
        result.compatible = false;
    }

    // Check for removed parameters
    if prev_params.len() > curr_params.len() {
        for i in min_len..prev_params.len() {
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::FunctionParamChanged,
                path: format!("{}(param[{}])", path, i),
                description: format!("Parameter {} was removed from function", i),
                previous: Some(prev_params[i].repr.clone()),
                current: None,
            });
        }
        // Removing parameters may be safe if they were optional
    }

    // Compare return types (covariant)
    match (&prev.return_type, &curr.return_type) {
        (Some(prev_ret), Some(curr_ret)) => {
            let ret_path = format!("{}.return", path);
            let ret_result = compare_types_structural(
                prev_ret,
                curr_ret,
                Variance::Covariant,
                &ret_path,
            );

            if !ret_result.compatible {
                result.compatible = false;
            }
            result.differences.extend(ret_result.differences);
        }
        (None, Some(curr_ret)) => {
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::FunctionReturnChanged,
                path: format!("{}.return", path),
                description: "Return type was added".to_string(),
                previous: None,
                current: Some(curr_ret.repr.clone()),
            });
        }
        (Some(prev_ret), None) => {
            result.differences.push(TypeDifference {
                kind: TypeDifferenceKind::FunctionReturnChanged,
                path: format!("{}.return", path),
                description: "Return type was removed".to_string(),
                previous: Some(prev_ret.repr.clone()),
                current: None,
            });
            result.compatible = false;
        }
        (None, None) => {}
    }
}

/// Compare generic types.
fn compare_generic_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    // Compare type arguments
    let prev_args = &prev.type_arguments;
    let curr_args = &curr.type_arguments;

    if prev_args.len() != curr_args.len() {
        result.compatible = false;
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::TypeArgumentChanged,
            path: path.to_string(),
            description: format!(
                "Number of type arguments changed from {} to {}",
                prev_args.len(),
                curr_args.len()
            ),
            previous: Some(prev_args.len().to_string()),
            current: Some(curr_args.len().to_string()),
        });
        return;
    }

    for (i, (prev_arg, curr_arg)) in prev_args.iter().zip(curr_args.iter()).enumerate() {
        let arg_path = format!("{}<{}>", path, i);
        // Type arguments are invariant by default (we don't know the variance)
        let arg_result = compare_types_structural(prev_arg, curr_arg, variance, &arg_path);

        if !arg_result.compatible {
            result.compatible = false;
        }
        result.differences.extend(arg_result.differences);
    }
}

/// Compare reference types.
fn compare_reference_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    _variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    if prev.repr != curr.repr {
        result.compatible = false;
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::ReferenceChanged,
            path: path.to_string(),
            description: format!(
                "Type reference at '{}' changed from '{}' to '{}'",
                path, prev.repr, curr.repr
            ),
            previous: Some(prev.repr.clone()),
            current: Some(curr.repr.clone()),
        });
    }
}

/// Compare tuple types.
fn compare_tuple_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    let prev_elems = &prev.type_arguments;
    let curr_elems = &curr.type_arguments;

    if prev_elems.len() != curr_elems.len() {
        result.compatible = false;
        result.differences.push(TypeDifference {
            kind: TypeDifferenceKind::KindChanged,
            path: path.to_string(),
            description: format!(
                "Tuple length changed from {} to {}",
                prev_elems.len(),
                curr_elems.len()
            ),
            previous: Some(prev_elems.len().to_string()),
            current: Some(curr_elems.len().to_string()),
        });
        return;
    }

    for (i, (prev_elem, curr_elem)) in prev_elems.iter().zip(curr_elems.iter()).enumerate() {
        let elem_path = format!("{}[{}]", path, i);
        let elem_result = compare_types_structural(prev_elem, curr_elem, variance, &elem_path);

        if !elem_result.compatible {
            result.compatible = false;
        }
        result.differences.extend(elem_result.differences);
    }
}

/// Compare map types.
fn compare_map_types(
    prev: &TypeInfo,
    curr: &TypeInfo,
    variance: Variance,
    path: &str,
    result: &mut TypeComparisonResult,
) {
    // Compare key types (invariant - used in both lookup and storage)
    if let (Some(prev_key), Some(curr_key)) = (&prev.key_type, &curr.key_type) {
        let key_path = format!("{}.key", path);
        let key_result = compare_types_structural(prev_key, curr_key, Variance::Invariant, &key_path);

        if !key_result.compatible {
            result.compatible = false;
        }
        result.differences.extend(key_result.differences);
    }

    // Compare value types (follows the outer variance)
    if let (Some(prev_val), Some(curr_val)) = (&prev.value_type, &curr.value_type) {
        let val_path = format!("{}.value", path);
        let val_result = compare_types_structural(prev_val, curr_val, variance, &val_path);

        if !val_result.compatible {
            result.compatible = false;
        }
        result.differences.extend(val_result.differences);
    }
}

/// Convert type differences to breaking changes.
#[allow(dead_code)]
pub fn differences_to_breaking_changes(
    differences: &[TypeDifference],
    base_path: &str,
    name: &str,
) -> Vec<BreakingChange> {
    differences
        .iter()
        .map(|diff| {
            let change_type = match diff.kind {
                TypeDifferenceKind::UnionMemberRemoved => BreakingChangeType::UnionMemberRemoved,
                TypeDifferenceKind::UnionMemberAdded => BreakingChangeType::UnionMemberAdded,
                TypeDifferenceKind::IntersectionMemberAdded => BreakingChangeType::IntersectionMemberAdded,
                TypeDifferenceKind::IntersectionMemberRemoved => BreakingChangeType::IntersectionMemberRemoved,
                TypeDifferenceKind::BecameNullable => BreakingChangeType::TypeBecameNullable,
                TypeDifferenceKind::BecameNonNullable => BreakingChangeType::TypeBecameNonNullable,
                TypeDifferenceKind::ObjectPropertyAdded => BreakingChangeType::PropertyAdded,
                TypeDifferenceKind::ObjectPropertyRemoved => BreakingChangeType::PropertyRemoved,
                TypeDifferenceKind::ObjectPropertyTypeChanged => BreakingChangeType::PropertyTypeChanged,
                TypeDifferenceKind::ObjectPropertyBecameRequired => BreakingChangeType::PropertyBecameRequired,
                TypeDifferenceKind::ObjectPropertyBecameOptional => BreakingChangeType::PropertyBecameOptional,
                TypeDifferenceKind::VarianceViolation => BreakingChangeType::GenericVarianceViolation,
                _ => BreakingChangeType::TypeChanged,
            };

            let mut change = BreakingChange::new(
                change_type,
                format!("{}#{}", base_path, diff.path),
                name,
                diff.description.clone(),
            );

            if let Some(ref prev) = diff.previous {
                change = change.with_previous(prev);
            }
            if let Some(ref curr) = diff.current {
                change = change.with_current(curr);
            }

            change
        })
        .collect()
}

/// Compare two public interfaces and return a list of breaking changes.
pub fn compare_interfaces(
    previous: &PublicInterface,
    current: &PublicInterface,
    include_additions: bool,
) -> Vec<BreakingChange> {
    let mut changes = Vec::new();

    // Compare entrypoints
    compare_entrypoints(previous, current, &mut changes, include_additions);

    // Compare modules
    compare_modules(previous, current, &mut changes, include_additions);

    // Compare assets
    compare_assets(previous, current, &mut changes, include_additions);

    // Sort by severity (most severe first)
    changes.sort_by(|a, b| b.severity.weight().cmp(&a.severity.weight()));

    changes
}

/// Compare entrypoints between interfaces.
fn compare_entrypoints(
    previous: &PublicInterface,
    current: &PublicInterface,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_entrypoints: HashSet<&str> = previous.entrypoints.iter().map(|e| e.name.as_str()).collect();
    let curr_entrypoints: HashSet<&str> = current.entrypoints.iter().map(|e| e.name.as_str()).collect();

    // Check for removed entrypoints
    for name in prev_entrypoints.difference(&curr_entrypoints) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::EntrypointRemoved,
                format!("entrypoint:{}", name),
                *name,
                format!("Entrypoint '{}' was removed", name),
            )
            .with_suggestion("Ensure consumers are not importing from this entrypoint"),
        );
    }

    // Check for added entrypoints
    if include_additions {
        for name in curr_entrypoints.difference(&prev_entrypoints) {
            changes.push(BreakingChange::new(
                BreakingChangeType::EntrypointAdded,
                format!("entrypoint:{}", name),
                *name,
                format!("Entrypoint '{}' was added", name),
            ));
        }
    }
}

/// Compare modules between interfaces.
fn compare_modules(
    previous: &PublicInterface,
    current: &PublicInterface,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_modules: HashMap<&str, &Module> =
        previous.modules.iter().map(|m| (m.path.as_str(), m)).collect();
    let curr_modules: HashMap<&str, &Module> =
        current.modules.iter().map(|m| (m.path.as_str(), m)).collect();

    // Check for removed modules
    for (path, _module) in &prev_modules {
        if !curr_modules.contains_key(path) {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::ModuleRemoved,
                    *path,
                    *path,
                    format!("Module '{}' was removed", path),
                )
                .with_suggestion("Ensure consumers are not importing from this module"),
            );
        }
    }

    // Check for added modules
    if include_additions {
        for (path, _module) in &curr_modules {
            if !prev_modules.contains_key(path) {
                changes.push(BreakingChange::new(
                    BreakingChangeType::ModuleAdded,
                    *path,
                    *path,
                    format!("Module '{}' was added", path),
                ));
            }
        }
    }

    // Compare exports in modules that exist in both
    for (path, prev_module) in &prev_modules {
        if let Some(curr_module) = curr_modules.get(path) {
            compare_exports(prev_module, curr_module, changes, include_additions);
        }
    }
}

/// Compare exports between two modules.
fn compare_exports(
    prev_module: &Module,
    curr_module: &Module,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_exports: HashMap<&str, &Export> =
        prev_module.exports.iter().map(|e| (e.name.as_str(), e)).collect();
    let curr_exports: HashMap<&str, &Export> =
        curr_module.exports.iter().map(|e| (e.name.as_str(), e)).collect();

    // Check for removed exports
    for (name, prev_export) in &prev_exports {
        if !curr_exports.contains_key(name) {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::ExportRemoved,
                    format!("{}#{}", prev_module.path, name),
                    *name,
                    format!("Export '{}' was removed from '{}'", name, prev_module.path),
                )
                .with_previous(signature_kind_name(&prev_export.signature))
                .with_suggestion(format!(
                    "Remove usages of '{}' or provide a migration path",
                    name
                )),
            );
        }
    }

    // Check for added exports
    if include_additions {
        for (name, _export) in &curr_exports {
            if !prev_exports.contains_key(name) {
                changes.push(BreakingChange::new(
                    BreakingChangeType::ExportAdded,
                    format!("{}#{}", curr_module.path, name),
                    *name,
                    format!("Export '{}' was added to '{}'", name, curr_module.path),
                ));
            }
        }
    }

    // Compare exports that exist in both
    for (name, prev_export) in &prev_exports {
        if let Some(curr_export) = curr_exports.get(name) {
            compare_export_signatures(
                prev_export,
                curr_export,
                &curr_module.path,
                changes,
                include_additions,
            );
        }
    }
}

/// Compare signatures of two exports.
fn compare_export_signatures(
    prev: &Export,
    curr: &Export,
    module_path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let path = format!("{}#{}", module_path, prev.name);

    // Check if default export status changed
    if prev.is_default != curr.is_default {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::DefaultExportChanged,
                &path,
                &prev.name,
                format!(
                    "Default export status changed for '{}' from {} to {}",
                    prev.name, prev.is_default, curr.is_default
                ),
            )
            .with_previous(prev.is_default.to_string())
            .with_current(curr.is_default.to_string()),
        );
    }

    // Check if export kind changed
    if std::mem::discriminant(&prev.signature) != std::mem::discriminant(&curr.signature) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::ExportKindChanged,
                &path,
                &prev.name,
                format!(
                    "Export '{}' changed from {} to {}",
                    prev.name,
                    signature_kind_name(&prev.signature),
                    signature_kind_name(&curr.signature)
                ),
            )
            .with_previous(signature_kind_name(&prev.signature))
            .with_current(signature_kind_name(&curr.signature)),
        );
        return; // Don't compare further if kind changed
    }

    // Compare based on signature type
    match (&prev.signature, &curr.signature) {
        (Signature::Function(prev_fn), Signature::Function(curr_fn)) => {
            compare_function_signatures(prev_fn, curr_fn, &path, changes, include_additions);
        }
        (Signature::Class(prev_class), Signature::Class(curr_class)) => {
            compare_class_signatures(prev_class, curr_class, &path, changes, include_additions);
        }
        (Signature::Interface(prev_iface), Signature::Interface(curr_iface)) => {
            compare_interface_signatures(prev_iface, curr_iface, &path, changes, include_additions);
        }
        (Signature::Enum(prev_enum), Signature::Enum(curr_enum)) => {
            compare_enum_signatures(prev_enum, curr_enum, &path, changes, include_additions);
        }
        (Signature::TypeAlias(prev_alias), Signature::TypeAlias(curr_alias)) => {
            compare_type_alias_signatures(prev_alias, curr_alias, &path, changes);
        }
        (Signature::Variable(prev_var), Signature::Variable(curr_var)) => {
            compare_variable_signatures(prev_var, curr_var, &path, changes);
        }
        (Signature::Namespace(prev_ns), Signature::Namespace(curr_ns)) => {
            // For namespaces, recursively compare exports
            compare_namespace_exports(
                &prev_ns.exports,
                &curr_ns.exports,
                &path,
                changes,
                include_additions,
            );
        }
        _ => {}
    }
}

/// Compare function signatures.
fn compare_function_signatures(
    prev: &FunctionSig,
    curr: &FunctionSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    // Check async modifier
    if prev.is_async != curr.is_async {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::AsyncModifierChanged,
                path,
                &prev.name,
                format!(
                    "Function '{}' async modifier changed from {} to {}",
                    prev.name, prev.is_async, curr.is_async
                ),
            )
            .with_previous(format!("async: {}", prev.is_async))
            .with_current(format!("async: {}", curr.is_async)),
        );
    }

    // Check return type with variance-aware comparison (covariant)
    let return_comparison = compare_types_structural(
        &prev.return_type,
        &curr.return_type,
        Variance::Covariant,
        &format!("{}.return", path),
    );

    if !return_comparison.compatible {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::ReturnTypeChanged,
                path,
                &prev.name,
                format!(
                    "Return type of '{}' changed from '{}' to '{}'",
                    prev.name, prev.return_type.repr, curr.return_type.repr
                ),
            )
            .with_previous(&prev.return_type.repr)
            .with_current(&curr.return_type.repr),
        );
    } else if !return_comparison.differences.is_empty() {
        // Add informational changes for compatible but different types
        for diff in &return_comparison.differences {
            let change_type = match diff.kind {
                TypeDifferenceKind::UnionMemberAdded => BreakingChangeType::UnionMemberAdded,
                TypeDifferenceKind::BecameNullable => BreakingChangeType::TypeBecameNullable,
                _ => continue, // Skip non-breaking differences
            };
            let mut change = BreakingChange::new(
                change_type,
                path,
                &prev.name,
                diff.description.clone(),
            )
            .with_severity(Severity::Warning);

            if let Some(ref prev_val) = diff.previous {
                change = change.with_previous(prev_val);
            }
            if let Some(ref curr_val) = diff.current {
                change = change.with_current(curr_val);
            }
            changes.push(change);
        }
    }

    // Compare parameters (with variance awareness)
    compare_parameters_with_variance(
        &prev.parameters,
        &curr.parameters,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare type parameters
    compare_type_parameters(
        &prev.type_parameters,
        &curr.type_parameters,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Check visibility
    if prev.visibility != curr.visibility {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::VisibilityChanged,
                path,
                &prev.name,
                format!(
                    "Visibility of '{}' changed from {:?} to {:?}",
                    prev.name, prev.visibility, curr.visibility
                ),
            )
            .with_previous(format!("{:?}", prev.visibility))
            .with_current(format!("{:?}", curr.visibility)),
        );
    }
}

/// Compare parameters with variance-aware type checking.
fn compare_parameters_with_variance(
    prev: &[Parameter],
    curr: &[Parameter],
    path: &str,
    fn_name: &str,
    changes: &mut Vec<BreakingChange>,
    _include_additions: bool,
) {
    let prev_params: HashMap<&str, &Parameter> =
        prev.iter().map(|p| (p.name.as_str(), p)).collect();
    let curr_params: HashMap<&str, &Parameter> =
        curr.iter().map(|p| (p.name.as_str(), p)).collect();

    // Check for removed parameters
    for (name, _param) in &prev_params {
        if !curr_params.contains_key(name) {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::ParameterRemoved,
                    path,
                    fn_name,
                    format!("Parameter '{}' was removed from '{}'", name, fn_name),
                )
                .with_previous(format!("parameter: {}", name)),
            );
        }
    }

    // Check for added required parameters
    for (name, param) in &curr_params {
        if !prev_params.contains_key(name) {
            let severity = if param.optional || param.default_value.is_some() {
                Severity::Warning
            } else {
                Severity::Critical
            };
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::ParameterAdded,
                    path,
                    fn_name,
                    format!(
                        "Parameter '{}' was added to '{}' ({})",
                        name,
                        fn_name,
                        if param.optional { "optional" } else { "required" }
                    ),
                )
                .with_severity(severity)
                .with_current(format!("parameter: {} ({})", name, param.type_info.repr)),
            );
        }
    }

    // Compare parameters that exist in both (with contravariant type checking)
    for (name, prev_param) in &prev_params {
        if let Some(curr_param) = curr_params.get(name) {
            // Check type change with variance-aware comparison
            // Parameters are contravariant - widening is safe, narrowing breaks callers
            let type_comparison = compare_types_structural(
                &prev_param.type_info,
                &curr_param.type_info,
                Variance::Contravariant,
                &format!("{}.{}", path, name),
            );

            if !type_comparison.compatible {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::ParameterTypeChanged,
                        path,
                        fn_name,
                        format!(
                            "Type of parameter '{}' in '{}' changed from '{}' to '{}'",
                            name, fn_name, prev_param.type_info.repr, curr_param.type_info.repr
                        ),
                    )
                    .with_previous(&prev_param.type_info.repr)
                    .with_current(&curr_param.type_info.repr),
                );
            } else {
                // Report structural changes that are safe but noteworthy
                for diff in &type_comparison.differences {
                    let change_type = match diff.kind {
                        TypeDifferenceKind::UnionMemberRemoved => BreakingChangeType::UnionMemberRemoved,
                        TypeDifferenceKind::BecameNonNullable => BreakingChangeType::TypeBecameNonNullable,
                        _ => continue,
                    };

                    let mut change = BreakingChange::new(
                        change_type,
                        path,
                        fn_name,
                        diff.description.clone(),
                    )
                    .with_severity(Severity::Warning);

                    if let Some(ref prev_val) = diff.previous {
                        change = change.with_previous(prev_val);
                    }
                    if let Some(ref curr_val) = diff.current {
                        change = change.with_current(curr_val);
                    }
                    changes.push(change);
                }
            }

            // Check optional -> required
            if prev_param.optional && !curr_param.optional && curr_param.default_value.is_none() {
                changes.push(BreakingChange::new(
                    BreakingChangeType::ParameterBecameRequired,
                    path,
                    fn_name,
                    format!(
                        "Parameter '{}' in '{}' changed from optional to required",
                        name, fn_name
                    ),
                ));
            }

            // Check required -> optional (usually safe)
            if !prev_param.optional && curr_param.optional {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::ParameterBecameOptional,
                        path,
                        fn_name,
                        format!(
                            "Parameter '{}' in '{}' changed from required to optional",
                            name, fn_name
                        ),
                    )
                    .with_severity(Severity::Warning),
                );
            }
        }
    }

    // Check parameter order (important for positional arguments)
    let prev_order: Vec<&str> = prev.iter().map(|p| p.name.as_str()).collect();
    let curr_order: Vec<&str> = curr.iter().map(|p| p.name.as_str()).collect();

    // Find common parameters and check their order
    let common_prev: Vec<&str> = prev_order
        .iter()
        .filter(|n| curr_params.contains_key(*n))
        .copied()
        .collect();
    let common_curr: Vec<&str> = curr_order
        .iter()
        .filter(|n| prev_params.contains_key(*n))
        .copied()
        .collect();

    if common_prev != common_curr {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::SignatureChanged,
                path,
                fn_name,
                format!("Parameter order changed in '{}'", fn_name),
            )
            .with_previous(common_prev.join(", "))
            .with_current(common_curr.join(", "))
            .with_severity(Severity::Major),
        );
    }
}

/// Compare type parameters.
fn compare_type_parameters(
    prev: &[interlint_format::TypeParameter],
    curr: &[interlint_format::TypeParameter],
    path: &str,
    name: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_params: HashMap<&str, &interlint_format::TypeParameter> =
        prev.iter().map(|p| (p.name.as_str(), p)).collect();
    let curr_params: HashMap<&str, &interlint_format::TypeParameter> =
        curr.iter().map(|p| (p.name.as_str(), p)).collect();

    // Check for removed type parameters
    for (param_name, _) in &prev_params {
        if !curr_params.contains_key(param_name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::TypeParameterRemoved,
                path,
                name,
                format!("Type parameter '{}' was removed from '{}'", param_name, name),
            ));
        }
    }

    // Check for added type parameters
    if include_additions {
        for (param_name, _) in &curr_params {
            if !prev_params.contains_key(param_name) {
                changes.push(BreakingChange::new(
                    BreakingChangeType::TypeParameterAdded,
                    path,
                    name,
                    format!("Type parameter '{}' was added to '{}'", param_name, name),
                ));
            }
        }
    }

    // Compare constraints
    for (param_name, prev_param) in &prev_params {
        if let Some(curr_param) = curr_params.get(param_name) {
            match (&prev_param.constraint, &curr_param.constraint) {
                (Some(prev_constraint), Some(curr_constraint)) => {
                    if !types_equal(prev_constraint, curr_constraint) {
                        changes.push(
                            BreakingChange::new(
                                BreakingChangeType::TypeParameterConstraintChanged,
                                path,
                                name,
                                format!(
                                    "Constraint for type parameter '{}' in '{}' changed",
                                    param_name, name
                                ),
                            )
                            .with_previous(&prev_constraint.repr)
                            .with_current(&curr_constraint.repr),
                        );
                    }
                }
                (Some(prev_constraint), None) => {
                    changes.push(
                        BreakingChange::new(
                            BreakingChangeType::TypeParameterConstraintChanged,
                            path,
                            name,
                            format!(
                                "Constraint removed from type parameter '{}' in '{}'",
                                param_name, name
                            ),
                        )
                        .with_previous(&prev_constraint.repr)
                        .with_severity(Severity::Warning),
                    );
                }
                (None, Some(curr_constraint)) => {
                    changes.push(
                        BreakingChange::new(
                            BreakingChangeType::TypeParameterConstraintChanged,
                            path,
                            name,
                            format!(
                                "Constraint added to type parameter '{}' in '{}'",
                                param_name, name
                            ),
                        )
                        .with_current(&curr_constraint.repr),
                    );
                }
                (None, None) => {}
            }
        }
    }
}

/// Compare class signatures.
fn compare_class_signatures(
    prev: &ClassSig,
    curr: &ClassSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    // Compare properties
    compare_properties(
        &prev.properties,
        &curr.properties,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare methods
    compare_methods(
        &prev.methods,
        &curr.methods,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare type parameters
    compare_type_parameters(
        &prev.type_parameters,
        &curr.type_parameters,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Check extends
    match (&prev.extends, &curr.extends) {
        (Some(prev_ext), Some(curr_ext)) => {
            if !types_equal(prev_ext, curr_ext) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::TypeChanged,
                        path,
                        &prev.name,
                        format!("Base class of '{}' changed", prev.name),
                    )
                    .with_previous(&prev_ext.repr)
                    .with_current(&curr_ext.repr),
                );
            }
        }
        (Some(prev_ext), None) => {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::TypeChanged,
                    path,
                    &prev.name,
                    format!("Base class '{}' removed from '{}'", prev_ext.repr, prev.name),
                )
                .with_previous(&prev_ext.repr),
            );
        }
        (None, Some(curr_ext)) => {
            if include_additions {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::TypeChanged,
                        path,
                        &prev.name,
                        format!("Base class '{}' added to '{}'", curr_ext.repr, prev.name),
                    )
                    .with_current(&curr_ext.repr)
                    .with_severity(Severity::Warning),
                );
            }
        }
        (None, None) => {}
    }

    // Check abstract modifier
    if prev.is_abstract != curr.is_abstract {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Abstract modifier of '{}' changed from {} to {}",
                    prev.name, prev.is_abstract, curr.is_abstract
                ),
            )
            .with_previous(format!("abstract: {}", prev.is_abstract))
            .with_current(format!("abstract: {}", curr.is_abstract)),
        );
    }
}

/// Compare interface signatures.
fn compare_interface_signatures(
    prev: &InterfaceSig,
    curr: &InterfaceSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    // Compare properties
    compare_properties(
        &prev.properties,
        &curr.properties,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare methods
    compare_methods(
        &prev.methods,
        &curr.methods,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare type parameters
    compare_type_parameters(
        &prev.type_parameters,
        &curr.type_parameters,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare extends
    let prev_extends: HashSet<&str> = prev.extends.iter().map(|t| t.repr.as_str()).collect();
    let curr_extends: HashSet<&str> = curr.extends.iter().map(|t| t.repr.as_str()).collect();

    for removed in prev_extends.difference(&curr_extends) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!("Interface '{}' no longer extends '{}'", prev.name, removed),
            )
            .with_previous(*removed),
        );
    }

    if include_additions {
        for added in curr_extends.difference(&prev_extends) {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::TypeChanged,
                    path,
                    &prev.name,
                    format!("Interface '{}' now extends '{}'", prev.name, added),
                )
                .with_current(*added)
                .with_severity(Severity::Warning),
            );
        }
    }
}

/// Compare properties between two signatures.
fn compare_properties(
    prev: &[Property],
    curr: &[Property],
    path: &str,
    parent_name: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_props: HashMap<&str, &Property> =
        prev.iter().map(|p| (p.name.as_str(), p)).collect();
    let curr_props: HashMap<&str, &Property> =
        curr.iter().map(|p| (p.name.as_str(), p)).collect();

    // Check for removed properties
    for (name, _) in &prev_props {
        if !curr_props.contains_key(name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::PropertyRemoved,
                path,
                parent_name,
                format!("Property '{}' was removed from '{}'", name, parent_name),
            ));
        }
    }

    // Check for added properties
    if include_additions {
        for (name, prop) in &curr_props {
            if !prev_props.contains_key(name) {
                let severity = if prop.optional {
                    Severity::Warning
                } else {
                    Severity::Major
                };
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::PropertyAdded,
                        path,
                        parent_name,
                        format!(
                            "Property '{}' was added to '{}' ({})",
                            name,
                            parent_name,
                            if prop.optional { "optional" } else { "required" }
                        ),
                    )
                    .with_severity(severity),
                );
            }
        }
    }

    // Compare properties that exist in both
    for (name, prev_prop) in &prev_props {
        if let Some(curr_prop) = curr_props.get(name) {
            // Determine variance for property type comparison
            // Readonly properties are covariant, mutable properties are invariant
            let prop_variance = if prev_prop.readonly && curr_prop.readonly {
                Variance::Covariant
            } else {
                Variance::Invariant
            };

            // Check type change with structural comparison
            let type_comparison = compare_types_structural(
                &prev_prop.type_info,
                &curr_prop.type_info,
                prop_variance,
                &format!("{}.{}", path, name),
            );

            if !type_comparison.compatible {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::PropertyTypeChanged,
                        path,
                        parent_name,
                        format!(
                            "Type of property '{}' in '{}' changed from '{}' to '{}'",
                            name, parent_name, prev_prop.type_info.repr, curr_prop.type_info.repr
                        ),
                    )
                    .with_previous(&prev_prop.type_info.repr)
                    .with_current(&curr_prop.type_info.repr),
                );
            } else {
                // Add structural changes as separate entries
                for diff in &type_comparison.differences {
                    let change_type = match diff.kind {
                        TypeDifferenceKind::UnionMemberRemoved => BreakingChangeType::UnionMemberRemoved,
                        TypeDifferenceKind::UnionMemberAdded => BreakingChangeType::UnionMemberAdded,
                        TypeDifferenceKind::BecameNullable => BreakingChangeType::TypeBecameNullable,
                        TypeDifferenceKind::BecameNonNullable => BreakingChangeType::TypeBecameNonNullable,
                        _ => continue,
                    };

                    let severity = match diff.kind {
                        TypeDifferenceKind::BecameNonNullable => Severity::Major,
                        TypeDifferenceKind::UnionMemberRemoved if prop_variance == Variance::Covariant => {
                            Severity::Major
                        }
                        _ => Severity::Warning,
                    };

                    let mut change = BreakingChange::new(
                        change_type,
                        path,
                        parent_name,
                        diff.description.clone(),
                    )
                    .with_severity(severity);

                    if let Some(ref prev_val) = diff.previous {
                        change = change.with_previous(prev_val);
                    }
                    if let Some(ref curr_val) = diff.current {
                        change = change.with_current(curr_val);
                    }
                    changes.push(change);
                }
            }

            // Check optional -> required
            if prev_prop.optional && !curr_prop.optional {
                changes.push(BreakingChange::new(
                    BreakingChangeType::PropertyBecameRequired,
                    path,
                    parent_name,
                    format!(
                        "Property '{}' in '{}' changed from optional to required",
                        name, parent_name
                    ),
                ));
            }

            // Check required -> optional
            if !prev_prop.optional && curr_prop.optional {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::PropertyBecameOptional,
                        path,
                        parent_name,
                        format!(
                            "Property '{}' in '{}' changed from required to optional",
                            name, parent_name
                        ),
                    )
                    .with_severity(Severity::Warning),
                );
            }

            // Check visibility change
            if prev_prop.visibility != curr_prop.visibility {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::VisibilityChanged,
                        path,
                        parent_name,
                        format!(
                            "Visibility of property '{}' in '{}' changed from {:?} to {:?}",
                            name, parent_name, prev_prop.visibility, curr_prop.visibility
                        ),
                    )
                    .with_previous(format!("{:?}", prev_prop.visibility))
                    .with_current(format!("{:?}", curr_prop.visibility)),
                );
            }

            // Check readonly change
            if prev_prop.readonly != curr_prop.readonly {
                let severity = if !prev_prop.readonly && curr_prop.readonly {
                    // Mutable -> readonly is safe (consumers can still read)
                    Severity::Warning
                } else {
                    // Readonly -> mutable changes the variance, potentially breaking
                    Severity::Minor
                };
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::TypeChanged,
                        path,
                        parent_name,
                        format!(
                            "Property '{}' in '{}' changed from {} to {}",
                            name,
                            parent_name,
                            if prev_prop.readonly { "readonly" } else { "mutable" },
                            if curr_prop.readonly { "readonly" } else { "mutable" }
                        ),
                    )
                    .with_severity(severity)
                    .with_previous(if prev_prop.readonly { "readonly" } else { "mutable" })
                    .with_current(if curr_prop.readonly { "readonly" } else { "mutable" }),
                );
            }
        }
    }
}

/// Compare methods between two signatures.
fn compare_methods(
    prev: &[FunctionSig],
    curr: &[FunctionSig],
    path: &str,
    parent_name: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_methods: HashMap<&str, &FunctionSig> =
        prev.iter().map(|m| (m.name.as_str(), m)).collect();
    let curr_methods: HashMap<&str, &FunctionSig> =
        curr.iter().map(|m| (m.name.as_str(), m)).collect();

    // Check for removed methods
    for (name, _) in &prev_methods {
        if !curr_methods.contains_key(name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::MethodRemoved,
                path,
                parent_name,
                format!("Method '{}' was removed from '{}'", name, parent_name),
            ));
        }
    }

    // Check for added methods
    if include_additions {
        for (name, _) in &curr_methods {
            if !prev_methods.contains_key(name) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::MethodAdded,
                        path,
                        parent_name,
                        format!("Method '{}' was added to '{}'", name, parent_name),
                    )
                    .with_severity(Severity::Warning),
                );
            }
        }
    }

    // Compare methods that exist in both
    for (name, prev_method) in &prev_methods {
        if let Some(curr_method) = curr_methods.get(name) {
            let method_path = format!("{}.{}", path, name);
            compare_function_signatures(
                prev_method,
                curr_method,
                &method_path,
                changes,
                include_additions,
            );
        }
    }
}

/// Compare enum signatures.
fn compare_enum_signatures(
    prev: &EnumSig,
    curr: &EnumSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_members: HashMap<&str, Option<&str>> = prev
        .members
        .iter()
        .map(|m| (m.name.as_str(), m.value.as_deref()))
        .collect();
    let curr_members: HashMap<&str, Option<&str>> = curr
        .members
        .iter()
        .map(|m| (m.name.as_str(), m.value.as_deref()))
        .collect();

    // Check for removed members
    for (name, _) in &prev_members {
        if !curr_members.contains_key(name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::EnumMemberRemoved,
                path,
                &prev.name,
                format!("Enum member '{}' was removed from '{}'", name, prev.name),
            ));
        }
    }

    // Check for added members
    if include_additions {
        for (name, _) in &curr_members {
            if !prev_members.contains_key(name) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::EnumMemberAdded,
                        path,
                        &prev.name,
                        format!("Enum member '{}' was added to '{}'", name, prev.name),
                    )
                    .with_severity(Severity::Warning),
                );
            }
        }
    }

    // Check for value changes
    for (name, prev_value) in &prev_members {
        if let Some(curr_value) = curr_members.get(name) {
            if prev_value != curr_value {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::EnumMemberValueChanged,
                        path,
                        &prev.name,
                        format!(
                            "Value of enum member '{}' in '{}' changed",
                            name, prev.name
                        ),
                    )
                    .with_previous(prev_value.unwrap_or("(auto)").to_string())
                    .with_current(curr_value.unwrap_or("(auto)").to_string()),
                );
            }
        }
    }

    // Check const modifier
    if prev.is_const != curr.is_const {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Const modifier of enum '{}' changed from {} to {}",
                    prev.name, prev.is_const, curr.is_const
                ),
            )
            .with_previous(format!("const: {}", prev.is_const))
            .with_current(format!("const: {}", curr.is_const)),
        );
    }
}

/// Compare type alias signatures.
fn compare_type_alias_signatures(
    prev: &TypeAliasSig,
    curr: &TypeAliasSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
) {
    // Type aliases can be used in both input and output positions,
    // so we treat them as invariant for breaking change detection
    let comparison = compare_types_structural(
        &prev.aliased_type,
        &curr.aliased_type,
        Variance::Invariant,
        &format!("{}.type", path),
    );

    if !comparison.compatible {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Type alias '{}' changed from '{}' to '{}'",
                    prev.name, prev.aliased_type.repr, curr.aliased_type.repr
                ),
            )
            .with_previous(&prev.aliased_type.repr)
            .with_current(&curr.aliased_type.repr),
        );
    }

    // Add detailed structural changes
    for diff in comparison.differences {
        let change_type = match diff.kind {
            TypeDifferenceKind::UnionMemberRemoved => BreakingChangeType::UnionMemberRemoved,
            TypeDifferenceKind::UnionMemberAdded => BreakingChangeType::UnionMemberAdded,
            TypeDifferenceKind::IntersectionMemberAdded => BreakingChangeType::IntersectionMemberAdded,
            TypeDifferenceKind::IntersectionMemberRemoved => BreakingChangeType::IntersectionMemberRemoved,
            TypeDifferenceKind::BecameNullable => BreakingChangeType::TypeBecameNullable,
            TypeDifferenceKind::BecameNonNullable => BreakingChangeType::TypeBecameNonNullable,
            TypeDifferenceKind::ObjectPropertyAdded => BreakingChangeType::PropertyAdded,
            TypeDifferenceKind::ObjectPropertyRemoved => BreakingChangeType::PropertyRemoved,
            TypeDifferenceKind::ObjectPropertyTypeChanged => BreakingChangeType::PropertyTypeChanged,
            TypeDifferenceKind::ObjectPropertyBecameRequired => BreakingChangeType::PropertyBecameRequired,
            TypeDifferenceKind::ObjectPropertyBecameOptional => BreakingChangeType::PropertyBecameOptional,
            _ => continue, // Skip minor differences
        };

        let mut change = BreakingChange::new(
            change_type,
            path,
            &prev.name,
            diff.description,
        );

        if let Some(prev_val) = diff.previous {
            change = change.with_previous(prev_val);
        }
        if let Some(curr_val) = diff.current {
            change = change.with_current(curr_val);
        }

        changes.push(change);
    }
}

/// Compare variable signatures.
fn compare_variable_signatures(
    prev: &VariableSig,
    curr: &VariableSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
) {
    if !types_equal(&prev.type_info, &curr.type_info) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Type of variable '{}' changed from '{}' to '{}'",
                    prev.name, prev.type_info.repr, curr.type_info.repr
                ),
            )
            .with_previous(&prev.type_info.repr)
            .with_current(&curr.type_info.repr),
        );
    }

    // Check const modifier change
    if prev.is_const != curr.is_const {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Const modifier of '{}' changed from {} to {}",
                    prev.name, prev.is_const, curr.is_const
                ),
            )
            .with_previous(format!("const: {}", prev.is_const))
            .with_current(format!("const: {}", curr.is_const))
            .with_severity(Severity::Minor),
        );
    }
}

/// Compare namespace exports recursively.
fn compare_namespace_exports(
    prev: &[Signature],
    curr: &[Signature],
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_exports: HashMap<&str, &Signature> =
        prev.iter().map(|s| (s.name(), s)).collect();
    let curr_exports: HashMap<&str, &Signature> =
        curr.iter().map(|s| (s.name(), s)).collect();

    // Check for removed exports
    for (name, _) in &prev_exports {
        if !curr_exports.contains_key(name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::ExportRemoved,
                path,
                *name,
                format!("Export '{}' was removed from namespace", name),
            ));
        }
    }

    // Check for added exports
    if include_additions {
        for (name, _) in &curr_exports {
            if !prev_exports.contains_key(name) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::ExportAdded,
                        path,
                        *name,
                        format!("Export '{}' was added to namespace", name),
                    )
                    .with_severity(Severity::Warning),
                );
            }
        }
    }

    // Compare exports that exist in both
    for (name, prev_sig) in &prev_exports {
        if let Some(curr_sig) = curr_exports.get(name) {
            // Create synthetic exports for comparison
            let prev_export = Export {
                name: name.to_string(),
                original_name: None,
                signature: (*prev_sig).clone(),
                is_default: false,
            };
            let curr_export = Export {
                name: name.to_string(),
                original_name: None,
                signature: (*curr_sig).clone(),
                is_default: false,
            };
            compare_export_signatures(&prev_export, &curr_export, path, changes, include_additions);
        }
    }
}

/// Compare assets between two package interfaces.
fn compare_assets(
    previous: &PublicInterface,
    current: &PublicInterface,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_assets: HashSet<&str> = previous.assets.iter().map(|a| a.path.as_str()).collect();
    let curr_assets: HashSet<&str> = current.assets.iter().map(|a| a.path.as_str()).collect();

    // Removed assets (breaking change)
    for path in prev_assets.difference(&curr_assets) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::AssetRemoved,
                format!("asset:{}", path),
                path.to_string(),
                format!("Asset '{}' was removed from the package", path),
            )
            .with_suggestion("Ensure consumers are not referencing this asset".to_string()),
        );
    }

    // Added assets (informational)
    if include_additions {
        for path in curr_assets.difference(&prev_assets) {
            changes.push(BreakingChange::new(
                BreakingChangeType::AssetAdded,
                format!("asset:{}", path),
                path.to_string(),
                format!("Asset '{}' was added to the package", path),
            ));
        }
    }
}

/// Check if two types are equal (simple string-based comparison).
///
/// For deep structural comparison, use `compare_types_structural` instead.
fn types_equal(a: &TypeInfo, b: &TypeInfo) -> bool {
    // Fast path: check kind and repr
    if a.kind != b.kind || a.repr != b.repr {
        return false;
    }

    // Check nullability
    if a.nullable != b.nullable {
        return false;
    }

    // For union/intersection types, check constituents
    if matches!(a.kind, TypeKind::Union | TypeKind::Intersection) {
        if a.constituents.len() != b.constituents.len() {
            return false;
        }
        let a_members: HashSet<&str> = a.constituents.iter().map(|t| t.repr.as_str()).collect();
        let b_members: HashSet<&str> = b.constituents.iter().map(|t| t.repr.as_str()).collect();
        if a_members != b_members {
            return false;
        }
    }

    // For array types, check element type
    if a.kind == TypeKind::Array {
        match (&a.element_type, &b.element_type) {
            (Some(a_elem), Some(b_elem)) => {
                if !types_equal(a_elem, b_elem) {
                    return false;
                }
            }
            (None, None) => {}
            _ => return false,
        }
    }

    // For object types, check properties
    if a.kind == TypeKind::Object && !a.properties.is_empty() {
        if a.properties.len() != b.properties.len() {
            return false;
        }
        for (ap, bp) in a.properties.iter().zip(b.properties.iter()) {
            if ap.name != bp.name
                || ap.optional != bp.optional
                || ap.readonly != bp.readonly
                || !types_equal(&ap.type_info, &bp.type_info)
            {
                return false;
            }
        }
    }

    true
}

/// Check if two types are compatible considering variance.
///
/// Returns true if replacing `prev` with `curr` is safe in the given variance position.
#[allow(dead_code)]
fn types_compatible(prev: &TypeInfo, curr: &TypeInfo, variance: Variance) -> bool {
    let result = compare_types_structural(prev, curr, variance, "");
    result.compatible
}

/// Get a human-readable name for a signature kind.
fn signature_kind_name(sig: &Signature) -> String {
    match sig {
        Signature::Function(_) => "function".to_string(),
        Signature::Class(_) => "class".to_string(),
        Signature::Variable(_) => "variable".to_string(),
        Signature::Interface(_) => "interface".to_string(),
        Signature::Enum(_) => "enum".to_string(),
        Signature::TypeAlias(_) => "type alias".to_string(),
        Signature::Namespace(_) => "namespace".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use interlint_format::{Metadata, TypeInfo, Visibility};

    fn create_test_metadata() -> Metadata {
        Metadata {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: Some("typescript".to_string()),
        }
    }

    #[test]
    fn test_export_removed() {
        let mut prev = PublicInterface::new(create_test_metadata());
        let mut prev_module = Module::new("index.ts");
        prev_module.add_export(Export {
            name: "foo".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "foo".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::void(),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });
        prev.add_module(prev_module);

        let mut curr = PublicInterface::new(create_test_metadata());
        curr.add_module(Module::new("index.ts"));

        let changes = compare_interfaces(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, BreakingChangeType::ExportRemoved);
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_parameter_added() {
        let mut prev = PublicInterface::new(create_test_metadata());
        let mut prev_module = Module::new("index.ts");
        prev_module.add_export(Export {
            name: "greet".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "greet".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::primitive("string"),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });
        prev.add_module(prev_module);

        let mut curr = PublicInterface::new(create_test_metadata());
        let mut curr_module = Module::new("index.ts");
        curr_module.add_export(Export {
            name: "greet".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "greet".to_string(),
                type_parameters: vec![],
                parameters: vec![Parameter {
                    name: "name".to_string(),
                    type_info: TypeInfo::primitive("string"),
                    optional: false,
                    rest: false,
                    default_value: None,
                }],
                return_type: TypeInfo::primitive("string"),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });
        curr.add_module(curr_module);

        let changes = compare_interfaces(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, BreakingChangeType::ParameterAdded);
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_no_changes() {
        let prev = PublicInterface::new(create_test_metadata());
        let curr = PublicInterface::new(create_test_metadata());

        let changes = compare_interfaces(&prev, &curr, false);
        assert!(changes.is_empty());
    }

    // =========================================================================
    // Structural Type Comparison Tests
    // =========================================================================

    #[test]
    fn test_structural_comparison_identical() {
        let t = TypeInfo::primitive("string");
        let result = compare_types_structural(&t, &t, Variance::Covariant, "test");
        assert!(result.compatible);
        assert!(result.differences.is_empty());
    }

    #[test]
    fn test_structural_comparison_primitive_change() {
        let prev = TypeInfo::primitive("string");
        let curr = TypeInfo::primitive("number");

        let result = compare_types_structural(&prev, &curr, Variance::Covariant, "test");
        assert!(!result.compatible);
        assert!(!result.differences.is_empty());
    }

    #[test]
    fn test_structural_comparison_nullable_change() {
        let mut prev = TypeInfo::primitive("string");
        let mut curr = TypeInfo::primitive("string");
        curr.nullable = true;

        // Becoming nullable is safe in covariant position
        let result = compare_types_structural(&prev, &curr, Variance::Covariant, "test");
        assert!(result.compatible);
        assert!(!result.differences.is_empty());
        assert!(result
            .differences
            .iter()
            .any(|d| d.kind == TypeDifferenceKind::BecameNullable));

        // Becoming nullable is breaking in contravariant position
        let result_contra = compare_types_structural(&prev, &curr, Variance::Contravariant, "test");
        assert!(!result_contra.compatible);

        // Becoming non-nullable is breaking in covariant position
        prev.nullable = true;
        curr.nullable = false;
        let result_non_null = compare_types_structural(&prev, &curr, Variance::Covariant, "test");
        assert!(!result_non_null.compatible);
    }

    #[test]
    fn test_structural_comparison_union_members() {
        let prev = TypeInfo::union(vec![
            TypeInfo::primitive("string"),
            TypeInfo::primitive("number"),
        ]);
        let curr = TypeInfo::union(vec![
            TypeInfo::primitive("string"),
            TypeInfo::primitive("number"),
            TypeInfo::primitive("boolean"),
        ]);

        // Adding union member widens the type - safe in covariant
        let result = compare_types_structural(&prev, &curr, Variance::Covariant, "test");
        assert!(result.compatible);
        assert!(result
            .differences
            .iter()
            .any(|d| d.kind == TypeDifferenceKind::UnionMemberAdded));

        // Adding union member is breaking in contravariant (params)
        let result_contra = compare_types_structural(&prev, &curr, Variance::Contravariant, "test");
        assert!(!result_contra.compatible);

        // Removing union member narrows the type - breaking in covariant
        let result_removed = compare_types_structural(&curr, &prev, Variance::Covariant, "test");
        assert!(!result_removed.compatible);
        assert!(result_removed
            .differences
            .iter()
            .any(|d| d.kind == TypeDifferenceKind::UnionMemberRemoved));
    }

    #[test]
    fn test_structural_comparison_array() {
        let prev = TypeInfo::array(TypeInfo::primitive("string"));
        let curr = TypeInfo::array(TypeInfo::primitive("number"));

        let result = compare_types_structural(&prev, &curr, Variance::Covariant, "test");
        assert!(!result.compatible);
    }

    #[test]
    fn test_variance_covariant() {
        let variance = Variance::Covariant;
        assert!(variance.is_safe_to_widen());
        assert!(!variance.is_safe_to_narrow());
    }

    #[test]
    fn test_variance_contravariant() {
        let variance = Variance::Contravariant;
        assert!(!variance.is_safe_to_widen());
        assert!(variance.is_safe_to_narrow());
    }

    #[test]
    fn test_variance_invariant() {
        let variance = Variance::Invariant;
        assert!(!variance.is_safe_to_widen());
        assert!(!variance.is_safe_to_narrow());
    }

    #[test]
    fn test_variance_bivariant() {
        let variance = Variance::Bivariant;
        assert!(variance.is_safe_to_widen());
        assert!(variance.is_safe_to_narrow());
    }

    #[test]
    fn test_variance_combine() {
        // Covariant * Covariant = Covariant
        assert_eq!(
            Variance::Covariant.combine(Variance::Covariant),
            Variance::Covariant
        );

        // Covariant * Contravariant = Contravariant
        assert_eq!(
            Variance::Covariant.combine(Variance::Contravariant),
            Variance::Contravariant
        );

        // Contravariant * Contravariant = Covariant
        assert_eq!(
            Variance::Contravariant.combine(Variance::Contravariant),
            Variance::Covariant
        );

        // Anything with Invariant = Invariant
        assert_eq!(
            Variance::Covariant.combine(Variance::Invariant),
            Variance::Invariant
        );
    }

    #[test]
    fn test_primitive_subtype_relationships() {
        assert!(is_primitive_subtype("integer", "number"));
        assert!(is_primitive_subtype("float", "number"));
        assert!(is_primitive_subtype("true", "boolean"));
        assert!(is_primitive_subtype("false", "boolean"));
        assert!(!is_primitive_subtype("number", "integer"));
        assert!(!is_primitive_subtype("string", "number"));
    }
}
