// Structural subtype checker for TypeShape.
//
// Implements TypeScript variance semantics:
// - Covariant positions: return types, array elements
// - Contravariant positions: function parameters
// - Invariant positions: generic type arguments (conservative for safety)
//
// Design decisions:
// - D-09: full structural subtype checking replacing equality-based comparison
// - D-10: Opaque/Conditional/Mapped/Indexed/Template are incomparable — return false
// - D-04: format_type produces human-readable strings for finding messages
// - MAX_DEPTH = 20 consistent with type_diff.rs

use crate::types::shape::{LiteralKind, ParamShape, PrimitiveKind, TypeShape};

const MAX_DEPTH: usize = 20;

// ─── Rule ID constants ───────────────────────────────────────────────────────

// Signature rules (SIG-*)
pub const RULE_PARAM_ADDED: &str = "param-added";
pub const RULE_PARAM_REMOVED: &str = "param-removed";
pub const RULE_PARAM_TYPE_NARROWED: &str = "param-type-narrowed";
pub const RULE_RETURN_TYPE_WIDENED: &str = "return-type-widened";
pub const RULE_PARAM_REORDERED: &str = "param-reordered";

// Type/interface field rules (TYPE-*)
pub const RULE_FIELD_REMOVED: &str = "field-removed";
pub const RULE_FIELD_ADDED: &str = "field-added";
pub const RULE_FIELD_TYPE_CHANGED: &str = "field-type-changed";

// Catch-all for truly incomparable type changes
pub const RULE_TYPE_INCOMPATIBLE: &str = "type-incompatible";

// Metadata rules (used by metadata.rs in Plan 03)
pub const RULE_PEER_DEP_BUMPED: &str = "peer-dep-version-bumped";
pub const RULE_PEER_DEP_ADDED: &str = "peer-dep-added";
pub const RULE_PEER_DEP_REMOVED: &str = "peer-dep-removed";
pub const RULE_ENGINE_BUMPED: &str = "engine-version-bumped";
pub const RULE_ENGINE_ADDED: &str = "engine-added";
pub const RULE_ENTRY_POINT_CHANGED: &str = "entry-point-changed";
pub const RULE_MODULE_TYPE_CHANGED: &str = "module-type-changed";

// Exports diff rules (EXP-*)
pub const RULE_SUBPATH_REMOVED: &str = "subpath-removed";
pub const RULE_EXPORTS_FIELD_ADDED: &str = "exports-field-added";
pub const RULE_CONDITION_REMOVED: &str = "condition-removed";
pub const RULE_CONDITION_TARGET_CHANGED: &str = "condition-target-changed";
pub const RULE_TYPES_CONDITION_REMOVED: &str = "types-condition-removed";
pub const RULE_SUBPATH_PATTERN_CHANGED: &str = "subpath-pattern-changed";
pub const RULE_SUBPATH_ADDED: &str = "subpath-added";
pub const RULE_CJS_TO_ESM: &str = "cjs-to-esm";
pub const RULE_ESM_TO_CJS: &str = "esm-to-cjs";
pub const RULE_TYPES_RUNTIME_MISMATCH: &str = "types-runtime-mismatch";
pub const RULE_BIN_ENTRY_CHANGED: &str = "bin-entry-changed";
pub const RULE_PLATFORM_NARROWED: &str = "platform-narrowed";
pub const RULE_IMPORTABLE_FILE_REMOVED: &str = "importable-file-removed";

// ─── Subtype checker ─────────────────────────────────────────────────────────

