use crate::languages::typescript::types::shape as type_shapes;
use crate::languages::typescript::types::shape::{
    LiteralKind, ParamShape, TypePredicateShape, TypeShape,
};

const MAX_DEPTH: usize = 20;

/// Determine whether `sub` is a structural subtype of `sup` under TypeScript
/// variance semantics.
///
/// Returns `false` for incomparable types (Opaque, Conditional, Mapped,
/// Indexed, Template) (assume-breaking conservatism).
///
/// Recursion is capped at `MAX_DEPTH = 20` to match type comparison.
pub fn is_subtype_of(sub: &TypeShape, sup: &TypeShape, depth: usize) -> bool {
    use crate::languages::typescript::types::shape::PrimitiveKind::*;
    use crate::languages::typescript::types::shape::TypeShape::*;

    // Depth cap: assume not a subtype (conservative)
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

    // NoInfer controls candidate collection during inference, not assignability.
    // Inference changes are reported separately from parameter narrowing.
    match (sub, sup) {
        (NoInferWrap(inner), _) => return is_subtype_of(inner, sup, depth + 1),
        (_, NoInferWrap(inner)) => return is_subtype_of(sub, inner, depth + 1),
        (
            GenericDeclaration {
                params: offered,
                body: offered_body,
            },
            GenericDeclaration {
                params: required,
                body: required_body,
            },
        ) => {
            if offered.len() < required.len()
                || offered[required.len()..]
                    .iter()
                    .any(|parameter| parameter.default.is_none())
            {
                return false;
            }
            let unknown = Primitive(Unknown);
            let accepts = offered.iter().zip(required).all(|(offered, required)| {
                is_subtype_of(
                    required.constraint.as_ref().unwrap_or(&unknown),
                    offered.constraint.as_ref().unwrap_or(&unknown),
                    depth + 1,
                ) && (required.default.is_none() || required.default == offered.default)
                    && required.is_const == offered.is_const
                    && required.variance == offered.variance
            });
            return accepts && is_subtype_of(offered_body, required_body, depth + 1);
        }
        (_, Generic { name, args }) if name == "Readonly" && args.len() == 1 => {
            let sub = match sub {
                Generic { name, args } if name == "Readonly" && args.len() == 1 => &args[0],
                other => other,
            };
            return is_subtype_of(sub, &args[0], depth + 1);
        }
        (_, Generic { name, args }) if name == "ReadonlyArray" && args.len() == 1 => {
            let sub = match sub {
                Generic { name, args } if name == "Readonly" && args.len() == 1 => &args[0],
                other => other,
            };
            match sub {
                Array(element) => return is_subtype_of(element, &args[0], depth + 1),
                Tuple(_) => {
                    return is_subtype_of(sub, &Array(Box::new(args[0].clone())), depth + 1)
                }
                _ => {}
            }
        }
        _ => {}
    }

    // Literal subtype of corresponding primitive
    match (sub, sup) {
        (Generic { name, .. }, Primitive(Symbol)) if name == "Symbol.for" || name == "Symbol" => {
            return true;
        }
        (Literal(LiteralKind::String(_)), Primitive(String)) => return true,
        (Literal(LiteralKind::Number(number)), Primitive(Number)) if !number.ends_with('n') => {
            return true
        }
        (Literal(LiteralKind::Number(number)), Primitive(Bigint)) if number.ends_with('n') => {
            return true
        }
        (Literal(LiteralKind::Boolean(_)), Primitive(Boolean)) => return true,
        (Literal(LiteralKind::Null), Primitive(Null)) => return true,
        _ => {}
    }

    // incomparable types — conservatively assume not a subtype
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
                        if !sup_field.readonly
                            && (sub_field.write_type.is_some() || sup_field.write_type.is_some())
                        {
                            let accepts = sub_field.write_type.as_ref().unwrap_or(&sub_field.shape);
                            let required =
                                sup_field.write_type.as_ref().unwrap_or(&sup_field.shape);
                            if sub_field.readonly || !is_subtype_of(required, accepts, depth + 1) {
                                return false;
                            }
                        }
                    }
                }
            }
            true
        }

        // Union sub: every member of sub must be a subtype of sup
        // e.g. (string | number) <: (string | number | boolean) iff
        // string <: (string | number | boolean) AND number <: (string | number | boolean)
        (Union(members), _) => members.iter().all(|m| is_subtype_of(m, sup, depth + 1)),

        // Union sup: sub must be a subtype of at least one member
        // e.g. string <: (string | number) iff string <: string OR string <: number
        (_, Union(members)) => members.iter().any(|m| is_subtype_of(sub, m, depth + 1)),

        // Intersection sup: sub must satisfy ALL members of the intersection
        // Check this first so separate offered members can satisfy each requirement.
        (_, Intersection(members)) => members.iter().all(|m| is_subtype_of(sub, m, depth + 1)),

        // An intersection is narrower than each of its individual members.
        (Intersection(members), _) => members.iter().any(|m| is_subtype_of(m, sup, depth + 1)),

        // Array: covariant in element type
        (Array(sub_inner), Array(sup_inner)) => is_subtype_of(sub_inner, sup_inner, depth + 1),
        (Tuple(elements), Array(element)) => elements.iter().all(|entry| {
            let shape = if entry.rest {
                match &entry.shape {
                    Array(inner) => inner.as_ref(),
                    other => other,
                }
            } else {
                &entry.shape
            };
            is_subtype_of(shape, element, depth + 1)
                && (!entry.optional || is_subtype_of(&Primitive(Undefined), element, depth + 1))
        }),

        // Labels have no effect on assignability; optional/rest markers do.
        (Tuple(sub_elems), Tuple(sup_elems)) => tuple_is_subtype(sub_elems, sup_elems, depth),

        // Function: contravariant params, covariant return
        // TypeScript callback compatibility: a function with fewer params is a subtype.
        // (You can always pass a callback that ignores extra arguments.)
        (
            Function {
                params: sub_params,
                return_type: sub_ret,
                contract: sub_contract,
            },
            Function {
                params: sup_params,
                return_type: sup_ret,
                contract: sup_contract,
            },
        ) => {
            if !accepts_receiver(
                sub_contract.receiver.as_deref(),
                sup_contract.receiver.as_deref(),
                depth + 1,
            ) || !preserves_predicate(
                sub_contract.predicate.as_ref(),
                sup_contract.predicate.as_ref(),
                depth + 1,
            ) {
                return false;
            }
            // sub may have fewer params than sup (TS callback pattern)
            // sub may NOT have more required params than sup has total
            let sub_required = sub_params.iter().filter(|p| !p.optional && !p.rest).count();
            let sup_required = sup_params.iter().filter(|p| !p.optional && !p.rest).count();
            if sub_required > sup_required {
                return false;
            }

            // For each position that both share, params are CONTRAVARIANT:
            // sup's param type must be a subtype of sub's param type
            // (the caller passes sup's type; sub must accept anything sup accepts)
            fn at(params: &[ParamShape], index: usize) -> Option<&TypeShape> {
                let parameter = params
                    .get(index)
                    .or_else(|| params.last().filter(|parameter| parameter.rest))?;
                if parameter.rest {
                    if let Array(element) = &parameter.shape {
                        return Some(element);
                    }
                }
                Some(&parameter.shape)
            }
            for i in 0..sub_params.len().max(sup_params.len()) {
                let (Some(offered), Some(required)) = (at(sub_params, i), at(sup_params, i)) else {
                    continue;
                };
                // contravariant: sup param must accept what sub param accepts
                if !is_subtype_of(required, offered, depth + 1) {
                    return false;
                }
            }

            // Return type is COVARIANT: sub's return must be a subtype of sup's return
            is_subtype_of(sub_ret, sup_ret, depth + 1)
        }

        // Library iterator types accept next inputs and produce yield/return outputs.
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
                && sub_args.iter().zip(sup_args.iter()).enumerate().all(
                    |(index, (sub_argument, sup_argument))| {
                        let next_input = index == 2
                            && matches!(
                                sub_name.as_str(),
                                "Generator"
                                    | "AsyncGenerator"
                                    | "Iterator"
                                    | "AsyncIterator"
                                    | "Iterable"
                                    | "AsyncIterable"
                                    | "IterableIterator"
                                    | "AsyncIterableIterator"
                            );
                        if next_input {
                            is_subtype_of(sup_argument, sub_argument, depth + 1)
                        } else {
                            is_subtype_of(sub_argument, sup_argument, depth + 1)
                        }
                    },
                )
        }

        // TypeParam: name equality only (no constraint information available)
        (TypeParam(sub_name), TypeParam(sup_name)) => sub_name == sup_name,

        // Catch-all: different variants not handled above — assume not a subtype
        _ => false,
    }
}

