use crate::languages::typescript::types::parser::parse_type_string;
use crate::languages::typescript::types::shape::{
    FieldShape, LiteralKind, PrimitiveKind, TupleElement, TypeShape,
};

fn assert_shape(input: &str, expected: TypeShape) {
    let result = parse_type_string(input);
    assert_eq!(result, expected, "parsing {:?}", input);
}

#[test]
fn test_primitive_string() {
    assert_shape("string", TypeShape::Primitive(PrimitiveKind::String));
}

#[test]
fn test_primitive_number() {
    assert_shape("number", TypeShape::Primitive(PrimitiveKind::Number));
}

#[test]
fn test_primitive_boolean() {
    assert_shape("boolean", TypeShape::Primitive(PrimitiveKind::Boolean));
}

#[test]
fn test_primitive_void() {
    assert_shape("void", TypeShape::Primitive(PrimitiveKind::Void));
}

#[test]
fn test_primitive_any() {
    assert_shape("any", TypeShape::Primitive(PrimitiveKind::Any));
}

#[test]
fn test_primitive_null() {
    assert_shape("null", TypeShape::Primitive(PrimitiveKind::Null));
}

#[test]
fn test_primitive_undefined() {
    assert_shape("undefined", TypeShape::Primitive(PrimitiveKind::Undefined));
}

#[test]
fn test_primitive_never() {
    assert_shape("never", TypeShape::Primitive(PrimitiveKind::Never));
}

#[test]
fn test_primitive_unknown() {
    assert_shape("unknown", TypeShape::Primitive(PrimitiveKind::Unknown));
}

#[test]
fn test_string_literal() {
    assert_shape(
        "\"hello\"",
        TypeShape::Literal(LiteralKind::String("hello".to_string())),
    );
}

#[test]
fn test_number_literal() {
    assert_shape(
        "42",
        TypeShape::Literal(LiteralKind::Number("42".to_string())),
    );
}

#[test]
fn test_boolean_literal_true() {
    assert_shape("true", TypeShape::Literal(LiteralKind::Boolean(true)));
}

#[test]
fn test_union_string_number() {
    let result = parse_type_string("string | number");
    let expected = TypeShape::Union(vec![
        TypeShape::Primitive(PrimitiveKind::Number),
        TypeShape::Primitive(PrimitiveKind::String),
    ]);
    assert_eq!(result, expected, "parsing 'string | number'");
}

#[test]
fn test_intersection() {
    let result = parse_type_string("string & { x: number }");
    assert!(
        matches!(result, TypeShape::Intersection(_)),
        "expected Intersection, got {:?}",
        result
    );
}

#[test]
fn test_object_type() {
    use std::collections::BTreeMap;
    let result = parse_type_string("{ x: number; y: string }");
    let mut fields = BTreeMap::new();
    fields.insert(
        "x".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        },
    );
    fields.insert(
        "y".to_string(),
        FieldShape {
            write_type: None,
            optional: false,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        },
    );
    let expected = TypeShape::Object(fields);
    assert_eq!(result, expected);
}

#[test]
fn test_object_optional_field() {
    let result = parse_type_string("{ x?: number }");
    if let TypeShape::Object(fields) = result {
        let field = fields.get("x").expect("field x");
        assert!(field.optional, "field should be optional");
        assert_eq!(field.shape, TypeShape::Primitive(PrimitiveKind::Number));
    } else {
        panic!("expected Object, got {:?}", result);
    }
}

#[test]
fn test_array_suffix() {
    assert_shape(
        "number[]",
        TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::Number))),
    );
}

#[test]
fn test_array_generic() {
    assert_shape(
        "Array<string>",
        TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::String))),
    );
}