/// Determine whether `sub` is a structural subtype of `sup` under TypeScript
/// variance semantics.
///
/// Returns `false` for incomparable types (Opaque, Conditional, Mapped,
/// Indexed, Template) per D-10 (assume-breaking conservatism).
///
/// Recursion is capped at `MAX_DEPTH = 20` to match type_diff.rs.
pub fn is_subtype_of(sub: &TypeShape, sup: &TypeShape, depth: usize) -> bool {
    use PrimitiveKind::*;
    use TypeShape::*;

    // Depth cap: assume not a subtype (D-10: conservative)
    if depth >= MAX_DEPTH {
        return false;
    }

    // any is the top type AND the bottom type in TypeScript (escape hatch)
    if let Primitive(Any) = sup {
        return true;
    }
    if let Primitive(Any) = sub {
        return true;
    }

    // unknown is the proper top type
    if let Primitive(Unknown) = sup {
        return true;
    }

    // never is the bottom type — never <: anything
    if let Primitive(Never) = sub {
        return true;
    }

    // Reflexive: a <: a (fast path before normalization)
    if sub == sup {
        return true;
    }

    // Literal subtype of corresponding primitive
    match (sub, sup) {
        (Literal(LiteralKind::String(_)), Primitive(String)) => return true,
        (Literal(LiteralKind::Number(_)), Primitive(Number)) => return true,
        (Literal(LiteralKind::Boolean(_)), Primitive(Boolean)) => return true,
        (Literal(LiteralKind::Null), Primitive(Null)) => return true,
        _ => {}
    }

    // D-10: incomparable types — conservatively assume not a subtype
    match (sub, sup) {
        (Opaque(_), _) | (_, Opaque(_)) => return false,
        (Conditional { .. }, _) | (_, Conditional { .. }) => return false,
        (Mapped { .. }, _) | (_, Mapped { .. }) => return false,
        (Indexed { .. }, _) | (_, Indexed { .. }) => return false,
        (Template(_), _) | (_, Template(_)) => return false,
        _ => {}
    }

    match (sub, sup) {
        // Object structural subtyping:
        // sub must satisfy every field in sup
        // (extra fields in sub are allowed — width subtyping)
        (Object(sub_fields), Object(sup_fields)) => {
            for (field_name, sup_field) in sup_fields {
                match sub_fields.get(field_name) {
                    None => {
                        // Field missing in sub: only ok if optional in sup
                        if !sup_field.optional {
                            return false;
                        }
                    }
                    Some(sub_field) => {
                        // Optionality: if sub marks it required but sup says optional, fine.
                        // If sub marks it optional but sup requires it, that's a problem.
                        if !sup_field.optional && sub_field.optional {
                            return false;
                        }
                        // Shapes must be compatible (covariant field types)
                        if !is_subtype_of(&sub_field.shape, &sup_field.shape, depth + 1) {
                            return false;
                        }
                    }
                }
            }
            true
        }

        // Union sub: every member of sub must be a subtype of sup
        // e.g. (string | number) <: (string | number | boolean) iff
        //   string <: (string | number | boolean) AND number <: (string | number | boolean)
        (Union(members), _) => members.iter().all(|m| is_subtype_of(m, sup, depth + 1)),

        // Union sup: sub must be a subtype of at least one member
        // e.g. string <: (string | number) iff string <: string OR string <: number
        (_, Union(members)) => members.iter().any(|m| is_subtype_of(sub, m, depth + 1)),

        // Intersection sub: sub is a subtype of sup if any member of sub is a subtype of sup
        // (Intersection is narrower than any of its members, so it subtypes if any does)
        (Intersection(members), _) => members.iter().any(|m| is_subtype_of(m, sup, depth + 1)),

        // Intersection sup: sub must satisfy ALL members of the intersection
        (_, Intersection(members)) => members.iter().all(|m| is_subtype_of(sub, m, depth + 1)),

        // Array: covariant in element type
        (Array(sub_inner), Array(sup_inner)) => is_subtype_of(sub_inner, sup_inner, depth + 1),

        // Tuple: same length and each position covariant
        (Tuple(sub_elems), Tuple(sup_elems)) => {
            sub_elems.len() == sup_elems.len()
                && sub_elems
                    .iter()
                    .zip(sup_elems.iter())
                    .all(|(s, p)| is_subtype_of(s, p, depth + 1))
        }

        // Function: contravariant params, covariant return
        // TypeScript callback compatibility: a function with fewer params is a subtype.
        // (You can always pass a callback that ignores extra arguments.)
        (
            Function {
                params: sub_params,
                return_type: sub_ret,
            },
            Function {
                params: sup_params,
                return_type: sup_ret,
            },
        ) => {
            // sub may have fewer params than sup (TS callback pattern)
            // sub may NOT have more required params than sup has total
            let sub_required = sub_params.iter().filter(|p| !p.optional).count();
            if sub_required > sup_params.len() {
                return false;
            }

            // For each position that both share, params are CONTRAVARIANT:
            // sup's param type must be a subtype of sub's param type
            // (the caller passes sup's type; sub must accept anything sup accepts)
            let shared = sub_params.len().min(sup_params.len());
            for i in 0..shared {
                // contravariant: sup param must accept what sub param accepts
                if !is_subtype_of(&sup_params[i].shape, &sub_params[i].shape, depth + 1) {
                    return false;
                }
            }

            // Return type is COVARIANT: sub's return must be a subtype of sup's return
            is_subtype_of(sub_ret, sup_ret, depth + 1)
        }

        // Generic: treat type arguments as invariant (conservative)
        (
            Generic {
                name: sub_name,
                args: sub_args,
            },
            Generic {
                name: sup_name,
                args: sup_args,
            },
        ) => {
            sub_name == sup_name
                && sub_args.len() == sup_args.len()
                && sub_args
                    .iter()
                    .zip(sup_args.iter())
                    .all(|(s, p)| is_subtype_of(s, p, depth + 1))
        }

        // TypeParam: name equality only (no constraint information available)
        (TypeParam(sub_name), TypeParam(sup_name)) => sub_name == sup_name,

        // Catch-all: different variants not handled above — assume not a subtype (D-10)
        _ => false,
    }
}

