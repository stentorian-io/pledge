//! Preserve assignability between public uses of private brands and unique symbols.
//!
//! Compiler declaration IDs are intentionally local to each snapshot. Comparing
//! names or offsets across snapshots would make private renames breaking changes.
use crate::core::findings::{Finding, Severity};
use crate::languages::javascript::model::{ExportMap, ExportSymbol, SymbolSpace};
use crate::languages::typescript::compare::rules::RULE_TYPE_INCOMPATIBLE;
use crate::languages::typescript::model::ReleaseTag;
use crate::languages::typescript::types::shape::{ParamShape, PrimitiveKind, TypeShape};
use crate::languages::typescript::types::subtype::is_subtype_of;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
struct Uses {
    source: bool,
    target: bool,
}

impl Uses {
    const BOTH: Self = Self {
        source: true,
        target: true,
    };
    const OUTPUT: Self = Self {
        source: true,
        target: false,
    };

    fn reverse(self) -> Self {
        Self {
            source: self.target,
            target: self.source,
        }
    }
}

struct Occurrence<'a> {
    old: &'a TypeShape,
    new: &'a TypeShape,
    old_brands: Vec<&'a str>,
    new_options: Vec<(&'a TypeShape, Vec<&'a str>)>,
    symbol: &'a ExportSymbol,
    condition: &'a Option<String>,
    uses: Uses,
}

pub(crate) fn compare_nominal_relations(old: &ExportMap, new: &ExportMap) -> Vec<Finding> {
    let is_public = |symbol: &ExportSymbol| {
        !matches!(
            symbol.release_tag,
            Some(ReleaseTag::Internal | ReleaseTag::Alpha | ReleaseTag::Beta)
        )
    };
    if old.iter().all(|(key, symbol)| {
        new.get(key)
            .is_none_or(|after| symbol.overloads == after.overloads)
    }) {
        return Vec::new();
    }
    let mut exports: Vec<_> = old.iter().filter(|(_, symbol)| is_public(symbol)).collect();
    exports.sort_by_key(|(key, _)| format!("{key:?}"));
    let mut occurrences = Vec::new();
    let mut findings = Vec::new();
    for (key, before) in exports {
        let Some(after) = new.get(key).filter(|symbol| is_public(symbol)) else {
            continue;
        };
        let uses = if key.space == SymbolSpace::Type {
            Uses::BOTH
        } else {
            Uses::OUTPUT
        };
        for (index, old_shape) in before.overloads.iter().enumerate() {
            // Exact correspondence handles unchanged branded overloads. When
            // several structurally identical replacements have different brands,
            // choosing one arbitrarily cannot establish an identity relationship.
            let exact = after.overloads.iter().find(|shape| *shape == old_shape);
            let candidates: Vec<_> = after
                .overloads
                .iter()
                .filter(|shape| is_subtype_of(shape, old_shape, 0))
                .collect();
            if exact.is_none() && candidates.len() > 1 && contains_nominal(old_shape) {
                findings.push(Finding {
                    severity: Severity::Warning,
                    rule: "analysis-incomplete",
                    file: after.file.clone(),
                    line: after.line,
                    col: after.col,
                    message: format!(
                        "nominal identity correspondence between overloads of `{}` is ambiguous",
                        after.name
                    ),
                });
                continue;
            }
            let structural_replacement = (!contains_nominal(old_shape))
                .then(|| {
                    candidates.iter().copied().find(|candidate| {
                        let mut candidate_uses = Vec::new();
                        let mut uncertain = false;
                        collect(
                            old_shape,
                            candidate,
                            after,
                            &key.condition,
                            uses,
                            &mut candidate_uses,
                            &mut uncertain,
                        );
                        !candidate_uses.iter().any(requires_structural_identity)
                    })
                })
                .flatten();
            let new_shape = exact
                .or(structural_replacement)
                .or_else(|| candidates.first().copied())
                .or_else(|| after.overloads.get(index));
            if let Some(new_shape) = new_shape {
                let mut uncertain = false;
                collect(
                    old_shape,
                    new_shape,
                    after,
                    &key.condition,
                    uses,
                    &mut occurrences,
                    &mut uncertain,
                );
                if uncertain {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        rule: "analysis-incomplete",
                        file: after.file.clone(),
                        line: after.line,
                        col: after.col,
                        message: format!(
                            "nominal output union correspondence in `{}` could not be established",
                            after.name
                        ),
                    });
                }
            }
        }
    }
    let mut sources: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, occurrence) in occurrences
        .iter()
        .enumerate()
        .filter(|(_, occurrence)| occurrence.uses.source)
    {
        for brand in &occurrence.old_brands {
            sources.entry(brand).or_default().push(index);
        }
    }
    let mut reported = HashSet::new();
    let mut comparisons = 0;
    for target in occurrences
        .iter()
        .filter(|occurrence| occurrence.uses.target)
    {
        let Some(brand) = target.old_brands.first() else {
            // Structural objects and ordinary symbols can be supplied without
            // obtaining a package-owned identity from another export.
            if requires_structural_identity(target)
                && reported.insert((
                    target.symbol.file.clone(),
                    target.symbol.line,
                    target.symbol.col,
                ))
            {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: target.symbol.file.clone(),
                    line: target.symbol.line,
                    col: target.symbol.col,
                    message: format!(
                        "`{}` now requires a nominal identity where structural values were accepted",
                        target.symbol.name
                    ),
                });
            }
            continue;
        };
        for source in sources
            .get(brand)
            .into_iter()
            .flatten()
            .map(|index| &occurrences[*index])
        {
            if matches!(
                source.new.without_nominal(),
                TypeShape::Primitive(
                    crate::languages::typescript::types::shape::PrimitiveKind::Any
                        | crate::languages::typescript::types::shape::PrimitiveKind::Never
                ) | TypeShape::Opaque(_)
            ) {
                continue;
            }
            if source.condition != target.condition
                || !target
                    .old_brands
                    .iter()
                    .all(|brand| source.old_brands.contains(brand))
            {
                continue;
            }
            comparisons += 1;
            if comparisons > 100_000 {
                findings.push(Finding {
                    severity: Severity::Warning,
                    rule: "analysis-incomplete",
                    file: target.symbol.file.clone(),
                    line: target.symbol.line,
                    col: target.symbol.col,
                    message: "nominal public type comparison budget exhausted".into(),
                });
                return findings;
            }
            if !source
                .new_options
                .iter()
                .all(|(source_shape, source_brands)| {
                    target
                        .new_options
                        .iter()
                        .any(|(target_shape, target_brands)| {
                            target_brands
                                .iter()
                                .all(|brand| source_brands.contains(brand))
                                && is_subtype_of(source_shape, target_shape, 0)
                        })
                })
                && is_subtype_of(source.old, target.old, 0)
                && reported.insert((
                    target.symbol.file.clone(),
                    target.symbol.line,
                    target.symbol.col,
                ))
            {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: target.symbol.file.clone(),
                    line: target.symbol.line,
                    col: target.symbol.col,
                    message: format!(
                        "nominal type required by `{}` no longer accepts values supplied by `{}`",
                        target.symbol.name, source.symbol.name
                    ),
                });
            }
        }
    }
    findings
}