pub(crate) fn accepts_receiver(
    offered: Option<&TypeShape>,
    required: Option<&TypeShape>,
    depth: usize,
) -> bool {
    let Some(offered) = offered else {
        return true;
    };
    let unconstrained = TypeShape::Primitive(type_shapes::PrimitiveKind::Unknown);
    is_subtype_of(required.unwrap_or(&unconstrained), offered, depth)
}

pub(crate) fn preserves_predicate(
    offered: Option<&TypePredicateShape>,
    required: Option<&TypePredicateShape>,
    depth: usize,
) -> bool {
    let Some(required) = required else {
        return true;
    };
    let Some(offered) = offered else {
        return false;
    };
    if offered.target != required.target || offered.asserts != required.asserts {
        return false;
    }
    match (&offered.shape, &required.shape) {
        (Some(offered), Some(required_shape)) => {
            is_subtype_of(offered, required_shape, depth)
                // A boolean predicate also narrows the false branch. Assertions
                // only promise the type reached after a successful return.
                && (required.asserts || is_subtype_of(required_shape, offered, depth))
        }
        (None, None) => true,
        _ => false,
    }
}

/// Produce a human-readable string representation of a TypeShape.
///
/// Used by: finding messages include old/new type values.
fn tuple_is_subtype(
    sub: &[type_shapes::TupleElement],
    sup: &[type_shapes::TupleElement],
    depth: usize,
) -> bool {
    // Unresolved variadic generics and middle rest elements cannot be expanded
    // safely here. Equality remains safe; other cases are conservative.
    if sub == sup {
        return true;
    }
    let split = |elems: &[type_shapes::TupleElement]| {
        elems.iter().position(|e| e.rest).unwrap_or(elems.len())
    };
    let sub_fixed = split(sub);
    let sup_fixed = split(sup);
    if sub_fixed + 1 < sub.len() || sup_fixed + 1 < sup.len() {
        return false;
    }
    let min_len = |elems: &[type_shapes::TupleElement]| {
        elems.iter().filter(|e| !e.optional && !e.rest).count()
    };
    if min_len(sub) < min_len(sup) {
        return false;
    }
    if sup_fixed == sup.len() && (sub_fixed != sub.len() || sub.len() > sup.len()) {
        return false;
    }
    fn at(
        elems: &[type_shapes::TupleElement],
        fixed: usize,
        index: usize,
    ) -> Option<(&TypeShape, bool)> {
        if index < fixed {
            return Some((&elems[index].shape, elems[index].optional));
        }
        match elems.get(fixed).map(|e| &e.shape) {
            Some(TypeShape::Array(inner)) => Some((inner, false)),
            _ => None,
        }
    }
    for index in 0..sub_fixed.max(sup_fixed) + 1 {
        let Some((s, optional)) = at(sub, sub_fixed, index) else {
            if index >= sub_fixed && sub_fixed != sub.len() {
                return false;
            }
            continue;
        };
        let Some((p, accepts_optional)) = at(sup, sup_fixed, index) else {
            return false;
        };
        if (optional && !accepts_optional) || !is_subtype_of(s, p, depth + 1) {
            return false;
        }
    }
    true
}