// ─── Type display helper ──────────────────────────────────────────────────────

/// Produce a human-readable string representation of a TypeShape.
///
/// Used by D-04: finding messages include old/new type values.
/// Example: `format_type(&Union([String, Number]))` returns `"string | number"`.
pub fn format_type(shape: &TypeShape) -> String {
    use PrimitiveKind::*;
    use TypeShape::*;

    match shape {
        Primitive(p) => match p {
            String => "string".to_string(),
            Number => "number".to_string(),
            Boolean => "boolean".to_string(),
            Null => "null".to_string(),
            Undefined => "undefined".to_string(),
            Void => "void".to_string(),
            Never => "never".to_string(),
            Any => "any".to_string(),
            Unknown => "unknown".to_string(),
            Symbol => "symbol".to_string(),
            Bigint => "bigint".to_string(),
        },
        Literal(lit) => match lit {
            LiteralKind::String(s) => format!("\"{}\"", s),
            LiteralKind::Number(n) => n.clone(),
            LiteralKind::Boolean(b) => format!("{}", b),
            LiteralKind::Null => "null".to_string(),
        },
        Union(members) => members
            .iter()
            .map(format_type)
            .collect::<Vec<_>>()
            .join(" | "),
        Intersection(members) => members
            .iter()
            .map(format_type)
            .collect::<Vec<_>>()
            .join(" & "),
        Array(inner) => format!("{}[]", format_type(inner)),
        Tuple(elems) => format!(
            "[{}]",
            elems.iter().map(format_type).collect::<Vec<_>>().join(", ")
        ),
        Object(_) => "{ ... }".to_string(),
        Function {
            params,
            return_type,
        } => {
            let params_str = params
                .iter()
                .map(|p: &ParamShape| format!("{}: {}", p.name, format_type(&p.shape)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}) => {}", params_str, format_type(return_type))
        }
        Generic { name, args } => format!(
            "{}<{}>",
            name,
            args.iter().map(format_type).collect::<Vec<_>>().join(", ")
        ),
        TypeParam(name) => name.clone(),
        // Opaque carries already-human-readable raw text
        Opaque(s) => s.clone(),
        // Fallback for Conditional, Mapped, Indexed, Template
        other => format!("{:?}", other),
    }
}

