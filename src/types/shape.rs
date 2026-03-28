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
    pub shape: TypeShape,
}

/// A parameter in a function type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamShape {
    pub name: std::string::String,
    pub optional: bool,
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
    /// Tuple types: `[string, number]`
    Tuple(Vec<TypeShape>),
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
                                shape: v.shape.normalize(),
                            },
                        )
                    })
                    .collect();
                TypeShape::Object(normalized)
            }
            TypeShape::Array(inner) => TypeShape::Array(Box::new(inner.normalize())),
            TypeShape::Tuple(elements) => {
                TypeShape::Tuple(elements.into_iter().map(|e| e.normalize()).collect())
            }
            TypeShape::Generic { name, args } => TypeShape::Generic {
                name,
                args: args.into_iter().map(|a| a.normalize()).collect(),
            },
            TypeShape::Function { params, return_type } => TypeShape::Function {
                params: params
                    .into_iter()
                    .map(|p| ParamShape {
                        name: p.name,
                        optional: p.optional,
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
            type_shape: None,
        };
        assert!(sym.type_signature.is_none());
        assert!(sym.type_shape.is_none());
    }

    #[test]
    fn test_field_shape_optional() {
        let field = FieldShape {
            optional: true,
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
            TypeShape::Primitive(PrimitiveKind::String),
            TypeShape::Primitive(PrimitiveKind::Number),
        ]);
        assert!(matches!(shape, TypeShape::Tuple(_)));
    }
}