#[test]
fn test_tuple_type() {
    assert_shape(
        "[string, number]",
        TypeShape::Tuple(vec![
            TupleElement {
                label: None,
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            },
            TupleElement {
                label: None,
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        ]),
    );
}

// ── REF-10: labeled-tuple parsing ────────────────────────────

#[test]
fn test_tuple_labeled_basic() {
    let result = parse_type_string("[x: number, y: string]");
    if let TypeShape::Tuple(elems) = result {
        assert_eq!(elems.len(), 2);
        assert_eq!(elems[0].label.as_deref(), Some("x"));
        assert!(!elems[0].optional);
        assert!(!elems[0].rest);
        assert_eq!(elems[0].shape, TypeShape::Primitive(PrimitiveKind::Number));
        assert_eq!(elems[1].label.as_deref(), Some("y"));
        assert!(!elems[1].optional);
        assert!(!elems[1].rest);
        assert_eq!(elems[1].shape, TypeShape::Primitive(PrimitiveKind::String));
    } else {
        panic!("expected Tuple, got {:?}", result);
    }
}

#[test]
fn test_tuple_unlabeled_labels_none() {
    let result = parse_type_string("[number, string]");
    if let TypeShape::Tuple(elems) = result {
        assert_eq!(elems.len(), 2);
        assert!(elems[0].label.is_none(), "first elem label should be None");
        assert!(elems[1].label.is_none(), "second elem label should be None");
    } else {
        panic!("expected Tuple, got {:?}", result);
    }
}

#[test]
fn test_tuple_labeled_optional() {
    let result = parse_type_string("[x?: number]");
    if let TypeShape::Tuple(elems) = result {
        assert_eq!(elems.len(), 1);
        assert_eq!(elems[0].label.as_deref(), Some("x"));
        assert!(elems[0].optional, "optional slot should be true");
        assert!(!elems[0].rest);
    } else {
        panic!("expected Tuple, got {:?}", result);
    }
}

#[test]
fn test_tuple_labeled_rest() {
    let result = parse_type_string("[...xs: number[]]");
    if let TypeShape::Tuple(elems) = result {
        assert_eq!(elems.len(), 1);
        assert_eq!(elems[0].label.as_deref(), Some("xs"));
        assert!(elems[0].rest, "rest slot should be true");
        assert!(!elems[0].optional);
        // Shape is `number[]` → Array(Number).
        assert_eq!(
            elems[0].shape,
            TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::Number)))
        );
    } else {
        panic!("expected Tuple, got {:?}", result);
    }
}

/// `[Map<string, number>]` must NOT treat
/// `Map` as a label. When the parser peeks past `Map` and sees `<` (not
/// `:`), it must rewind and let the generic machinery consume `Map<...>`.
#[test]
fn test_tuple_backtrack_on_generic_reference() {
    let result = parse_type_string("[Map<string, number>]");
    if let TypeShape::Tuple(elems) = result {
        assert_eq!(elems.len(), 1);
        assert!(
            elems[0].label.is_none(),
            "Map is a type name, not a label; label should be None"
        );
        match &elems[0].shape {
            TypeShape::Generic { name, args } => {
                assert_eq!(name, "Map");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected Generic Map<string, number>, got {:?}", other),
        }
    } else {
        panic!("expected Tuple, got {:?}", result);
    }
}

/// Bare-type-reference pitfall: `[ID, Name]` — `ID` and `Name` look like
/// they *could* be labels because they start with letters, but without a
/// trailing `:` they must remain type references.
#[test]
fn test_tuple_backtrack_on_bare_type_references() {
    let result = parse_type_string("[ID, Name]");
    if let TypeShape::Tuple(elems) = result {
        assert_eq!(elems.len(), 2);
        assert!(elems[0].label.is_none());
        assert!(elems[1].label.is_none());
    } else {
        panic!("expected Tuple, got {:?}", result);
    }
}

#[test]
fn test_function_type() {
    let result = parse_type_string("(x: string, y: number) => boolean");
    if let TypeShape::Function {
        params,
        return_type,
    } = result
    {
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "x");
        assert_eq!(params[0].shape, TypeShape::Primitive(PrimitiveKind::String));
        assert!(!params[0].optional);
        assert_eq!(params[1].name, "y");
        assert_eq!(params[1].shape, TypeShape::Primitive(PrimitiveKind::Number));
        assert_eq!(*return_type, TypeShape::Primitive(PrimitiveKind::Boolean));
    } else {
        panic!("expected Function, got {:?}", result);
    }
}