/// Example: `format_type(&Union([String, Number]))` returns `"string | number"`.
pub fn format_type(shape: &TypeShape) -> String {
    use crate::languages::typescript::types::shape::PrimitiveKind::*;
    use crate::languages::typescript::types::shape::TypeShape::*;

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
            elems
                .iter()
                .map(|e| {
                    let rest_prefix = if e.rest { "..." } else { "" };
                    let opt_suffix = if e.optional { "?" } else { "" };
                    match &e.label {
                        Some(lbl) => {
                            format!("{rest_prefix}{lbl}{opt_suffix}: {}", format_type(&e.shape))
                        }
                        None => format!("{rest_prefix}{}{opt_suffix}", format_type(&e.shape)),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Object(_) => "{ ... }".to_string(),
        Function {
            params,
            return_type,
            ..
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
        GenericDeclaration { params, body } => {
            let parameters = params
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    let constraint = parameter
                        .constraint
                        .as_ref()
                        .map(|shape| format!(" extends {}", format_type(shape)))
                        .unwrap_or_default();
                    let default = parameter
                        .default
                        .as_ref()
                        .map(|shape| format!(" = {}", format_type(shape)))
                        .unwrap_or_default();
                    format!("T{index}{constraint}{default}")
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("<{parameters}> {}", format_type(body))
        }
        // Opaque carries already-human-readable raw text
        Opaque(s) => s.clone(),
        // Fallback for Conditional, Mapped, Indexed, Template
        other => format!("{:?}", other),
    }
}

#[cfg(test)]
mod tests;
