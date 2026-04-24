use std::collections::BTreeMap;

/// All primitive TypeScript types.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrimitiveKind {
    String,
    Number,
    Boolean,
    Null,
    Undefined,
    Void,
    Never,
    Any,
    Unknown,
    Symbol,
    Bigint,
}

/// A literal type value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiteralKind {
    /// `"foo"` — string literal type
    String(std::string::String),
    /// `42` — number literal type (stored as string to avoid float issues)
    Number(std::string::String),
    /// `true` or `false`
    Boolean(bool),
    /// `null`
    Null,
}

/// Modifiers on mapped type properties.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MappedModifier {
    Readonly,
    Optional,
    ReadonlyOptional,
}

/// A component of a template literal type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
    /// A literal string segment.
    Literal(std::string::String),
    /// A type placeholder `${T}`.
    Placeholder(Box<TypeShape>),
}

/// A field in an object type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldShape {
    pub optional: bool,
    pub readonly: bool, // IFT-02 — readonly modifier on type alias object field
    pub shape: TypeShape,
}

/// REF-10 (Phase 22): per-position metadata for labeled tuple elements.
///
/// Unlabeled tuples have `label: None` at every position. Labels are nominal
/// metadata — they participate in the diff (`tuple-label-renamed`,
/// `tuple-label-reordered`, `tuple-label-dropped`, `tuple-label-added`) but
/// are IGNORED by structural subtyping. `optional` and `rest` map to the
/// per-position `?:` and `...` markers respectively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TupleElement {
    /// Label text (e.g., "x" in `[x: number]`). `None` for unlabeled elements.
    pub label: Option<std::string::String>,
    /// `[x?: T]` — per-position optionality. Independent of label presence.
    pub optional: bool,
    /// `[...xs: T[]]` — variadic/rest element; always the tail position.
    pub rest: bool,
    /// The element's type.
    pub shape: TypeShape,
}

/// A parameter in a function type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamShape {
    pub name: std::string::String,
    pub optional: bool,
    pub rest: bool, // true if this is a ...rest parameter, per REF-02
    pub shape: TypeShape,
}

/// Structural representation of a TypeScript type.
///
/// This enum covers all major TypeScript type constructs and is used as the
/// canonical form for structural comparison between package versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeShape {
    /// Primitive types: `string`, `number`, `boolean`, `void`, etc.
    Primitive(PrimitiveKind),
    /// Literal types: `"foo"`, `42`, `true`, `null`
    Literal(LiteralKind),
    /// Object types: `{ x: number; y?: string }`
    Object(BTreeMap<std::string::String, FieldShape>),
    /// Array types: `string[]` or `Array<string>`
    Array(Box<TypeShape>),
    /// Tuple types: `[string, number]` or labeled `[x: number, y?: string, ...rest: T[]]`.
    ///
    /// REF-10 (Phase 22): labeled-tuple support. Each element is a `TupleElement`
    /// carrying `label`, `optional`, `rest`, and `shape`. Unlabeled tuples have
    /// `label: None` at every position; `optional` and `rest` map to the
    /// per-position `?:` and `...` markers. Labels are nominal metadata — they
    /// participate in the diff (`tuple-label-renamed` etc.) but are IGNORED by
    /// structural subtyping.
    Tuple(Vec<TupleElement>),
    /// Union types: `string | number` — normalized (sorted) by canonical string
    Union(Vec<TypeShape>),
    /// Intersection types: `A & B` — normalized (sorted) by canonical string
    Intersection(Vec<TypeShape>),
    /// Generic types: `Map<string, number>`, `Promise<void>`
    Generic {
        name: std::string::String,
        args: Vec<TypeShape>,
    },
    /// Type parameter reference: `T`, `K`, `V`
    TypeParam(std::string::String),
    /// Function types: `(x: string, y: number) => boolean`
    Function {
        params: Vec<ParamShape>,
        return_type: Box<TypeShape>,
    },
    /// Conditional types: `T extends U ? Then : Else`
    Conditional {
        check: Box<TypeShape>,
        extends: Box<TypeShape>,
        then_type: Box<TypeShape>,
        else_type: Box<TypeShape>,
    },
    /// Mapped types: `{ [K in keyof T]: V }`
    Mapped {
        key_type: Box<TypeShape>,
        value_type: Box<TypeShape>,
        modifier: Option<MappedModifier>,
    },
    /// Indexed access types: `T[K]`
    Indexed {
        base: Box<TypeShape>,
        index: Box<TypeShape>,
    },
    /// Template literal types: `` `prefix_${T}` ``
    Template(Vec<TemplatePart>),
    /// TYPE-01 (Phase 20): value-level `X satisfies T`. `constraint` is the
    /// RHS `T`; `narrowed` is the LSP-derived narrowed type of the value
    /// (`None` in degraded mode — carries
    /// `limitation: satisfies-narrowed-type-not-verified` in its finding).
    Satisfies {
        constraint: Box<TypeShape>,
        narrowed: Option<Box<TypeShape>>,
    },
    /// TYPE-02 (Phase 20): `infer U extends C` — structured so the
    /// constraint is a diff-visible child. Replaces the pre-Phase-20 path
    /// that flattened `infer U` into `TypeShape::TypeParam("infer U")`
    /// and dropped the `extends` clause. Produced by Plan 20-02's parser
    /// extension; diff-consumed by Plan 20-02's `type_diff.rs` branch.
    InferExtends {
        name: std::string::String,
        constraint: Box<TypeShape>,
    },
    /// GEN-07 (Phase 20): `NoInfer<T>` wrapper — canonical form so diff
    /// engines observe wrapper-vs-unwrapped at a position without string
    /// parsing. Produced by Plan 20-03's `parse_generic` extension.
    NoInferWrap(Box<TypeShape>),
    /// Fallback for unparseable types — carries raw text.
    /// Comparison treats two Opaque values as incomparable unless identical.
    Opaque(std::string::String),
}