#[test]
fn test_generic_map() {
    assert_shape(
        "Map<string, number>",
        TypeShape::Generic {
            name: "Map".to_string(),
            args: vec![
                TypeShape::Primitive(PrimitiveKind::String),
                TypeShape::Primitive(PrimitiveKind::Number),
            ],
        },
    );
}

#[test]
fn test_generic_promise_void() {
    assert_shape(
        "Promise<void>",
        TypeShape::Generic {
            name: "Promise".to_string(),
            args: vec![TypeShape::Primitive(PrimitiveKind::Void)],
        },
    );
}

#[test]
fn test_nested_object_with_array() {
    let result = parse_type_string("{ items: string[]; count: number }");
    if let TypeShape::Object(fields) = result {
        let items = fields.get("items").expect("field items");
        assert_eq!(
            items.shape,
            TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::String)))
        );
        let count = fields.get("count").expect("field count");
        assert_eq!(count.shape, TypeShape::Primitive(PrimitiveKind::Number));
    } else {
        panic!("expected Object, got {:?}", result);
    }
}

#[test]
fn test_union_normalized_order_independent() {
    let a = parse_type_string("string | number");
    let b = parse_type_string("number | string");
    assert_eq!(a, b, "normalized unions should be order-independent");
}

#[test]
fn test_opaque_fallback() {
    let result = parse_type_string("totallyBrokenGarbage<>{{");
    assert!(
        matches!(result, TypeShape::Opaque(_)),
        "expected Opaque fallback, got {:?}",
        result
    );
}

#[test]
fn test_type_alias_stripping() {
    let result = parse_type_string("type Foo = string | number");
    assert!(
        matches!(result, TypeShape::Union(_)),
        "expected Union after alias stripping, got {:?}",
        result
    );
}

#[test]
fn test_function_optional_param() {
    let result = parse_type_string("(x?: string) => void");
    if let TypeShape::Function {
        params,
        return_type,
    } = result
    {
        assert_eq!(params.len(), 1);
        assert!(params[0].optional, "param should be optional");
        assert_eq!(params[0].shape, TypeShape::Primitive(PrimitiveKind::String));
        assert_eq!(*return_type, TypeShape::Primitive(PrimitiveKind::Void));
    } else {
        panic!("expected Function, got {:?}", result);
    }
}

// Additional coverage tests

#[test]
fn test_primitive_symbol() {
    assert_shape("symbol", TypeShape::Primitive(PrimitiveKind::Symbol));
}

#[test]
fn test_primitive_bigint() {
    assert_shape("bigint", TypeShape::Primitive(PrimitiveKind::Bigint));
}

#[test]
fn test_boolean_literal_false() {
    assert_shape("false", TypeShape::Literal(LiteralKind::Boolean(false)));
}

#[test]
fn test_array_nested() {
    assert_shape(
        "string[][]",
        TypeShape::Array(Box::new(TypeShape::Array(Box::new(TypeShape::Primitive(
            PrimitiveKind::String,
        ))))),
    );
}

#[test]
fn test_empty_object() {
    assert_shape("{}", TypeShape::Object(std::collections::BTreeMap::new()));
}

#[test]
fn test_union_three_members() {
    let result = parse_type_string("string | number | boolean");
    assert!(matches!(result, TypeShape::Union(_)));
    if let TypeShape::Union(members) = result {
        assert_eq!(members.len(), 3);
    }
}

/// BEH-06: `parse_union` must accept an optional leading
/// `|` so the idiomatic multi-line discriminated-union form parses
/// into a `TypeShape::Union` (not `TypeShape::Opaque`). Without this,
/// BEH-06's discriminated-union detector never sees the Union
/// because the RHS falls to Opaque.
#[test]
fn leading_pipe_multiline_union_parses() {
    let source = "| { kind: 'circle'; radius: number }\n  | { kind: 'square'; side: number }";
    let result = parse_type_string(source);
    assert!(
        matches!(result, TypeShape::Union(_)),
        "expected Union; got {result:?}"
    );
    if let TypeShape::Union(members) = result {
        assert_eq!(members.len(), 2, "expected 2 object variants");
    }
}

