use crate::ecosystems::node::compare::rules as package_rules;
use crate::languages::javascript::compare::rules as runtime_rules;
use crate::languages::typescript::compare::rules as type_rules;
use crate::languages::typescript::types::shape::{
    FieldShape, LiteralKind, ParamShape, PrimitiveKind, TypeShape,
};
use crate::languages::typescript::types::subtype::*;
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

// ── 7. Opaque assume-breaking ──────────────────────────────────────

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
            write_type: None,
            optional: false,
            readonly: false,
            shape: number(),
        },
    );

    // sub: { x: number, y: string } — has extra field y
    let mut sub_fields = BTreeMap::new();
    sub_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: number(),
        },
    );
    sub_fields.insert(
        "y".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
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
            write_type: None,
            optional: false,
            readonly: false,
            shape: number(),
        },
    );
    sup_fields.insert(
        "y".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: string(),
        },
    );

    // sub: { x: number } — missing required y
    let mut sub_fields = BTreeMap::new();
    sub_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
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
            write_type: None,
            optional: false,
            readonly: false,
            shape: number(),
        },
    );
    sup_fields.insert(
        "y".to_string(),
        FieldShape {
            write_type: None,
            optional: true,
            readonly: false,
            shape: string(),
        },
    );

    // sub: { x: number } — missing optional y
    let mut sub_fields = BTreeMap::new();
    sub_fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
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
    use crate::languages::typescript::types::shape::TupleElement;
    let te = |s: TypeShape| TupleElement {
        label: None,
        optional: false,
        rest: false,
        shape: s,
    };
    let t1 = TypeShape::Tuple(vec![te(string()), te(number())]);
    let t2 = TypeShape::Tuple(vec![te(string()), te(number())]);
    let t3 = TypeShape::Tuple(vec![te(string()), te(string())]);
    let t4 = TypeShape::Tuple(vec![te(string())]);

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

/// REF-10: labels are nominal metadata. Structural subtype
/// must ignore them — `[a: number]` is both ways a subtype of `[number]`.
#[test]
fn test_tuple_subtype_label_blind() {
    use crate::languages::typescript::types::shape::TupleElement;
    let labeled = TypeShape::Tuple(vec![TupleElement {
        label: Some("a".to_string()),
        optional: false,
        rest: false,
        shape: number(),
    }]);
    let unlabeled = TypeShape::Tuple(vec![TupleElement {
        label: None,
        optional: false,
        rest: false,
        shape: number(),
    }]);
    assert!(
        sub(&labeled, &unlabeled),
        "labeled <: unlabeled (same shape)"
    );
    assert!(
        sub(&unlabeled, &labeled),
        "unlabeled <: labeled (same shape)"
    );
}

// ── 20. Conditional/Mapped/Indexed/Template return false ───────────