fn contains_nominal(shape: &TypeShape) -> bool {
    !shape.nominal_brands().is_empty() || shape.children().into_iter().any(contains_nominal)
}

fn requires_structural_identity(occurrence: &Occurrence<'_>) -> bool {
    occurrence.uses.target
        && nominal_options(occurrence.old).iter().any(|(old, brands)| {
            brands.is_empty()
                && matches!(
                    old,
                    TypeShape::Object(_) | TypeShape::Primitive(PrimitiveKind::Symbol)
                )
                && !occurrence
                    .new_options
                    .iter()
                    .any(|(new, brands)| brands.is_empty() && is_subtype_of(old, new, 0))
        })
}

/// Retain references into the snapshots while aligning tuple-rest positions.
fn argument_shapes(params: &[ParamShape]) -> (Vec<&TypeShape>, Option<&TypeShape>) {
    let shapes: Vec<_> = params
        .iter()
        .flat_map(|param| match (&param.shape, param.rest) {
            (TypeShape::Tuple(elements), true) => elements
                .iter()
                .map(|element| match (&element.shape, element.rest) {
                    (TypeShape::Array(shape), true) => shape.as_ref(),
                    _ => &element.shape,
                })
                .collect(),
            (TypeShape::Array(shape), true) => vec![shape.as_ref()],
            _ => vec![&param.shape],
        })
        .collect();
    let tail =
        params
            .last()
            .filter(|param| param.rest)
            .and_then(|param| match &param.shape {
                TypeShape::Array(shape) => Some(shape.as_ref()),
                TypeShape::Tuple(elements) => elements
                    .last()
                    .filter(|element| element.rest)
                    .and_then(|element| match &element.shape {
                        TypeShape::Array(shape) => Some(shape.as_ref()),
                        _ => None,
                    }),
                _ => None,
            });
    (shapes, tail)
}

fn nominal_options(shape: &TypeShape) -> Vec<(&TypeShape, Vec<&str>)> {
    match shape {
        TypeShape::Union(members) => members.iter().flat_map(nominal_options).collect(),
        TypeShape::Intersection(members) => vec![(
            shape,
            members.iter().flat_map(TypeShape::nominal_brands).collect(),
        )],
        _ => vec![(shape, shape.nominal_brands())],
    }
}