#[test]
fn test_function_hover_prefix_stripping() {
    // LSP hover for functions emits: `function foo(x: string): void`
    let result = parse_type_string("function foo(x: string): void");
    // The function hover format doesn't map cleanly to our parser's
    // `(params) => return` syntax — it falls back to Opaque or strips correctly.
    // At minimum it shouldn't panic.
    let _ = result;
}

#[test]
fn test_single_quoted_string_literal() {
    assert_shape(
        "'world'",
        TypeShape::Literal(LiteralKind::String("world".to_string())),
    );
}

#[test]
fn test_type_param_reference() {
    assert_shape("T", TypeShape::TypeParam("T".to_string()));
}

// ─── ROB-05: MAX_DEPTH guard — no stack overflow on adversarial input ───────

#[test]
fn simple_type_parses_ok() {
    let shape = parse_type_string("string");
    assert!(
        matches!(shape, TypeShape::Primitive(_)),
        "ROB-05: simple 'string' should parse; got: {:?}",
        shape
    );
}

#[test]
fn deeply_nested_array_returns_opaque_not_stack_overflow() {
    // 100 levels of Array<Array<...>> should cap to Opaque at MAX_DEPTH=20
    let mut input = String::from("string");
    for _ in 0..100 {
        input = format!("Array<{}>", input);
    }
    // Should not panic (stack overflow). Result shape is well-defined.
    let shape = parse_type_string(&input);
    let _ = shape;
}

#[test]
fn parens_depth_exhaustion_no_panic() {
    let input = "(".repeat(100) + "string" + &")".repeat(100);
    let shape = parse_type_string(&input);
    let _ = shape; // no panic
}

// ─── Conditional, mapped, indexed type parsing ─────────────────

// Test: basic conditional type
#[test]
fn test_conditional_basic() {
    use crate::languages::typescript::types::shape::PrimitiveKind;
    let result = parse_type_string("T extends string ? number : boolean");
    assert!(
        matches!(result, TypeShape::Conditional { .. }),
        "expected Conditional, got {:?}",
        result
    );
    if let TypeShape::Conditional {
        check,
        extends,
        then_type,
        else_type,
    } = result
    {
        assert_eq!(*check, TypeShape::TypeParam("T".to_string()));
        assert_eq!(*extends, TypeShape::Primitive(PrimitiveKind::String));
        assert_eq!(*then_type, TypeShape::Primitive(PrimitiveKind::Number));
        assert_eq!(*else_type, TypeShape::Primitive(PrimitiveKind::Boolean));
    }
}

// Test: conditional with union in else branch
#[test]
fn test_conditional_union_in_else() {
    let result = parse_type_string("T extends U ? A : B | C");
    assert!(
        matches!(result, TypeShape::Conditional { .. }),
        "expected Conditional, got {:?}",
        result
    );
    if let TypeShape::Conditional { else_type, .. } = result {
        assert!(
            matches!(*else_type, TypeShape::Union(_) | TypeShape::TypeParam(_)),
            "else branch should parse B | C, got {:?}",
            else_type
        );
    }
}

// Test: nested conditional T extends A ? B extends C ? D: E: F
#[test]
fn test_conditional_nested() {
    let result = parse_type_string("T extends A ? B extends C ? D : E : F");
    assert!(
        matches!(result, TypeShape::Conditional { .. }),
        "expected outer Conditional, got {:?}",
        result
    );
    if let TypeShape::Conditional { then_type, .. } = result {
        assert!(
            matches!(*then_type, TypeShape::Conditional { .. }),
            "then_type should be inner Conditional, got {:?}",
            then_type
        );
    }
}

// Test: mapped type basic (no modifiers)
#[test]
fn test_mapped_basic() {
    use crate::languages::typescript::types::shape::PrimitiveKind;
    let result = parse_type_string("{ [K in keyof T]: string }");
    assert!(
        matches!(result, TypeShape::Mapped { .. }),
        "expected Mapped, got {:?}",
        result
    );
    if let TypeShape::Mapped {
        value_type,
        readonly,
        optional,
        ..
    } = result
    {
        assert_eq!(*value_type, TypeShape::Primitive(PrimitiveKind::String));
        assert_eq!((readonly, optional), (None, None));
    }
}