#[test]
fn test_incomparable_types_return_false() {
    use crate::languages::typescript::types::shape::TemplatePart;

    let cond = TypeShape::Conditional {
        check: Box::new(string()),
        extends: Box::new(string()),
        then_type: Box::new(string()),
        else_type: Box::new(number()),
    };
    let mapped = TypeShape::Mapped {
        key_type: Box::new(string()),
        value_type: Box::new(number()),
        key_name: "K".into(),
        name_type: None,
        readonly: None,
        optional: Some(true),
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
    use crate::languages::typescript::types::shape::PrimitiveKind::*;
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
    use crate::languages::typescript::types::shape::TupleElement;
    let te = |s: TypeShape| TupleElement {
        label: None,
        optional: false,
        rest: false,
        shape: s,
    };
    let tuple = TypeShape::Tuple(vec![te(string()), te(number())]);
    assert_eq!(format_type(&tuple), "[string, number]");
}

/// REF-10: `format_type` must render labeled tuple elements
/// with the label prefix and the optional/rest markers.
#[test]
fn test_format_type_tuple_labeled() {
    use crate::languages::typescript::types::shape::TupleElement;
    let tuple = TypeShape::Tuple(vec![
        TupleElement {
            label: Some("x".to_string()),
            optional: false,
            rest: false,
            shape: number(),
        },
        TupleElement {
            label: Some("y".to_string()),
            optional: true,
            rest: false,
            shape: string(),
        },
        TupleElement {
            label: Some("rest".to_string()),
            optional: false,
            rest: true,
            shape: TypeShape::Array(Box::new(boolean())),
        },
    ]);
    assert_eq!(
        format_type(&tuple),
        "[x: number, y?: string, ...rest: boolean[]]"
    );
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

// ──: TLF rule constant values ───────────────────────────────────
#[test]
fn language_feature_rule_identifiers() {
    assert_eq!(
        type_rules::RULE_CONSTRUCT_SIGNATURE_REMOVED,
        "construct-signature-removed"
    );
    assert_eq!(type_rules::RULE_THIS_PARAM_CHANGED, "this-param-changed");
    assert_eq!(type_rules::RULE_ASSERTION_WIDENED, "assertion-widened");
    assert_eq!(
        type_rules::RULE_GENERIC_VARIANCE_CHANGED,
        "generic-variance-changed"
    );
    assert_eq!(
        runtime_rules::RULE_CLASS_ACCESSOR_CHANGED,
        "class-accessor-changed"
    );
    assert_eq!(
        type_rules::RULE_TYPE_ONLY_REEXPORT_CHANGED,
        "type-only-reexport-changed"
    );
}

// ──: BUN/CHUNK rule constant values ─────────────────────────────
#[test]
fn bundler_rule_identifiers() {
    assert_eq!(
        package_rules::RULE_CJS_BUNDLER_EXPORT_DETECTED,
        "cjs-bundler-export-detected"
    );
    assert_eq!(
        package_rules::RULE_CHUNK_PATH_NORMALIZED,
        "chunk-path-normalized"
    );
}

// ──: IFT rule constant values ───────────────────────────────────
#[test]
fn interface_rule_identifiers() {
    assert_eq!(
        type_rules::RULE_INTERFACE_PROPERTY_MADE_REQUIRED,
        "interface-property-made-required"
    );
    assert_eq!(
        type_rules::RULE_TYPE_ALIAS_READONLY_CHANGED,
        "type-alias-readonly-changed"
    );
    assert_eq!(
        runtime_rules::RULE_CONSTRUCTOR_PARAM_MADE_REQUIRED,
        "constructor-param-made-required"
    );
}

// ──: META rule constant values ──────────────────────────────────
#[test]
fn metadata_rule_identifiers() {
    assert_eq!(
        package_rules::RULE_SIDE_EFFECTS_WIDENED,
        "side-effects-widened"
    );
    assert_eq!(
        package_rules::RULE_PEER_DEP_OPTIONAL_REMOVED,
        "peer-dep-optional-removed"
    );
    assert_eq!(type_rules::RULE_EXPORT_DEPRECATED, "export-deprecated");
    assert_eq!(
        package_rules::RULE_BARREL_UNRESOLVABLE,
        "barrel-unresolvable"
    );
    assert_eq!(
        type_rules::RULE_AUGMENT_GLOBAL_CHANGED,
        "augment-global-changed"
    );
    assert_eq!(
        type_rules::RULE_AUGMENT_MODULE_CHANGED,
        "augment-module-changed"
    );
}

// ──: TS type-system features rule constant values ───────────────
#[test]
fn type_system_rule_identifiers() {
    assert_eq!(
        type_rules::RULE_TYPE_NARROWING_WIDENED,
        "type-narrowing-widened"
    );
    assert_eq!(
        type_rules::RULE_INFER_EXTENDS_CONSTRAINT_CHANGED,
        "infer-extends-constraint-changed"
    );
    assert_eq!(
        type_rules::RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED,
        "generic-inference-constraint-removed"
    );
    assert_eq!(type_rules::RULE_UNION_VARIANT_ADDED, "union-variant-added");
    assert_eq!(
        type_rules::RULE_INDEX_SIGNATURE_KEY_PATTERN_CHANGED,
        "index-signature-key-pattern-changed"
    );
}

// ──: class-surface completion rule constant values ──────────────
#[test]
fn class_surface_rule_identifiers() {
    assert_eq!(
        runtime_rules::RULE_CLASS_FIELD_MADE_PRIVATE,
        "class-field-made-private"
    );
    assert_eq!(
        runtime_rules::RULE_CLASS_FIELD_MADE_PUBLIC,
        "class-field-made-public"
    );
}

// ──: imports-field diff rule constant values ────────────────────
#[test]
fn package_import_rule_identifiers() {
    assert_eq!(package_rules::RULE_IMPORT_ADDED, "import-added");
    assert_eq!(package_rules::RULE_IMPORT_REMOVED, "import-removed");
    assert_eq!(
        package_rules::RULE_IMPORT_CONDITION_CHANGED,
        "import-condition-changed"
    );
    assert_eq!(
        package_rules::RULE_IMPORT_PATTERN_REFINED,
        "import-pattern-refined"
    );
    assert_eq!(package_rules::RULE_IMPORT_REROUTE, "import-reroute");
}

#[test]
fn iterator_next_inputs_are_contravariant_while_outputs_are_covariant() {
    use crate::languages::typescript::types::parser::parse_type_string;
    for name in [
        "Generator",
        "AsyncGenerator",
        "Iterator",
        "AsyncIterator",
        "Iterable",
        "AsyncIterable",
        "IterableIterator",
        "AsyncIterableIterator",
    ] {
        let accepts_both = parse_type_string(&format!("{name}<number, void, string | number>"));
        let accepts_string = parse_type_string(&format!("{name}<number, void, string>"));
        assert!(
            sub(&accepts_both, &accepts_string),
            "{name} must preserve broader next inputs"
        );
        assert!(
            !sub(&accepts_string, &accepts_both),
            "{name} must reject narrower next inputs"
        );
        let yields_both = parse_type_string(&format!("{name}<number | string, void, string>"));
        assert!(sub(&accepts_string, &yields_both));
        assert!(!sub(&yields_both, &accepts_string));
    }
}