fn collect<'a>(
    old: &'a TypeShape,
    new: &'a TypeShape,
    symbol: &'a ExportSymbol,
    condition: &'a Option<String>,
    uses: Uses,
    occurrences: &mut Vec<Occurrence<'a>>,
    uncertain: &mut bool,
) {
    let old_brands = old.nominal_brands();
    let new_options = nominal_options(new);
    if !old_brands.is_empty() || new_options.iter().any(|(_, brands)| !brands.is_empty()) {
        occurrences.push(Occurrence {
            old,
            new,
            old_brands,
            new_options,
            symbol,
            condition,
            uses,
        });
    }
    let old = old.without_nominal();
    let new = new.without_nominal();
    if let TypeShape::Union(members) = old {
        if members.iter().any(contains_nominal) {
            if uses.source {
                *uncertain = true;
            } else {
                for old in members {
                    collect(old, new, symbol, condition, uses, occurrences, uncertain);
                }
            }
            return;
        }
    }
    match (old, new) {
        (
            TypeShape::Function {
                params: before,
                return_type: old_return,
                contract: old_contract,
            },
            TypeShape::Function {
                params: after,
                return_type: new_return,
                contract: new_contract,
            },
        ) => {
            let (before, old_tail) = argument_shapes(before);
            let (after, new_tail) = argument_shapes(after);
            for index in 0..before.len().max(after.len()) {
                let old = before.get(index).copied().or(old_tail);
                let new = after.get(index).copied().or(new_tail);
                let (Some(old), Some(new)) = (old, new) else {
                    continue;
                };
                collect(
                    old,
                    new,
                    symbol,
                    condition,
                    uses.reverse(),
                    occurrences,
                    uncertain,
                );
            }
            collect(
                old_return,
                new_return,
                symbol,
                condition,
                uses,
                occurrences,
                uncertain,
            );
            if let (Some(old), Some(new)) = (&old_contract.receiver, &new_contract.receiver) {
                collect(
                    old,
                    new,
                    symbol,
                    condition,
                    uses.reverse(),
                    occurrences,
                    uncertain,
                );
            }
        }
        (TypeShape::Object(before), TypeShape::Object(after)) => {
            for (name, old) in before {
                if let Some(new) = after.get(name) {
                    let field_uses = if old.readonly { uses } else { Uses::BOTH };
                    collect(
                        &old.shape,
                        &new.shape,
                        symbol,
                        condition,
                        field_uses,
                        occurrences,
                        uncertain,
                    );
                    if let (Some(old), Some(new)) = (&old.write_type, &new.write_type) {
                        collect(
                            old,
                            new,
                            symbol,
                            condition,
                            uses.reverse(),
                            occurrences,
                            uncertain,
                        );
                    }
                }
            }
        }
        (TypeShape::Array(old), TypeShape::Array(new)) => collect(
            old,
            new,
            symbol,
            condition,
            Uses::BOTH,
            occurrences,
            uncertain,
        ),
        (TypeShape::Tuple(before), TypeShape::Tuple(after)) => {
            for (old, new) in before.iter().zip(after) {
                collect(
                    &old.shape,
                    &new.shape,
                    symbol,
                    condition,
                    Uses::BOTH,
                    occurrences,
                    uncertain,
                );
            }
        }
        (
            TypeShape::Generic { name, args: before },
            TypeShape::Generic {
                name: after_name,
                args: after,
            },
        ) if name == "Readonly" && after_name == name => {
            if let (Some(old), Some(new)) = (before.first(), after.first()) {
                // The outer array/tuple cannot be written, but objects reached
                // through its elements can still expose mutable contracts.
                for (old, new) in old.children().into_iter().zip(new.children()) {
                    collect(old, new, symbol, condition, uses, occurrences, uncertain);
                }
            }
        }
        (TypeShape::Generic { name, args: before }, TypeShape::Generic { args: after, .. }) => {
            let uses = if matches!(
                name.as_str(),
                "Map" | "Set" | "WeakMap" | "WeakSet" | "Record"
            ) {
                Uses::BOTH
            } else {
                uses
            };
            for (old, new) in before.iter().zip(after) {
                collect(old, new, symbol, condition, uses, occurrences, uncertain);
            }
        }
        _ => {
            for (old, new) in old.children().into_iter().zip(new.children()) {
                collect(old, new, symbol, condition, uses, occurrences, uncertain);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::javascript::model::{DeclarationKind, ExportKey, ExportKind};
    use crate::languages::typescript::types::shape::{LiteralKind, ParamShape, PrimitiveKind};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn brand(identities: &[&str]) -> TypeShape {
        let body = TypeShape::Object(BTreeMap::new());
        if identities.is_empty() {
            return body;
        }
        let mut args: Vec<_> = identities
            .iter()
            .map(|identity| TypeShape::Literal(LiteralKind::String((*identity).into())))
            .collect();
        args.push(body);
        TypeShape::Generic {
            name: "<nominal>".into(),
            args,
        }
    }

    fn contracts(output: &[&str], input: &[&str]) -> ExportMap {
        let mut exports = ExportMap::new();
        for (name, shape) in [
            ("api", TypeShape::function(Vec::new(), brand(output))),
            (
                "consume",
                TypeShape::function(
                    vec![ParamShape {
                        name: "x".into(),
                        optional: false,
                        rest: false,
                        shape: brand(input),
                    }],
                    TypeShape::Primitive(PrimitiveKind::Void),
                ),
            ),
        ] {
            exports.insert(
                ExportKey::new_root_value(name),
                ExportSymbol {
                    name: name.into(),
                    kind: ExportKind::Named,
                    file: PathBuf::from("index.d.ts"),
                    line: if name == "api" { 1 } else { 2 },
                    col: 1,
                    type_signature: None,
                    overloads: vec![shape],
                    decl_kinds: vec![DeclarationKind::Function],
                    type_guard_type: None,
                    is_async: false,
                    is_generator: false,
                    is_reexport: false,
                    is_type_only: false,
                    release_tag: None,
                    satisfies_type: None,
                    compound_members: Vec::new(),
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                },
            );
        }
        exports
    }

    #[test]
    fn split_brands_break_cross_export_calls() {
        let before = contracts(&["old"], &["old"]);
        let after = contracts(&["new-result"], &["new-input"]);
        let findings = compare_nominal_relations(&before, &after);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_TYPE_INCOMPATIBLE);
    }

    #[test]
    fn private_renames_and_inherited_result_brands_preserve_cross_export_calls() {
        let before = contracts(&["old"], &["old"]);
        for after in [
            contracts(&["renamed"], &["renamed"]),
            contracts(&["base", "derived"], &["base"]),
            contracts(&[], &[]),
        ] {
            assert!(compare_nominal_relations(&before, &after).is_empty());
        }
    }

    #[test]
    fn adding_an_input_brand_requires_evidence_from_the_result() {
        let before = contracts(&["old"], &["old"]);
        let after = contracts(&["base"], &["base", "derived"]);
        assert_eq!(compare_nominal_relations(&before, &after).len(), 1);
    }
    #[test]
    fn input_union_order_preserves_each_brand_alternative() {
        let mut before = contracts(&["old-a"], &["old-a"]);
        let mut after = contracts(&["new-a"], &["new-a"]);
        for (exports, first, second) in [
            (&mut before, "old-a", "old-b"),
            (&mut after, "new-b", "new-a"),
        ] {
            let symbol = exports
                .get_mut(&ExportKey::new_root_value("consume"))
                .unwrap();
            let TypeShape::Function { params, .. } = &mut symbol.overloads[0] else {
                panic!()
            };
            params[0].shape = TypeShape::Union(vec![brand(&[first]), brand(&[second])]);
        }
        assert!(compare_nominal_relations(&before, &after).is_empty());
    }

    #[test]
    fn non_public_contracts_do_not_constrain_public_nominal_relationships() {
        let mut before = contracts(&["old-a"], &["old-a"]);
        let mut after = contracts(&["new-a"], &["new-b"]);
        for exports in [&mut before, &mut after] {
            exports
                .get_mut(&ExportKey::new_root_value("consume"))
                .unwrap()
                .release_tag = Some(ReleaseTag::Internal);
        }
        assert!(compare_nominal_relations(&before, &after).is_empty());
    }

    #[test]
    fn changed_nominal_output_unions_retain_uncertainty() {
        let mut before = contracts(&["old-a"], &["old-a"]);
        let mut after = contracts(&["new-a"], &["new-a"]);
        for (exports, first, second) in [
            (&mut before, "old-a", "old-b"),
            (&mut after, "new-a", "new-c"),
        ] {
            let symbol = exports.get_mut(&ExportKey::new_root_value("api")).unwrap();
            let TypeShape::Function { return_type, .. } = &mut symbol.overloads[0] else {
                panic!()
            };
            **return_type = TypeShape::Union(vec![brand(&[first]), brand(&[second])]);
        }
        let findings = compare_nominal_relations(&before, &after);
        assert!(findings
            .iter()
            .all(|finding| finding.severity == Severity::Warning));
        assert!(findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"));
    }
}