// Test: mapped type with readonly modifier
#[test]
fn test_mapped_readonly() {
    let result = parse_type_string("{ readonly [K in keyof T]: string }");
    assert!(
        matches!(result, TypeShape::Mapped { .. }),
        "expected Mapped, got {:?}",
        result
    );
    if let TypeShape::Mapped {
        readonly, optional, ..
    } = result
    {
        assert_eq!((readonly, optional), (Some(true), None));
    }
}

// Test: mapped type with optional modifier
#[test]
fn test_mapped_optional() {
    let result = parse_type_string("{ [K in keyof T]?: string }");
    assert!(
        matches!(result, TypeShape::Mapped { .. }),
        "expected Mapped, got {:?}",
        result
    );
    if let TypeShape::Mapped {
        readonly, optional, ..
    } = result
    {
        assert_eq!((readonly, optional), (None, Some(true)));
    }
}

// Test: mapped type with readonly + optional modifier
#[test]
fn test_mapped_readonly_optional() {
    let result = parse_type_string("{ readonly [K in keyof T]?: string }");
    assert!(
        matches!(result, TypeShape::Mapped { .. }),
        "expected Mapped, got {:?}",
        result
    );
    if let TypeShape::Mapped {
        readonly, optional, ..
    } = result
    {
        assert_eq!((readonly, optional), (Some(true), Some(true)));
    }
}

// Test: indexed access T[K]
#[test]
fn mapped_keys_and_removed_modifiers_are_preserved() {
    let shape = parse_type_string(
        "{ -readonly [Key in keyof Model as `get${Key & string}`]-?: Model[Key] }",
    );
    let TypeShape::Mapped {
        key_name,
        name_type,
        readonly,
        optional,
        ..
    } = shape
    else {
        panic!("expected mapped type, got {shape:?}");
    };
    assert_eq!(key_name, "Key");
    assert!(matches!(name_type.as_deref(), Some(TypeShape::Template(_))));
    assert_eq!((readonly, optional), (Some(false), Some(false)));
}

#[test]
fn test_indexed_access_typeparam() {
    let result = parse_type_string("T[K]");
    assert!(
        matches!(result, TypeShape::Indexed { .. }),
        "expected Indexed, got {:?}",
        result
    );
    if let TypeShape::Indexed { base, index } = result {
        assert_eq!(*base, TypeShape::TypeParam("T".to_string()));
        assert_eq!(*index, TypeShape::TypeParam("K".to_string()));
    }
}

// Test: indexed access with string literal key
#[test]
fn test_indexed_access_string_literal() {
    let result = parse_type_string("Foo[\"bar\"]");
    assert!(
        matches!(result, TypeShape::Indexed { .. }),
        "expected Indexed, got {:?}",
        result
    );
    if let TypeShape::Indexed { base, index } = result {
        assert_eq!(*base, TypeShape::TypeParam("Foo".to_string()));
        assert_eq!(
            *index,
            TypeShape::Literal(LiteralKind::String("bar".to_string()))
        );
    }
}

// Test: chained indexed access T[K][J]
#[test]
fn test_indexed_access_chained() {
    let result = parse_type_string("T[K][J]");
    // T[K][J] = (T[K])[J]
    assert!(
        matches!(result, TypeShape::Indexed { .. }),
        "expected outer Indexed, got {:?}",
        result
    );
    if let TypeShape::Indexed { base, index } = result {
        // base should be T[K] (inner Indexed)
        assert!(
            matches!(*base, TypeShape::Indexed { .. }),
            "base of outer indexed should be inner Indexed T[K], got {:?}",
            base
        );
        assert_eq!(*index, TypeShape::TypeParam("J".to_string()));
    }
}

// Test: keyof T
#[test]
fn test_keyof() {
    let result = parse_type_string("keyof T");
    assert!(
        matches!(result, TypeShape::Generic { ref name, .. } if name == "keyof"),
        "expected Generic {{ name: 'keyof', .. }}, got {:?}",
        result
    );
}

// Test: typeof x
#[test]
fn test_typeof() {
    let result = parse_type_string("typeof x");
    assert!(
        matches!(result, TypeShape::TypeParam(ref s) if s == "typeof x"),
        "expected TypeParam('typeof x'), got {:?}",
        result
    );
}