// Class rules (CLS-*)
pub const RULE_CLASS_CONSTRUCTOR_PARAM_ADDED: &str = "class-constructor-param-added";
pub const RULE_CLASS_METHOD_VISIBILITY_CHANGED: &str = "class-method-visibility-changed";
pub const RULE_CLASS_METHOD_REMOVED: &str = "class-method-removed";
pub const RULE_CLASS_MADE_ABSTRACT: &str = "class-made-abstract";
pub const RULE_CLASS_ABSTRACT_METHOD_ADDED: &str = "class-abstract-method-added";
pub const RULE_CLASS_INHERITANCE_CHANGED: &str = "class-inheritance-changed";
pub const RULE_CLASS_STATIC_TYPE_CHANGED: &str = "class-static-type-changed";
pub const RULE_CLASS_READONLY_CHANGED: &str = "class-readonly-changed";
pub const RULE_CLASS_IMPLEMENTS_REMOVED: &str = "class-implements-removed";

// Enum rules (ENUM-*)
pub const RULE_ENUM_MEMBER_REMOVED: &str = "enum-member-removed";
pub const RULE_ENUM_MEMBER_VALUE_CHANGED: &str = "enum-member-value-changed";
pub const RULE_ENUM_MEMBER_REORDERED: &str = "enum-member-reordered";
pub const RULE_ENUM_CONST_TO_REGULAR: &str = "enum-const-to-regular";
pub const RULE_ENUM_NUMERIC_TO_STRING: &str = "enum-numeric-to-string";

// Generic rules (GEN-*)
pub const RULE_GENERIC_PARAM_ADDED: &str = "generic-param-added";
pub const RULE_GENERIC_PARAM_REMOVED: &str = "generic-param-removed";
pub const RULE_GENERIC_PARAM_REORDERED: &str = "generic-param-reordered";
pub const RULE_GENERIC_CONSTRAINT_TIGHTENED: &str = "generic-constraint-tightened";
pub const RULE_GENERIC_DEFAULT_CHANGED: &str = "generic-default-changed";
pub const RULE_GENERIC_PARAM_ADDED_WITH_DEFAULT: &str = "generic-param-added-with-default";