impl TypeShape {
    /// Normalize a TypeShape by sorting Union and Intersection members
    /// into a canonical order (by Debug representation). Recurses into
    /// nested shapes. BTreeMap-based Object is already sorted.
    pub fn normalize(self) -> Self {
        match self {
            TypeShape::Union(mut members) => {
                let members: Vec<TypeShape> = members.drain(..).map(|m| m.normalize()).collect();
                let mut sorted = members;
                sorted.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
                TypeShape::Union(sorted)
            }
            TypeShape::Intersection(mut members) => {
                let members: Vec<TypeShape> = members.drain(..).map(|m| m.normalize()).collect();
                let mut sorted = members;
                sorted.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
                TypeShape::Intersection(sorted)
            }
            TypeShape::Object(fields) => {
                let normalized = fields
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            FieldShape {
                                optional: v.optional,
                                readonly: v.readonly,
                                shape: v.shape.normalize(),
                            },
                        )
                    })
                    .collect();
                TypeShape::Object(normalized)
            }
            TypeShape::Array(inner) => TypeShape::Array(Box::new(inner.normalize())),
            TypeShape::Tuple(elements) => TypeShape::Tuple(
                elements
                    .into_iter()
                    .map(|e| TupleElement {
                        label: e.label,
                        optional: e.optional,
                        rest: e.rest,
                        shape: e.shape.normalize(),
                    })
                    .collect(),
            ),
            TypeShape::Generic { name, args } => TypeShape::Generic {
                name,
                args: args.into_iter().map(|a| a.normalize()).collect(),
            },
            TypeShape::Function {
                params,
                return_type,
            } => TypeShape::Function {
                params: params
                    .into_iter()
                    .map(|p| ParamShape {
                        name: p.name,
                        optional: p.optional,
                        rest: p.rest,
                        shape: p.shape.normalize(),
                    })
                    .collect(),
                return_type: Box::new(return_type.normalize()),
            },
            TypeShape::Conditional {
                check,
                extends,
                then_type,
                else_type,
            } => TypeShape::Conditional {
                check: Box::new(check.normalize()),
                extends: Box::new(extends.normalize()),
                then_type: Box::new(then_type.normalize()),
                else_type: Box::new(else_type.normalize()),
            },
            TypeShape::Mapped {
                key_type,
                value_type,
                modifier,
            } => TypeShape::Mapped {
                key_type: Box::new(key_type.normalize()),
                value_type: Box::new(value_type.normalize()),
                modifier,
            },
            TypeShape::Indexed { base, index } => TypeShape::Indexed {
                base: Box::new(base.normalize()),
                index: Box::new(index.normalize()),
            },
            TypeShape::Satisfies { constraint, narrowed } => TypeShape::Satisfies {
                constraint: Box::new(constraint.normalize()),
                narrowed: narrowed.map(|n| Box::new(n.normalize())),
            },
            TypeShape::InferExtends { name, constraint } => TypeShape::InferExtends {
                name,
                constraint: Box::new(constraint.normalize()),
            },
            TypeShape::NoInferWrap(inner) => TypeShape::NoInferWrap(Box::new(inner.normalize())),
            // Primitive, Literal, TypeParam, Template, Opaque — already canonical
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_string_can_be_constructed() {
        let shape = TypeShape::Primitive(PrimitiveKind::String);
        assert_eq!(format!("{:?}", shape), "Primitive(String)");
    }

    #[test]
    fn test_object_with_btreemap_fields() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "x".to_string(),
            FieldShape {
                optional: false,
                readonly: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            },
        );
        let shape = TypeShape::Object(fields);
        assert!(matches!(shape, TypeShape::Object(_)));
    }

    #[test]
    fn test_union_normalizes_sorted() {
        // "number | string" and "string | number" should normalize to the same order
        let union1 = TypeShape::Union(vec![
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
        ])
        .normalize();
        let union2 = TypeShape::Union(vec![
            TypeShape::Primitive(PrimitiveKind::Number),
            TypeShape::Primitive(PrimitiveKind::String),
        ])
        .normalize();
        assert_eq!(union1, union2);
    }

    #[test]
    fn test_function_with_params_and_return() {
        let shape = TypeShape::Function {
            params: vec![ParamShape {
                name: "x".to_string(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Boolean)),
        };
        assert!(matches!(shape, TypeShape::Function { .. }));
    }

    #[test]
    fn test_opaque_fallback() {
        let shape = TypeShape::Opaque("complex".to_string());
        if let TypeShape::Opaque(s) = shape {
            assert_eq!(s, "complex");
        } else {
            panic!("expected Opaque");
        }
    }

    #[test]
    fn test_export_symbol_backward_compat() {
        use crate::types::{ExportKind, ExportSymbol};
        use std::path::PathBuf;
        let sym = ExportSymbol {
            name: "foo".to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("src/index.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: Vec::new(),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
        };
        assert!(sym.type_signature.is_none());
        assert!(sym.overloads.is_empty());
    }

    #[test]
    fn type_shape_satisfies_normalize_recurses() {
        let shape = TypeShape::Satisfies {
            constraint: Box::new(TypeShape::Union(vec![
                TypeShape::Primitive(PrimitiveKind::String),
                TypeShape::Primitive(PrimitiveKind::Number),
            ])),
            narrowed: None,
        }
        .normalize();
        match shape {
            TypeShape::Satisfies {
                constraint,
                narrowed,
            } => {
                assert!(narrowed.is_none());
                // Inner union must be sorted by normalize()
                if let TypeShape::Union(members) = *constraint {
                    assert_eq!(members.len(), 2);
                } else {
                    panic!("expected Union inside Satisfies.constraint");
                }
            }
            _ => panic!("expected TypeShape::Satisfies"),
        }
    }

    #[test]
    fn type_shape_infer_extends_normalize_stable() {
        let shape = TypeShape::InferExtends {
            name: "U".to_string(),
            constraint: Box::new(TypeShape::Primitive(PrimitiveKind::String)),
        };
        let normalized = shape.clone().normalize();
        assert_eq!(shape, normalized);
    }

    #[test]
    fn type_shape_noinfer_wrap_normalize_stable() {
        let shape = TypeShape::NoInferWrap(Box::new(TypeShape::Primitive(PrimitiveKind::String)));
        let normalized = shape.clone().normalize();
        assert_eq!(shape, normalized);
    }

    #[test]
    fn test_field_shape_optional() {
        let field = FieldShape {
            optional: true,
            readonly: false,
            shape: TypeShape::Primitive(PrimitiveKind::Number),
        };
        assert!(field.optional);
    }

    #[test]
    fn test_intersection_normalizes_sorted() {
        let inter1 = TypeShape::Intersection(vec![
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
        ])
        .normalize();
        let inter2 = TypeShape::Intersection(vec![
            TypeShape::Primitive(PrimitiveKind::Number),
            TypeShape::Primitive(PrimitiveKind::String),
        ])
        .normalize();
        assert_eq!(inter1, inter2);
    }

    #[test]
    fn test_opaque_carries_raw_string() {
        let raw = "some complex type<>{}[]".to_string();
        let shape = TypeShape::Opaque(raw.clone());
        if let TypeShape::Opaque(s) = shape {
            assert_eq!(s, raw);
        } else {
            panic!("expected Opaque");
        }
    }

    #[test]
    fn test_array_shape() {
        let shape = TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::String)));
        assert!(matches!(shape, TypeShape::Array(_)));
    }

    #[test]
    fn test_tuple_shape() {
        let shape = TypeShape::Tuple(vec![
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
        ]);
        assert!(matches!(shape, TypeShape::Tuple(_)));
    }

    /// REF-10 (Phase 22): verify that `normalize()` on a labeled tuple
    /// preserves label/optional/rest metadata and recurses into inner shapes.
    #[test]
    fn test_tuple_labeled_normalize_preserves_metadata() {
        let shape = TypeShape::Tuple(vec![
            TupleElement {
                label: Some("x".to_string()),
                optional: false,
                rest: false,
                shape: TypeShape::Union(vec![
                    TypeShape::Primitive(PrimitiveKind::Number),
                    TypeShape::Primitive(PrimitiveKind::String),
                ]),
            },
            TupleElement {
                label: Some("rest".to_string()),
                optional: false,
                rest: true,
                shape: TypeShape::Array(Box::new(TypeShape::Primitive(PrimitiveKind::String))),
            },
        ])
        .normalize();

        if let TypeShape::Tuple(elems) = shape {
            assert_eq!(elems.len(), 2);
            assert_eq!(elems[0].label.as_deref(), Some("x"));
            assert!(!elems[0].optional);
            assert!(!elems[0].rest);
            // Inner Union should be normalized (sorted). Original order
            // [Number, String] and normalized order is deterministic.
            assert!(matches!(elems[0].shape, TypeShape::Union(_)));
            assert_eq!(elems[1].label.as_deref(), Some("rest"));
            assert!(elems[1].rest);
            assert!(matches!(elems[1].shape, TypeShape::Array(_)));
        } else {
            panic!("expected Tuple after normalize");
        }
    }
}