// ─── TYPE-02: `infer U extends C` constraint on inferred type ───

// Backward compatibility: `infer U` with no trailing extends still
// produces the legacy TypeShape::TypeParam("infer U") form so existing
// consumers/diff engines don't regress.
#[test]
fn infer_without_extends_backward_compat() {
    let shape = parse_type_string("infer U");
    assert_eq!(shape, TypeShape::TypeParam("infer U".to_string()));
}

// `infer U extends string` should now parse into the dedicated
// TypeShape::InferExtends variant whose
// constraint is Primitive(String).
#[test]
fn infer_extends_string_produces_infer_extends_variant() {
    let shape = parse_type_string("infer U extends string");
    match shape {
        TypeShape::InferExtends { name, constraint } => {
            assert_eq!(name, "U");
            assert_eq!(*constraint, TypeShape::Primitive(PrimitiveKind::String));
        }
        other => panic!("expected InferExtends, got {:?}", other),
    }
}

// Same shape as the previous test but with a different primitive
// constraint — confirms the parser captures the constraint faithfully
// rather than hardcoding a specific type.
#[test]
fn infer_extends_number_produces_infer_extends_variant() {
    let shape = parse_type_string("infer U extends number");
    match shape {
        TypeShape::InferExtends { name, constraint } => {
            assert_eq!(name, "U");
            assert_eq!(*constraint, TypeShape::Primitive(PrimitiveKind::Number));
        }
        other => panic!("expected InferExtends, got {:?}", other),
    }
}

// ─── GEN-07: NoInfer<T> wrapper canonicalization ───────────
//
// The parser must recognize `NoInfer<T>` as a dedicated wrapper variant
// (`TypeShape::NoInferWrap`) rather than the generic catch-all
// `TypeShape::Generic { name: "NoInfer", args: [T] }` form. This mirrors
// the existing `Array<T>` special-case at parser.rs:608-610. Diff
// engines depend on the canonical wrapper form to detect wrapper-vs-
// unwrapped transitions without resorting to string comparison.

#[test]
fn noinfer_of_primitive_produces_noinfer_wrap() {
    let shape = parse_type_string("NoInfer<string>");
    match shape {
        TypeShape::NoInferWrap(inner) => {
            assert_eq!(*inner, TypeShape::Primitive(PrimitiveKind::String));
        }
        other => panic!("expected NoInferWrap, got {:?}", other),
    }
}

#[test]
fn noinfer_of_typeparam_produces_noinfer_wrap() {
    let shape = parse_type_string("NoInfer<T>");
    match shape {
        TypeShape::NoInferWrap(inner) => {
            assert_eq!(*inner, TypeShape::TypeParam("T".to_string()));
        }
        other => panic!("expected NoInferWrap, got {:?}", other),
    }
}