// ─── Phase 8: Signature & Type Refinement rules ────────────────────────────
pub const RULE_OVERLOAD_REMOVED: &str = "overload-removed";
pub const RULE_REST_PARAM_NARROWED: &str = "rest-param-narrowed";
pub const RULE_TYPE_GUARD_WIDENED: &str = "type-guard-widened";
pub const RULE_DECLARATION_KIND_CHANGED: &str = "declaration-kind-changed";
pub const RULE_NAMESPACE_TO_OTHER: &str = "namespace-to-other";
pub const RULE_PROPERTY_MADE_REQUIRED: &str = "property-made-required";
pub const RULE_INDEX_SIGNATURE_CHANGED: &str = "index-signature-changed";
pub const RULE_DEFAULT_EXPORT_CHANGED: &str = "default-export-changed";

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::shape::{FieldShape, LiteralKind, ParamShape, PrimitiveKind, TypeShape};
    use std::collections::BTreeMap;

    fn p(k: PrimitiveKind) -> TypeShape {
        TypeShape::Primitive(k)
    }

    fn string() -> TypeShape {
        p(PrimitiveKind::String)
    }

    fn number() -> TypeShape {
        p(PrimitiveKind::Number)
    }

    fn boolean() -> TypeShape {
        p(PrimitiveKind::Boolean)
    }

    fn sub(a: &TypeShape, b: &TypeShape) -> bool {
        is_subtype_of(a, b, 0)
    }

    // ── 1. Primitive reflexivity ──────────────────────────────────────────────

    #[test]
    fn test_primitive_reflexive() {
        assert!(sub(&string(), &string()), "string <: string");
        assert!(sub(&number(), &number()), "number <: number");
        assert!(sub(&boolean(), &boolean()), "boolean <: boolean");
    }

    // ── 2. Different primitives ───────────────────────────────────────────────

    #[test]
    fn test_different_primitives_not_subtype() {
        assert!(!sub(&string(), &number()), "string is NOT <: number");
        assert!(!sub(&number(), &boolean()), "number is NOT <: boolean");
    }

    // ── 3. Literal subtype of corresponding primitive ─────────────────────────

    #[test]
    fn test_literal_subtype_of_primitive() {
        let lit_str = TypeShape::Literal(LiteralKind::String("foo".to_string()));
        let lit_num = TypeShape::Literal(LiteralKind::Number("42".to_string()));
        let lit_bool = TypeShape::Literal(LiteralKind::Boolean(true));
        let lit_null = TypeShape::Literal(LiteralKind::Null);

        assert!(sub(&lit_str, &string()), "\"foo\" <: string");
        assert!(sub(&lit_num, &number()), "42 <: number");
        assert!(sub(&lit_bool, &boolean()), "true <: boolean");
        assert!(
            sub(&lit_null, &p(PrimitiveKind::Null)),
            "null literal <: null"
        );

        // Literal is NOT a subtype of a different primitive
        assert!(!sub(&lit_str, &number()), "\"foo\" is NOT <: number");
    }

    // ── 4. any is top AND bottom ──────────────────────────────────────────────

    #[test]
    fn test_any_is_top_and_bottom() {
        let any = p(PrimitiveKind::Any);
        // anything <: any
        assert!(sub(&string(), &any), "string <: any");
        assert!(sub(&number(), &any), "number <: any");
        assert!(
            sub(&TypeShape::Opaque("X".to_string()), &any),
            "Opaque <: any"
        );
        // any <: anything (any is also a bottom-like escape hatch in TS)
        assert!(sub(&any, &string()), "any <: string");
        assert!(sub(&any, &number()), "any <: number");
    }

    // ── 5. unknown is top only ────────────────────────────────────────────────

    #[test]
    fn test_unknown_is_top_only() {
        let unknown = p(PrimitiveKind::Unknown);
        // anything <: unknown
        assert!(sub(&string(), &unknown), "string <: unknown");
        assert!(sub(&number(), &unknown), "number <: unknown");
        // unknown is NOT a subtype of regular types
        assert!(!sub(&unknown, &string()), "unknown is NOT <: string");
        assert!(!sub(&unknown, &number()), "unknown is NOT <: number");
    }

    // ── 6. never is bottom ────────────────────────────────────────────────────

    #[test]
    fn test_never_is_bottom() {
        let never = p(PrimitiveKind::Never);
        assert!(sub(&never, &string()), "never <: string");
        assert!(sub(&never, &number()), "never <: number");
        assert!(sub(&never, &p(PrimitiveKind::Unknown)), "never <: unknown");
        // string is NOT <: never
        assert!(!sub(&string(), &never), "string is NOT <: never");
    }

    // ── 7. Opaque assume-breaking (D-10) ──────────────────────────────────────

    #[test]
    fn test_opaque_assume_breaking() {
        let opaque = TypeShape::Opaque("SomeType".to_string());
        assert!(!sub(&opaque, &string()), "Opaque is NOT <: string");
        assert!(!sub(&string(), &opaque), "string is NOT <: Opaque");
        // any overrides opaque on the sup side
        assert!(
            sub(&string(), &p(PrimitiveKind::Any)),
            "string <: any still works"
        );
        // but opaque <: any should return true (any is checked first)
        assert!(sub(&opaque, &p(PrimitiveKind::Any)), "Opaque <: any");
    }

    // ── 8. Object structural subtype with extra fields ────────────────────────

    #[test]
    fn test_object_structural_extra_fields_allowed() {
        // sup: { x: number }
        let mut sup_fields = BTreeMap::new();
        sup_fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                shape: number(),
            },
        );

        // sub: { x: number, y: string } — has extra field y
        let mut sub_fields = BTreeMap::new();
        sub_fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                shape: number(),
            },
        );
        sub_fields.insert(
            "y".to_string(),
            FieldShape {
                optional: false,
                shape: string(),
            },
        );

        assert!(
            sub(
                &TypeShape::Object(sub_fields),
                &TypeShape::Object(sup_fields)
            ),
            "object with extra field is still a subtype"
        );
    }

    // ── 9. Object missing required field ─────────────────────────────────────

    #[test]
    fn test_object_missing_required_field_not_subtype() {
        // sup: { x: number, y: string }
        let mut sup_fields = BTreeMap::new();
        sup_fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                shape: number(),
            },
        );
        sup_fields.insert(
            "y".to_string(),
            FieldShape {
                optional: false,
                shape: string(),
            },
        );

        // sub: { x: number } — missing required y
        let mut sub_fields = BTreeMap::new();
        sub_fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                shape: number(),
            },
        );

        assert!(
            !sub(
                &TypeShape::Object(sub_fields),
                &TypeShape::Object(sup_fields)
            ),
            "object missing required field is NOT a subtype"
        );
    }

    // ── 10. Object missing optional field is still subtype ───────────────────

    #[test]
    fn test_object_missing_optional_field_is_subtype() {
        // sup: { x: number, y?: string }
        let mut sup_fields = BTreeMap::new();
        sup_fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                shape: number(),
            },
        );
        sup_fields.insert(
            "y".to_string(),
            FieldShape {
                optional: true,
                shape: string(),
            },
        );

        // sub: { x: number } — missing optional y
        let mut sub_fields = BTreeMap::new();
        sub_fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                shape: number(),
            },
        );

        assert!(
            sub(
                &TypeShape::Object(sub_fields),
                &TypeShape::Object(sup_fields)
            ),
            "object missing optional field IS a subtype"
        );
    }

    // ── 11. Union sub distribution ────────────────────────────────────────────

    #[test]
    fn test_union_sub_distribution() {
        // (string | number) <: (string | number | boolean) should be true
        let sub_union = TypeShape::Union(vec![string(), number()]);
        let sup_union = TypeShape::Union(vec![string(), number(), boolean()]);
        assert!(
            sub(&sub_union, &sup_union),
            "(string | number) <: (string | number | boolean)"
        );

        // (string | number | boolean) <: (string | number) should be false
        let sub_union2 = TypeShape::Union(vec![string(), number(), boolean()]);
        let sup_union2 = TypeShape::Union(vec![string(), number()]);
        assert!(
            !sub(&sub_union2, &sup_union2),
            "(string | number | boolean) is NOT <: (string | number)"
        );
    }

    // ── 12. Union sup any-match ───────────────────────────────────────────────

    #[test]
    fn test_union_sup_any_match() {
        let sup_union = TypeShape::Union(vec![string(), number()]);
        // string <: (string | number) — matches string member
        assert!(sub(&string(), &sup_union), "string <: (string | number)");
        // boolean <: (string | number) — no match
        assert!(
            !sub(&boolean(), &sup_union),
            "boolean is NOT <: (string | number)"
        );
    }

    // ── 13. Function fewer params is subtype ──────────────────────────────────

    #[test]
    fn test_function_fewer_params_is_subtype() {
        // sub: (x: string) => void
        let sub_fn = TypeShape::Function {
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: string(),
            }],
            return_type: Box::new(p(PrimitiveKind::Void)),
        };
        // sup: (x: string, y: number) => void
        let sup_fn = TypeShape::Function {
            params: vec![
                ParamShape {
                    name: "x".to_string(),
                    optional: false,
                    rest: false,
                    shape: string(),
                },
                ParamShape {
                    name: "y".to_string(),
                    optional: false,
                    rest: false,
                    shape: number(),
                },
            ],
            return_type: Box::new(p(PrimitiveKind::Void)),
        };
        assert!(
            sub(&sub_fn, &sup_fn),
            "(x: string) => void <: (x: string, y: number) => void (callback pattern)"
        );
    }

    // ── 14. Function contravariant params ─────────────────────────────────────

    #[test]
    fn test_function_contravariant_params() {
        // sub: (x: string | number) => void
        // sup: (x: string) => void
        // sup's param (string) must be <: sub's param (string | number)
        // string <: (string | number) == true, so sub IS a subtype of sup
        let sub_fn = TypeShape::Function {
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Union(vec![string(), number()]),
            }],
            return_type: Box::new(p(PrimitiveKind::Void)),
        };
        let sup_fn = TypeShape::Function {
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: string(),
            }],
            return_type: Box::new(p(PrimitiveKind::Void)),
        };
        assert!(
            sub(&sub_fn, &sup_fn),
            "(string | number) -> void <: (string) -> void (contravariant param)"
        );
    }

    // ── 15. Function covariant return ─────────────────────────────────────────

    #[test]
    fn test_function_covariant_return() {
        // sub: () => string
        // sup: () => (string | number)
        // string <: (string | number) is true => sub IS a subtype
        let sub_fn = TypeShape::Function {
            params: vec![],
            return_type: Box::new(string()),
        };
        let sup_fn = TypeShape::Function {
            params: vec![],
            return_type: Box::new(TypeShape::Union(vec![string(), number()])),
        };
        assert!(
            sub(&sub_fn, &sup_fn),
            "() => string <: () => (string | number) (covariant return)"
        );
    }

    // ── 16. Function return type widened breaks subtyping ─────────────────────

    #[test]
    fn test_function_return_widened_breaks_subtyping() {
        // sub: () => string
        // sup: () => number
        // string is NOT <: number => sub is NOT a subtype
        let sub_fn = TypeShape::Function {
            params: vec![],
            return_type: Box::new(string()),
        };
        let sup_fn = TypeShape::Function {
            params: vec![],
            return_type: Box::new(number()),
        };
        assert!(
            !sub(&sub_fn, &sup_fn),
            "() => string is NOT <: () => number"
        );
    }

    // ── 17. Depth limit returns false ─────────────────────────────────────────

    #[test]
    fn test_depth_limit_returns_false() {
        // Calling with depth = MAX_DEPTH should return false
        assert!(
            !is_subtype_of(&string(), &string(), MAX_DEPTH),
            "at MAX_DEPTH even reflexive types return false"
        );
    }

    // ── 18. Array covariant ───────────────────────────────────────────────────

    #[test]
    fn test_array_covariant() {
        let str_arr = TypeShape::Array(Box::new(string()));
        let num_arr = TypeShape::Array(Box::new(number()));
        assert!(sub(&str_arr, &str_arr), "string[] <: string[] (reflexive)");
        assert!(!sub(&str_arr, &num_arr), "string[] is NOT <: number[]");
    }

    // ── 19. Tuple same-length subtype ─────────────────────────────────────────

    #[test]
    fn test_tuple_same_length_subtype() {
        let t1 = TypeShape::Tuple(vec![string(), number()]);
        let t2 = TypeShape::Tuple(vec![string(), number()]);
        let t3 = TypeShape::Tuple(vec![string(), string()]);
        let t4 = TypeShape::Tuple(vec![string()]);

        assert!(sub(&t1, &t2), "[string, number] <: [string, number]");
        assert!(
            !sub(&t1, &t3),
            "[string, number] is NOT <: [string, string]"
        );
        assert!(
            !sub(&t1, &t4),
            "[string, number] is NOT <: [string] (different length)"
        );
    }

    // ── 20. Conditional/Mapped/Indexed/Template return false (D-10) ───────────

    #[test]
    fn test_incomparable_types_return_false() {
        use crate::types::shape::{MappedModifier, TemplatePart};

        let cond = TypeShape::Conditional {
            check: Box::new(string()),
            extends: Box::new(string()),
            then_type: Box::new(string()),
            else_type: Box::new(number()),
        };
        let mapped = TypeShape::Mapped {
            key_type: Box::new(string()),
            value_type: Box::new(number()),
            modifier: Some(MappedModifier::Optional),
        };
        let indexed = TypeShape::Indexed {
            base: Box::new(TypeShape::Object(BTreeMap::new())),
            index: Box::new(string()),
        };
        let template = TypeShape::Template(vec![TemplatePart::Literal("prefix_".to_string())]);

        // All should return false regardless of direction
        assert!(!sub(&cond, &string()), "Conditional is NOT <: string");
        assert!(!sub(&string(), &cond), "string is NOT <: Conditional");
        assert!(!sub(&mapped, &string()), "Mapped is NOT <: string");
        assert!(!sub(&string(), &mapped), "string is NOT <: Mapped");
        assert!(!sub(&indexed, &string()), "Indexed is NOT <: string");
        assert!(!sub(&string(), &indexed), "string is NOT <: Indexed");
        assert!(!sub(&template, &string()), "Template is NOT <: string");
        assert!(!sub(&string(), &template), "string is NOT <: Template");
    }

    // ── 21–25. format_type tests ──────────────────────────────────────────────

    #[test]
    fn test_format_type_primitive() {
        use PrimitiveKind::*;
        assert_eq!(format_type(&p(String)), "string");
        assert_eq!(format_type(&p(Number)), "number");
        assert_eq!(format_type(&p(Boolean)), "boolean");
        assert_eq!(format_type(&p(Void)), "void");
        assert_eq!(format_type(&p(Null)), "null");
        assert_eq!(format_type(&p(Undefined)), "undefined");
        assert_eq!(format_type(&p(Never)), "never");
        assert_eq!(format_type(&p(Any)), "any");
        assert_eq!(format_type(&p(Unknown)), "unknown");
        assert_eq!(format_type(&p(Symbol)), "symbol");
        assert_eq!(format_type(&p(Bigint)), "bigint");
    }

    #[test]
    fn test_format_type_union() {
        let union = TypeShape::Union(vec![string(), number()]);
        assert_eq!(format_type(&union), "string | number");
    }

    #[test]
    fn test_format_type_array() {
        let arr = TypeShape::Array(Box::new(string()));
        assert_eq!(format_type(&arr), "string[]");
    }

    #[test]
    fn test_format_type_literal_string() {
        let lit = TypeShape::Literal(LiteralKind::String("hello".to_string()));
        assert_eq!(format_type(&lit), "\"hello\"");
        assert!(format_type(&lit).contains("hello"));
    }

    #[test]
    fn test_format_type_opaque() {
        let opaque = TypeShape::Opaque("MyType".to_string());
        assert_eq!(format_type(&opaque), "MyType");
    }

    // Extra: format intersection
    #[test]
    fn test_format_type_intersection() {
        let inter = TypeShape::Intersection(vec![string(), number()]);
        assert_eq!(format_type(&inter), "string & number");
    }

    // Extra: format tuple
    #[test]
    fn test_format_type_tuple() {
        let tuple = TypeShape::Tuple(vec![string(), number()]);
        assert_eq!(format_type(&tuple), "[string, number]");
    }

    // Extra: format function
    #[test]
    fn test_format_type_function() {
        let func = TypeShape::Function {
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: string(),
            }],
            return_type: Box::new(boolean()),
        };
        assert_eq!(format_type(&func), "(x: string) => boolean");
    }

    // Extra: generic
    #[test]
    fn test_format_type_generic() {
        let gen = TypeShape::Generic {
            name: "Promise".to_string(),
            args: vec![string()],
        };
        assert_eq!(format_type(&gen), "Promise<string>");
    }
}