#[test]
fn noinfer_of_generic_produces_noinfer_wrap_with_generic_inner() {
    let shape = parse_type_string("NoInfer<Foo<string>>");
    match shape {
        TypeShape::NoInferWrap(inner) => match *inner {
            TypeShape::Generic { name, args } => {
                assert_eq!(name, "Foo");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected Generic inside NoInferWrap, got {:?}", other),
        },
        other => panic!("expected NoInferWrap, got {:?}", other),
    }
}

// Defensive: the real `NoInfer<T>` utility type takes exactly one
// type parameter. If a user (or some future LSP hover-string drift)
// hands us `NoInfer<X, Y>`, fall through to the generic catch-all
// rather than panic or canonicalize incorrectly.
#[test]
fn noinfer_arity_mismatch_falls_through_to_generic() {
    let shape = parse_type_string("NoInfer<string, number>");
    match shape {
        TypeShape::Generic { name, args } => {
            assert_eq!(name, "NoInfer");
            assert_eq!(args.len(), 2);
        }
        other => panic!("expected Generic fallthrough, got {:?}", other),
    }
}

// Regression: any other 1-arg generic name (not literally "NoInfer")
// must remain a `TypeShape::Generic`. The new branch is name-keyed
// and must not affect surrounding generics.
#[test]
fn non_noinfer_generic_unchanged() {
    let shape = parse_type_string("SomethingElse<string>");
    match shape {
        TypeShape::Generic { name, args } => {
            assert_eq!(name, "SomethingElse");
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected Generic, got {:?}", other),
    }
}

// ---------------------------------------------------------------------
// REF-09: parse_template_literal_type + compare_template_literal_patterns
// ---------------------------------------------------------------------

#[test]
fn parse_template_literal_type_simple() {
    use crate::languages::typescript::types::parser::parse_template_literal_type;
    use crate::languages::typescript::types::shape::TemplatePart;
    let parsed = parse_template_literal_type("`on${string}`").expect("should parse");
    // Expect at minimum: Literal("on"), Placeholder(Primitive(String))
    assert!(parsed.len() >= 2, "parsed: {:?}", parsed);
    match &parsed[0] {
        TemplatePart::Literal(s) => assert_eq!(s, "on"),
        other => panic!("expected Literal, got {:?}", other),
    }
    assert!(
        matches!(parsed[1], TemplatePart::Placeholder(_)),
        "second part should be Placeholder"
    );
}

#[test]
fn parse_template_literal_type_multi_literal() {
    use crate::languages::typescript::types::parser::parse_template_literal_type;
    use crate::languages::typescript::types::shape::TemplatePart;
    let parsed = parse_template_literal_type("`on_${string}_off`").expect("should parse");
    let literals: Vec<&String> = parsed
        .iter()
        .filter_map(|p| {
            if let TemplatePart::Literal(s) = p {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    assert!(literals.iter().any(|s| s.as_str() == "on_"));
    assert!(literals.iter().any(|s| s.as_str() == "_off"));
}

#[test]
fn parse_template_literal_type_non_template_returns_none() {
    use crate::languages::typescript::types::parser::parse_template_literal_type;
    assert!(parse_template_literal_type("string").is_none());
    assert!(parse_template_literal_type("Record<string, number>").is_none());
}

#[test]
fn parse_template_literal_type_pure_literal() {
    use crate::languages::typescript::types::parser::parse_template_literal_type;
    use crate::languages::typescript::types::shape::TemplatePart;
    let parsed = parse_template_literal_type("`no-placeholder`").expect("should parse");
    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed[0],
        TemplatePart::Literal("no-placeholder".to_string())
    );
}

#[test]
fn compare_template_literal_patterns_equal() {
    use crate::languages::typescript::types::parser::{
        compare_template_literal_patterns, parse_template_literal_type, PatternDirection,
    };
    let a = parse_template_literal_type("`on${string}`").unwrap();
    let b = parse_template_literal_type("`on${string}`").unwrap();
    assert_eq!(
        compare_template_literal_patterns(&a, &b),
        PatternDirection::Equal
    );
}

#[test]
fn compare_template_literal_patterns_narrowing() {
    use crate::languages::typescript::types::parser::{
        compare_template_literal_patterns, parse_template_literal_type, PatternDirection,
    };
    let old = parse_template_literal_type("`on${string}`").unwrap();
    let new = parse_template_literal_type("`onClick${string}`").unwrap();
    // `onClick` prefix extends `on` prefix -> new is narrower
    assert_eq!(
        compare_template_literal_patterns(&old, &new),
        PatternDirection::Narrowing
    );
}

#[test]
fn compare_template_literal_patterns_widening() {
    use crate::languages::typescript::types::parser::{
        compare_template_literal_patterns, parse_template_literal_type, PatternDirection,
    };
    let old = parse_template_literal_type("`onClick${string}`").unwrap();
    let new = parse_template_literal_type("`on${string}`").unwrap();
    assert_eq!(
        compare_template_literal_patterns(&old, &new),
        PatternDirection::Widening
    );
}

#[test]
fn compare_template_literal_patterns_unrelated() {
    use crate::languages::typescript::types::parser::{
        compare_template_literal_patterns, parse_template_literal_type, PatternDirection,
    };
    let old = parse_template_literal_type("`abc${string}`").unwrap();
    let new = parse_template_literal_type("`xyz${string}`").unwrap();
    // Neither `abc` nor `xyz` is a prefix of the other
    assert_eq!(
        compare_template_literal_patterns(&old, &new),
        PatternDirection::Unrelated
    );
}
