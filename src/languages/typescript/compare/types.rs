// Structural TypeShape diff engine — compares two type shapes and produces findings.
//
// Design decisions:
// -: recursive shape diff
// - Unresolvable changed types produce incomplete analysis.
// -: normalization applied before comparison
// -: all findings use fine-grained rule IDs — no generic "type-changed" rule
// -: finding messages include both old and new type values
// - SIG-01..05: variance-aware function signature analysis

use crate::core::findings::{Finding, Severity};
use crate::languages::javascript::model as js_model;
use crate::languages::typescript::compare::rules::{
    RULE_FIELD_ADDED, RULE_FIELD_REMOVED, RULE_FIELD_TYPE_CHANGED,
    RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED, RULE_INFER_EXTENDS_CONSTRAINT_CHANGED,
    RULE_PARAM_ADDED, RULE_PARAM_MADE_REQUIRED, RULE_PARAM_REMOVED, RULE_PARAM_REORDERED,
    RULE_PARAM_TYPE_NARROWED, RULE_PROPERTY_MADE_REQUIRED, RULE_RETURN_TYPE_WIDENED,
    RULE_TUPLE_LABEL_ADDED, RULE_TUPLE_LABEL_DROPPED, RULE_TUPLE_LABEL_RENAMED,
    RULE_TUPLE_LABEL_REORDERED, RULE_TYPE_ALIAS_READONLY_CHANGED, RULE_TYPE_INCOMPATIBLE,
    RULE_UNION_VARIANT_ADDED,
};
use crate::languages::typescript::compare::{generics as generic_diff, rules as type_rules};
use crate::languages::typescript::types::shape::{FieldShape, ParamShape, TypeShape};
use crate::languages::typescript::types::subtype::{
    accepts_calls, accepts_receiver, argument_parameters, format_type, is_subtype_of,
    preserves_predicate,
};
use crate::languages::typescript::types::{parser as type_parser, shape as type_shapes};
use std::path::Path;

const MAX_DEPTH: usize = 20;

pub(crate) fn exceeds_comparison_depth(shape: &TypeShape, depth: usize) -> bool {
    depth >= MAX_DEPTH
        || shape
            .children()
            .iter()
            .any(|child| exceeds_comparison_depth(child, depth + 1))
}

fn incomplete_comparison(
    old: &TypeShape,
    new: &TypeShape,
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Option<Finding> {
    (old != new && (exceeds_comparison_depth(old, 0) || exceeds_comparison_depth(new, 0))).then(
        || Finding {
            severity: Severity::Warning,
            rule: "analysis-incomplete",
            file: file.to_path_buf(),
            line,
            col,
            message: format!("type comparison truncated at depth {MAX_DEPTH} for `{name}`"),
        },
    )
}

/// SEM-03: Returns true when the shape is `any` (TypeShape::Primitive(PrimitiveKind::Any)).
///
/// Used to detect `any → T` narrowing before calling is_subtype_of, which returns true
/// for (any, X) in both directions and would suppress the narrowing finding.
fn is_any_shape(s: &TypeShape) -> bool {
    matches!(s, TypeShape::Primitive(type_shapes::PrimitiveKind::Any))
}

/// Compare two TypeShapes structurally and produce findings for differences.
///
/// `symbol_name`: the export symbol name (for finding messages)
/// `file`: file path for the finding location
/// `line`, `col`: source position for the finding
///
/// Changed opaque shapes cannot establish compatibility and emit incomplete analysis.
/// recursion depth capped at 20 levels.
pub fn diff_types(
    old: &TypeShape,
    new: &TypeShape,
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    if let Some(finding) = incomplete_comparison(old, new, symbol_name, file, line, col) {
        return vec![finding];
    }
    let mut findings = Vec::new();

    // Changed opaque syntax is insufficient evidence for a breaking verdict.
    if matches!(old, TypeShape::Opaque(_)) || matches!(new, TypeShape::Opaque(_)) {
        if old != new {
            findings.push(Finding {
                severity: Severity::Warning,
                rule: "analysis-incomplete",
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type of `{symbol_name}` could not be compared (unresolvable type)"
                ),
            });
        }
        return findings;
    }

    diff_recursive(old, new, symbol_name, "", file, line, col, 0, &mut findings);
    findings
}

/// Exported values are read by consumers, so their types have the same
/// covariance and writable-property obligations as function results.
pub fn diff_values(
    old: &TypeShape,
    new: &TypeShape,
    name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    if let Some(finding) = incomplete_comparison(old, new, name, file, line, col) {
        return vec![finding];
    }
    if old == new {
        return Vec::new();
    }
    if matches!(old, TypeShape::Opaque(_)) || matches!(new, TypeShape::Opaque(_)) {
        return diff_types(old, new, name, file, line, col);
    }
    let mut findings = Vec::new();
    diff_functions(
        &[],
        old,
        &[],
        new,
        name,
        "",
        file,
        line,
        col,
        0,
        &mut findings,
    );
    for finding in &mut findings {
        if finding.rule == RULE_RETURN_TYPE_WIDENED {
            finding.rule = RULE_TYPE_INCOMPATIBLE;
        }
        finding.message = finding
            .message
            .replace("return type", "value type")
            .replace("returned field", "exported field");
    }
    findings
}

/// BEH-06: Heuristic — is this union of `TypeShape`s a
/// discriminated (tagged) union? Returns the discriminant property name
/// if so, `None` otherwise.
///
/// Discriminant heuristic:
///  "union of object types where >=1 property has literal type AND that
/// property name is present in every variant".
///
/// Algorithm:
/// 1. Require every member to be `TypeShape::Object`. Non-object unions
///    (primitives, literals alone, generics) are plain unions.
/// 2. From the first member, collect every property name whose value is
///    a `TypeShape::Literal(...)` — these are candidate discriminants.
/// 3. Walk the candidates in deterministic BTreeMap order; the first one
///    that appears as a literal-typed field in *every* other member wins
///    (the canonical TS tagged-union shape).
/// 4. If no candidate satisfies every-variant presence, return None.
pub(crate) fn is_discriminated_union(members: &[TypeShape]) -> Option<String> {
    // All members must be Object. `collect::<Option<Vec<_>>>()` short-
    // circuits to None on the first non-Object member.
    let all_objects: Vec<&std::collections::BTreeMap<String, FieldShape>> = members
        .iter()
        .map(|m| match m {
            TypeShape::Object(fields) => Some(fields),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if all_objects.is_empty() {
        return None;
    }
    // Candidate discriminant properties from the first member: names
    // whose field.shape is `TypeShape::Literal`. BTreeMap iteration is
    // in sorted order, so candidate selection is deterministic.
    let candidate_props: Vec<&String> = all_objects[0]
        .iter()
        .filter(|(_, f)| matches!(f.shape, TypeShape::Literal(_)))
        .map(|(k, _)| k)
        .collect();
    for cand in candidate_props {
        let present_everywhere = all_objects.iter().all(|obj| {
            obj.get(cand)
                .map(|f| matches!(f.shape, TypeShape::Literal(_)))
                .unwrap_or(false)
        });
        if present_everywhere {
            return Some(cand.clone());
        }
    }
    None
}

/// BEH-06: Given two discriminated unions confirmed to share
/// the same discriminant property name and to be in the widening
/// direction (old \subseteq new), return the literal discriminant values
/// present in `new` but absent from `old`. Each returned string is a
/// human-readable rendering of the literal (e.g. `` `"circle"` ``, `42`,
/// `true`) produced by `format_type`.
pub(crate) fn added_discriminant_values(
    discriminant: &str,
    old_members: &[TypeShape],
    new_members: &[TypeShape],
) -> Vec<String> {
    fn disc_lit(member: &TypeShape, disc: &str) -> Option<String> {
        if let TypeShape::Object(fields) = member {
            if let Some(f) = fields.get(disc) {
                if matches!(f.shape, TypeShape::Literal(_)) {
                    return Some(format_type(&f.shape));
                }
            }
        }
        None
    }
    let old_values: std::collections::HashSet<String> = old_members
        .iter()
        .filter_map(|m| disc_lit(m, discriminant))
        .collect();
    new_members
        .iter()
        .filter_map(|m| disc_lit(m, discriminant))
        .filter(|v| !old_values.contains(v))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn diff_recursive(
    old: &TypeShape,
    new: &TypeShape,
    symbol_name: &str,
    field_path: &str,
    file: &Path,
    line: u32,
    col: u32,
    depth: usize,
    findings: &mut Vec<Finding>,
) {
    let old = old.without_nominal();
    let new = new.without_nominal();

    // stop at max depth with a warning-level finding
    if depth >= MAX_DEPTH {
        findings.push(Finding {
            severity: Severity::Warning,
            rule: "analysis-incomplete",
            file: file.to_path_buf(),
            line,
            col,
            message: format!("type comparison truncated at depth {MAX_DEPTH} for `{symbol_name}`"),
        });
        return;
    }

    // Preserve uncertainty when either shape is opaque.
    if matches!(old, TypeShape::Opaque(_)) || matches!(new, TypeShape::Opaque(_)) {
        findings.push(Finding {
            severity: Severity::Warning,
            rule: "analysis-incomplete",
            file: file.to_path_buf(),
            line,
            col,
            message: format!("type of `{symbol_name}` could not be compared (unresolvable type)"),
        });
        return;
    }

    // Same type — no findings
    if old == new {
        return;
    }

    // Normalize for comparison
    let old_n = old.clone().normalize();
    let new_n = new.clone().normalize();

    // After normalization, check equality again
    if old_n == new_n {
        return;
    }

    match (&old_n, &new_n) {
        (
            TypeShape::GenericDeclaration {
                params: before,
                body: old_body,
            },
            TypeShape::GenericDeclaration {
                params: after,
                body: new_body,
            },
        ) => {
            findings.extend(generic_diff::compare_parameter_shapes(
                before,
                after,
                symbol_name,
                file,
                line,
                col,
            ));
            diff_recursive(
                old_body,
                new_body,
                symbol_name,
                field_path,
                file,
                line,
                col,
                depth + 1,
                findings,
            );
        }
        (TypeShape::GenericDeclaration { params, body }, other) => {
            findings.extend(generic_diff::compare_parameter_shapes(
                params,
                &[],
                symbol_name,
                file,
                line,
                col,
            ));
            diff_recursive(
                body,
                other,
                symbol_name,
                field_path,
                file,
                line,
                col,
                depth + 1,
                findings,
            );
        }
        (other, TypeShape::GenericDeclaration { params, body }) => {
            findings.extend(generic_diff::compare_parameter_shapes(
                &[],
                params,
                symbol_name,
                file,
                line,
                col,
            ));
            diff_recursive(
                other,
                body,
                symbol_name,
                field_path,
                file,
                line,
                col,
                depth + 1,
                findings,
            );
        }
        // Opaque after normalization (shouldn't happen due to early return above, but be safe)
        (TypeShape::Opaque(_), _) | (_, TypeShape::Opaque(_)) => {
            let display = make_display_name(symbol_name, field_path);
            findings.push(Finding {
                severity: Severity::Warning,
                rule: "analysis-incomplete",
                file: file.to_path_buf(),
                line,
                col,
                message: format!("type of `{display}` could not be compared (unresolvable type)"),
            });
        }

        // Both Object — compare field sets
        (TypeShape::Object(old_fields), TypeShape::Object(new_fields)) => {
            // Fields in old but not new → field-removed (Major)
            for (field_name, old_field) in old_fields {
                if !new_fields.contains_key(field_name) {
                    let path = make_path(field_path, field_name);
                    findings.push(Finding {
                        severity: Severity::Major,
                        rule: RULE_FIELD_REMOVED,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "field `{path}` was removed from type `{symbol_name}` (was {})",
                            format_type(&old_field.shape)
                        ),
                    });
                }
            }

            // Fields in new but not old → field-added
            for (field_name, new_field) in new_fields {
                if !old_fields.contains_key(field_name) {
                    let path = make_path(field_path, field_name);
                    let (severity, optional_label) = if new_field.optional {
                        (Severity::Minor, "optional ")
                    } else {
                        (Severity::Major, "required ")
                    };
                    findings.push(Finding {
                        severity,
                        rule: RULE_FIELD_ADDED,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "{optional_label}field `{path}` was added to type `{symbol_name}` ({})",
                            format_type(&new_field.shape)
                        ),
                    });
                }
            }

            // Fields in both — compare shapes
            for (field_name, old_field) in old_fields {
                if let Some(new_field) = new_fields.get(field_name) {
                    // REF-06: optional property made required
                    if old_field.optional && !new_field.optional {
                        let path = make_path(field_path, field_name);
                        findings.push(Finding {
                            severity: Severity::Major,
                            rule: RULE_PROPERTY_MADE_REQUIRED,
                            file: file.to_path_buf(),
                            line,
                            col,
                            message: format!(
                                "property `{}` was made required in `{}`  (was optional)",
                                path, symbol_name
                            ),
                        });
                    }

                    // IFT-02: readonly modifier flip on type alias object field (parity with CLS-08)
                    if old_field.readonly != new_field.readonly {
                        let path = make_path(field_path, field_name);
                        let direction = if !old_field.readonly && new_field.readonly {
                            "added"
                        } else {
                            "removed"
                        };
                        findings.push(Finding {
                            severity: Severity::Major,
                            rule: RULE_TYPE_ALIAS_READONLY_CHANGED,
                            file: file.to_path_buf(),
                            line,
                            col,
                            message: format!(
                                "field `{}` readonly modifier {} on type `{}`",
                                path, direction, symbol_name
                            ),
                        });
                    }

                    let child_path = make_path(field_path, field_name);
                    if !old_field.readonly
                        && !new_field.readonly
                        && (old_field.write_type.is_some() || new_field.write_type.is_some())
                    {
                        let before = old_field.write_type.as_ref().unwrap_or(&old_field.shape);
                        let after = new_field.write_type.as_ref().unwrap_or(&new_field.shape);
                        if !is_subtype_of(before, after, depth + 1) {
                            findings.push(Finding { severity: Severity::Major, rule: RULE_FIELD_TYPE_CHANGED,
                                file: file.to_path_buf(), line, col,
                                message: format!("setter `{child_path}` in `{symbol_name}` stopped accepting `{}` (now `{}`)", format_type(before), format_type(after)) });
                        }
                    }
                    if old_field.shape != new_field.shape {
                        // Use subtype check to decide: try recursion first for more specific findings.
                        // If recursion produces nothing, emit field-type-changed with old/new values.
                        let prev_len = findings.len();
                        diff_recursive(
                            &old_field.shape,
                            &new_field.shape,
                            symbol_name,
                            &child_path,
                            file,
                            line,
                            col,
                            depth + 1,
                            findings,
                        );
                        if findings.len() == prev_len {
                            // Recursion added nothing — emit field-type-changed
                            findings.push(Finding {
                                severity: Severity::Major,
                                rule: RULE_FIELD_TYPE_CHANGED,
                                file: file.to_path_buf(),
                                line,
                                col,
                                message: format!(
                                    "field `{child_path}` changed type from `{}` to `{}` in `{symbol_name}`",
                                    format_type(&old_field.shape),
                                    format_type(&new_field.shape)
                                ),
                            });
                        }
                    }
                }
            }
        }

        // Both Array — use subtype check on inner type
        (TypeShape::Array(old_inner), TypeShape::Array(new_inner)) => {
            let display = make_display_name(symbol_name, field_path);
            if !is_subtype_of(new_inner, old_inner, depth + 1)
                && !is_subtype_of(old_inner, new_inner, depth + 1)
            {
                // Types are incompatible in both directions
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` changed from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            } else {
                // Try recursion to get more specific findings
                let prev_len = findings.len();
                diff_recursive(
                    old_inner,
                    new_inner,
                    symbol_name,
                    field_path,
                    file,
                    line,
                    col,
                    depth + 1,
                    findings,
                );
                if findings.len() == prev_len && old_inner != new_inner {
                    // Inner types differ but recursion didn't capture it — emit incompatible
                    findings.push(Finding {
                        severity: Severity::Major,
                        rule: RULE_TYPE_INCOMPATIBLE,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "type of `{display}` changed from `{}` to `{}`",
                            format_type(&old_n),
                            format_type(&new_n)
                        ),
                    });
                }
            }
        }

        // Both Tuple — compare element counts, diff each position, and emit
        // REF-10 label-family findings (rename/reorder/drop/add).
        (TypeShape::Tuple(old_elems), TypeShape::Tuple(new_elems)) => {
            if old_elems.len() != new_elems.len() {
                // Length change dominates — existing RULE_TYPE_INCOMPATIBLE
                // finding. REF-10 label findings are SUPPRESSED when length
                // differs: length change already signals breaking, and
                // label findings at now-misaligned positions would be noise.
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` changed from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            } else {
                // REF-10 pre-pass: detect reorder (same label multiset,
                // different positions) and emit ONE Major
                // tuple-label-reordered, skipping per-position rename
                // emission. Labels must be Some() on BOTH sides for the
                // reorder detector to fire — missing labels use the
                // per-position drop/add path instead.
                let old_labels: Vec<Option<&String>> =
                    old_elems.iter().map(|e| e.label.as_ref()).collect();
                let new_labels: Vec<Option<&String>> =
                    new_elems.iter().map(|e| e.label.as_ref()).collect();

                let reorder_detected = {
                    let all_old_labeled = old_labels.iter().all(|l| l.is_some());
                    let all_new_labeled = new_labels.iter().all(|l| l.is_some());
                    if all_old_labeled && all_new_labeled {
                        let mut old_sorted: Vec<&String> =
                            old_labels.iter().filter_map(|l| *l).collect();
                        let mut new_sorted: Vec<&String> =
                            new_labels.iter().filter_map(|l| *l).collect();
                        old_sorted.sort();
                        new_sorted.sort();
                        old_sorted == new_sorted && old_labels != new_labels
                    } else {
                        false
                    }
                };

                if reorder_detected {
                    let display = make_display_name(symbol_name, field_path);
                    findings.push(Finding {
                        severity: Severity::Patch,
                        rule: RULE_TUPLE_LABEL_REORDERED,
                        file: file.to_path_buf(),
                        line,
                        col,
                        message: format!(
                            "tuple labels on type `{display}` were reordered (labels do not affect positional types)"
                        ),
                    });
                }

                for (i, (old_elem, new_elem)) in old_elems.iter().zip(new_elems.iter()).enumerate()
                {
                    let idx_path = if field_path.is_empty() {
                        format!("[{i}]")
                    } else {
                        format!("{field_path}[{i}]")
                    };
                    if (old_elem.optional && !new_elem.optional) || old_elem.rest != new_elem.rest {
                        let display = make_display_name(symbol_name, &idx_path);
                        findings.push(Finding {
                            severity: Severity::Major,
                            rule: RULE_TYPE_INCOMPATIBLE,
                            file: file.to_path_buf(),
                            line,
                            col,
                            message: format!(
                                "tuple element `{display}` changed optional or rest requirements"
                            ),
                        });
                    }
                    // Recurse on shape (label-blind structural diff).
                    diff_recursive(
                        &old_elem.shape,
                        &new_elem.shape,
                        symbol_name,
                        &idx_path,
                        file,
                        line,
                        col,
                        depth + 1,
                        findings,
                    );

                    // Skip per-position label findings when the reorder
                    // pre-pass already fired — label change is accounted for
                    // by the single RULE_TUPLE_LABEL_REORDERED finding.
                    if reorder_detected {
                        continue;
                    }

                    let display = make_display_name(symbol_name, field_path);
                    match (&old_elem.label, &new_elem.label) {
                        (Some(a), Some(b)) if a != b => {
                            findings.push(Finding {
                                severity: Severity::Minor,
                                rule: RULE_TUPLE_LABEL_RENAMED,
                                file: file.to_path_buf(),
                                line,
                                col,
                                message: format!(
                                    "tuple element at position {i} on type `{display}` was relabeled (`{a}` -> `{b}`)"
                                ),
                            });
                        }
                        (Some(a), None) => {
                            findings.push(Finding {
                                severity: Severity::Patch,
                                rule: RULE_TUPLE_LABEL_DROPPED,
                                file: file.to_path_buf(),
                                line,
                                col,
                                message: format!(
                                    "tuple element at position {i} on type `{display}` lost its label (`{a}`)"
                                ),
                            });
                        }
                        (None, Some(b)) => {
                            findings.push(Finding {
                                severity: Severity::Minor,
                                rule: RULE_TUPLE_LABEL_ADDED,
                                file: file.to_path_buf(),
                                line,
                                col,
                                message: format!(
                                    "tuple element at position {i} on type `{display}` gained a label (`{b}`)"
                                ),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }

        // Both Union — use subtype checker for variance-aware comparison.
        //
        // BEH-06: BEFORE the plain-union subtype check, detect
        // discriminated-union widening — variants added to an exported
        // discriminated (tagged) union. This
        // fires Warning `union-variant-added` (mirroring BEH-04
        // `enum-member-added`) because an exhaustive-switch consumer will
        // break at runtime even though the type is structurally widened.
        //
        // Scope guard: only discriminated unions — plain-union widening
        // (e.g. `number | boolean` -> `number | boolean | string`) keeps
        // its existing compatible-widening behavior (no finding).
        (TypeShape::Union(old_members), TypeShape::Union(new_members)) => {
            if let (Some(old_disc), Some(new_disc)) = (
                is_discriminated_union(old_members),
                is_discriminated_union(new_members),
            ) {
                if old_disc == new_disc && is_subtype_of(&old_n, &new_n, depth + 1) {
                    // Widening direction confirmed (old ⊆ new) with a
                    // shared discriminant — fire one Warning per added
                    // variant, keyed by discriminant literal value.
                    let added = added_discriminant_values(&old_disc, old_members, new_members);
                    let display = make_display_name(symbol_name, field_path);
                    for added_value in &added {
                        findings.push(Finding {
                            severity: Severity::Warning,
                            rule: RULE_UNION_VARIANT_ADDED,
                            file: file.to_path_buf(),
                            line,
                            col,
                            message: format!(
                                "discriminated union variant {} was added to `{}` -- may break exhaustive switch statements on `{}`",
                                added_value, display, old_disc,
                            ),
                        });
                    }
                    // BEH-06 handled — do NOT fall through to the plain-union
                    // subtype check (otherwise widening would emit nothing
                    // AND we would miss the Warning).
                    if !added.is_empty() {
                        return;
                    }
                    // Discriminated & widening but zero added values — this
                    // can happen when variants change shape without changing
                    // the discriminant value set. Let the plain-union path
                    // below handle that (it will be a subtype relationship
                    // so no finding fires; any field-level diffs would have
                    // already been caught if the values were non-disjoint).
                }
            }

            if !is_subtype_of(&old_n, &new_n, depth + 1) {
                // Old union is not a subtype of new union — some members were removed/narrowed
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` narrowed from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            }
            // If old is a subtype of new (and not a discriminated widening
            // handled above), the union was widened in a plain-union sense —
            // compatible, no finding.
        }

        // Both Intersection — adding a member narrows the type (breaking)
        (TypeShape::Intersection(_), TypeShape::Intersection(_)) => {
            if !is_subtype_of(&new_n, &old_n, depth + 1) {
                // New intersection requires more constraints than old — narrowing
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` changed incompatibly from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            }
        }

        // Both Function — variance-aware signature analysis (SIG-01..05)
        (
            TypeShape::Function {
                params: old_params,
                return_type: old_ret,
                contract: old_contract,
            },
            TypeShape::Function {
                params: new_params,
                return_type: new_ret,
                contract: new_contract,
            },
        ) => {
            if !accepts_receiver(
                new_contract.receiver.as_deref(),
                old_contract.receiver.as_deref(),
                depth + 1,
            ) {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: type_rules::RULE_THIS_PARAM_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "receiver requirements narrowed in `{}`",
                        make_display_name(symbol_name, field_path)
                    ),
                });
            }
            if !preserves_predicate(
                new_contract.predicate.as_ref(),
                old_contract.predicate.as_ref(),
                depth + 1,
            ) {
                let assertion = old_contract
                    .predicate
                    .as_ref()
                    .is_some_and(|predicate| predicate.asserts);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: if assertion {
                        type_rules::RULE_ASSERTION_WIDENED
                    } else {
                        type_rules::RULE_TYPE_GUARD_WIDENED
                    },
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type narrowing guarantee changed in `{}`",
                        make_display_name(symbol_name, field_path)
                    ),
                });
            }
            diff_functions(
                old_params,
                old_ret,
                new_params,
                new_ret,
                symbol_name,
                field_path,
                file,
                line,
                col,
                depth,
                findings,
            );
        }

        // Both Generic — compare names and args (invariant position)
        (
            TypeShape::Generic {
                name: old_name,
                args: old_args,
            },
            TypeShape::Generic {
                name: new_name,
                args: new_args,
            },
        ) => {
            if old_name != new_name || old_args.len() != new_args.len() {
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_TYPE_INCOMPATIBLE,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "type of `{display}` changed from `{}` to `{}`",
                        format_type(&old_n),
                        format_type(&new_n)
                    ),
                });
            } else {
                for (i, (old_arg, new_arg)) in old_args.iter().zip(new_args.iter()).enumerate() {
                    let arg_path = if field_path.is_empty() {
                        format!("arg[{i}]")
                    } else {
                        format!("{field_path}.arg[{i}]")
                    };
                    diff_recursive(
                        old_arg,
                        new_arg,
                        symbol_name,
                        &arg_path,
                        file,
                        line,
                        col,
                        depth + 1,
                        findings,
                    );
                }
            }
        }

        // TYPE-02: Both InferExtends — any change in the `extends`
        // constraint is Major. The `name` field (the inferred identifier,
        // e.g. `U` vs `V`) is NOT consumer-observable and so differences in
        // the name alone do NOT fire a finding.
        (
            TypeShape::InferExtends {
                name: old_name,
                constraint: old_c,
            },
            TypeShape::InferExtends {
                name: new_name,
                constraint: new_c,
            },
        ) => {
            // Silence unused-variable warnings — the name is intentionally
            // NOT part of the comparison because infer binders are local to their conditional type.
            let _ = (old_name, new_name);
            if old_c != new_c {
                let display = make_display_name(symbol_name, field_path);
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INFER_EXTENDS_CONSTRAINT_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "constraint on `infer {}` in `{}` changed from `{}` to `{}`",
                        old_name,
                        display,
                        format_type(old_c),
                        format_type(new_c),
                    ),
                });
            }
        }

        // TYPE-02 plumbing: Both Conditional — walk into each
        // of the four child shapes (check / extends / then_type / else_type)
        // so an `InferExtends` buried inside the extends-leg (the canonical
        // TYPE-02 position) is visible to the (InferExtends, InferExtends)
        // arm above. Without this walker, conditional-type pairs fall
        // through to the `_` arm and emit a generic `type-incompatible`
        // that hides the more-specific TYPE-02 rule.
        (
            TypeShape::Conditional {
                check: old_check,
                extends: old_extends,
                then_type: old_then,
                else_type: old_else,
            },
            TypeShape::Conditional {
                check: new_check,
                extends: new_extends,
                then_type: new_then,
                else_type: new_else,
            },
        ) => {
            let legs: [(&str, &TypeShape, &TypeShape); 4] = [
                ("check", old_check, new_check),
                ("extends", old_extends, new_extends),
                ("then_type", old_then, new_then),
                ("else_type", old_else, new_else),
            ];
            for (leg_name, old_leg, new_leg) in legs {
                let leg_path = if field_path.is_empty() {
                    leg_name.to_string()
                } else {
                    format!("{field_path}.{leg_name}")
                };
                diff_recursive(
                    old_leg,
                    new_leg,
                    symbol_name,
                    &leg_path,
                    file,
                    line,
                    col,
                    depth + 1,
                    findings,
                );
            }
        }

        // Both Primitive — if we got here after equality check, they differ
        (TypeShape::Primitive(_), TypeShape::Primitive(_)) => {
            let display = make_display_name(symbol_name, field_path);
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type of `{display}` changed from `{}` to `{}`",
                    format_type(&old_n),
                    format_type(&new_n)
                ),
            });
        }

        // Both Literal — if we got here after equality check, they differ
        (TypeShape::Literal(_), TypeShape::Literal(_)) => {
            let display = make_display_name(symbol_name, field_path);
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type of `{display}` changed from `{}` to `{}`",
                    format_type(&old_n),
                    format_type(&new_n)
                ),
            });
        }

        // Different variant entirely — emit type-incompatible (Major)
        _ => {
            let display = make_display_name(symbol_name, field_path);
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "type of `{display}` changed incompatibly from `{}` to `{}`",
                    format_type(&old_n),
                    format_type(&new_n)
                ),
            });
        }
    }
}

/// Variance-aware function signature diff (SIG-01..05).
///
/// Called from diff_recursive when both old and new are Function variants.
#[allow(clippy::too_many_arguments)]
fn diff_functions(
    old_params: &[ParamShape],
    old_ret: &TypeShape,
    new_params: &[ParamShape],
    new_ret: &TypeShape,
    symbol_name: &str,
    field_path: &str,
    file: &Path,
    line: u32,
    col: u32,
    depth: usize,
    findings: &mut Vec<Finding>,
) {
    let old_arguments = argument_parameters(old_params);
    let new_arguments = argument_parameters(new_params);
    let old_params = old_arguments.as_ref();
    let new_params = new_arguments.as_ref();
    // TypeScript permits assigning a readonly object to a mutable object type,
    // but callers writing directly through a returned property still break.
    let mut property_restrictions = Vec::new();
    returned_property_restrictions(
        old_ret,
        new_ret,
        field_path,
        depth,
        &mut property_restrictions,
    );
    findings.extend(property_restrictions.into_iter().map(|(path, rule)| Finding {
        severity: Severity::Major,
        rule,
        file: file.to_path_buf(),
        line,
        col,
        message: if rule == RULE_TYPE_ALIAS_READONLY_CHANGED {
            format!("returned field `{path}` of `{symbol_name}` became readonly")
        } else if rule == RULE_PARAM_TYPE_NARROWED {
            format!("returned callable `{path}` of `{symbol_name}` no longer accepts every previous call")
        } else {
            format!("returned field `{path}` of `{symbol_name}` no longer accepts every previous assignment")
        },
    }));

    // SIG-05: Param reorder detection (check FIRST).
    // Find params that exist in both by name. If 2+ common params and their
    // relative order differs, emit param-reordered and return.
    {
        let old_names: Vec<&str> = old_params.iter().map(|p| p.name.as_str()).collect();
        let new_names: Vec<&str> = new_params.iter().map(|p| p.name.as_str()).collect();

        // Common params: names present in both lists
        let common_in_old: Vec<&str> = old_names
            .iter()
            .filter(|&&n| new_names.contains(&n))
            .copied()
            .collect();
        let common_in_new: Vec<&str> = new_names
            .iter()
            .filter(|&&n| old_names.contains(&n))
            .copied()
            .collect();

        let positions_changed = old_params.iter().zip(new_params).any(|(before, after)| {
            before.shape != after.shape
                || before.optional != after.optional
                || before.rest != after.rest
        });
        if common_in_old.len() >= 2 && common_in_old != common_in_new && positions_changed {
            let fn_name = symbol_name;
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_PARAM_REORDERED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "parameter order changed in `{fn_name}`: was ({}), now ({})",
                    old_names.join(", "),
                    new_names.join(", ")
                ),
            });
            return;
        }
    }

    // SIG-01/SIG-04: Param count changes.
    // New params beyond old.len() are truly added params.
    if new_params.len() > old_params.len() {
        for (i, new_param) in new_params.iter().enumerate().skip(old_params.len()) {
            let requires_argument = !new_param.optional
                && (!new_param.rest
                    || match &new_param.shape {
                        TypeShape::Array(_) => false,
                        TypeShape::Tuple(elements) => elements
                            .iter()
                            .any(|element| !element.optional && !element.rest),
                        _ => true,
                    });
            if requires_argument {
                // SIG-01: required param added — Major
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_PARAM_ADDED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "required parameter `{}` added to `{symbol_name}` at position {i}",
                        new_param.name
                    ),
                });
            } else {
                // SIG-04: optional param added — Minor
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_PARAM_ADDED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "optional parameter `{}` added to `{symbol_name}`",
                        new_param.name
                    ),
                });
            }
        }
    }

    // SIG-01: Removed params (old has more than new).
    if old_params.len() > new_params.len() {
        for old_param in old_params.iter().skip(new_params.len()) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_PARAM_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "parameter `{}` removed from `{symbol_name}`",
                    old_param.name
                ),
            });
        }
    }

    // Optional parameter becoming required at same position.
    // ParamShape has `optional: bool` (shape.rs:60). When old_param.optional=true
    // and new_param.optional=false at the same position, this is a breaking change:
    // callers that omitted the argument will now get a type error.
    for (i, (old_param, new_param)) in old_params.iter().zip(new_params.iter()).enumerate() {
        if old_param.optional && !new_param.optional {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_PARAM_MADE_REQUIRED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "parameter `{}` changed from optional to required in `{}` (position {})",
                    new_param.name, symbol_name, i
                ),
            });
        }
    }

    // SIG-02: Param type narrowing (for params that exist at same position in both).
    // Params are contravariant: new param type must accept everything old param type did.
    // Check: is new_param a supertype of old_param? i.e., is_subtype_of(old_param, new_param)?
    // If NOT, the new param type is narrower — breaking.
    for (i, (old_param, new_param)) in old_params.iter().zip(new_params.iter()).enumerate() {
        if old_param.shape != new_param.shape {
            // SEM-03: Treat any → T (where T is not any) as tightening regardless of subtype
            // direction. Reason: is_subtype_of returns true for (any, X) in BOTH directions
            // (any is both top and bottom in TypeScript's escape-hatch semantics), which
            // incorrectly suppresses the narrowing finding when old param is `any`.
            let is_any_to_concrete =
                is_any_shape(&old_param.shape) && !is_any_shape(&new_param.shape);
            if is_any_to_concrete {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_PARAM_TYPE_NARROWED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "parameter `{}` narrowed from `any` to `{}` in `{symbol_name}` (position {i})",
                        new_param.name,
                        format_type(&new_param.shape)
                    ),
                });
                continue;
            }
            // Contravariant check: new param must accept what old param accepted.
            // new_param is "safe" if old_param <: new_param (old is subtype of new)
            // meaning new accepts at least everything old accepted.
            if !is_subtype_of(&old_param.shape, &new_param.shape, depth + 1) {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_PARAM_TYPE_NARROWED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "parameter `{}` type narrowed from `{}` to `{}` in `{symbol_name}` (position {i})",
                        new_param.name,
                        format_type(&old_param.shape),
                        format_type(&new_param.shape)
                    ),
                });
            }
            // If old_param <: new_param, the new param accepts a wider type — safe, no finding.
        }
    }

    // SIG-03: Return type widening.
    // Return types are covariant: the safe direction is new_ret <: old_ret (new is narrower).
    // Widening occurs when old_ret <: new_ret but new_ret !<: old_ret (new strictly wider).
    // Example: string -> string | number is widening (breaking for callers that assumed string).
    // Incompatible change (string -> number): neither subtypes the other → emit type-incompatible
    // instead of return-type-widened to be accurate.
    if old_ret != new_ret {
        let old_subtypes_new = is_subtype_of(old_ret, new_ret, depth + 1);
        let new_subtypes_old = is_subtype_of(new_ret, old_ret, depth + 1);
        if old_subtypes_new && !new_subtypes_old {
            // New return type is strictly wider — breaking (callers see more possible values)
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_RETURN_TYPE_WIDENED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "return type widened from `{}` to `{}` in `{symbol_name}`",
                    format_type(old_ret),
                    format_type(new_ret)
                ),
            });
        } else if !old_subtypes_new && !new_subtypes_old {
            // Completely incompatible return types — emit type-incompatible
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_INCOMPATIBLE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "return type of `{symbol_name}` changed incompatibly from `{}` to `{}`",
                    format_type(old_ret),
                    format_type(new_ret)
                ),
            });
        }
        // else: new_subtypes_old (new is narrower or equal) — safe, no finding
    }
}

fn returned_property_restrictions(
    old: &TypeShape,
    new: &TypeShape,
    path: &str,
    depth: usize,
    paths: &mut Vec<(String, &'static str)>,
) {
    let old = old.without_nominal();
    let new = new.without_nominal();
    if depth >= MAX_DEPTH || old == new {
        return;
    }
    match (old, new) {
        (TypeShape::Object(before), TypeShape::Object(after)) => {
            for (name, old_field) in before {
                let Some(new_field) = after.get(name) else {
                    continue;
                };
                let child = make_path(path, name);
                if !old_field.readonly && new_field.readonly {
                    paths.push((child.clone(), RULE_TYPE_ALIAS_READONLY_CHANGED));
                } else if !old_field.readonly
                    && old_field.write_type.is_none()
                    && new_field.write_type.is_none()
                    && is_subtype_of(&new_field.shape, &old_field.shape, depth + 1)
                    && !is_subtype_of(&old_field.shape, &new_field.shape, depth + 1)
                {
                    paths.push((child.clone(), RULE_FIELD_TYPE_CHANGED));
                }
                returned_property_restrictions(
                    &old_field.shape,
                    &new_field.shape,
                    &child,
                    depth + 1,
                    paths,
                );
            }
        }
        (TypeShape::Array(before), TypeShape::Array(after)) => {
            if !is_subtype_of(before, after, depth + 1) {
                paths.push((format!("{path}[]"), RULE_FIELD_TYPE_CHANGED));
            }
            returned_property_restrictions(before, after, path, depth + 1, paths);
        }
        (TypeShape::Tuple(before), TypeShape::Tuple(after)) => {
            for (index, (before, after)) in before.iter().zip(after).enumerate() {
                if !is_subtype_of(&before.shape, &after.shape, depth + 1) {
                    paths.push((format!("{path}[{index}]"), RULE_FIELD_TYPE_CHANGED));
                }
                returned_property_restrictions(&before.shape, &after.shape, path, depth + 1, paths);
            }
        }
        (
            TypeShape::Function {
                params: before_params,
                return_type: before,
                ..
            },
            TypeShape::Function {
                params: after_params,
                return_type: after,
                ..
            },
        ) => {
            if !accepts_calls(after_params, before_params, depth + 1) {
                paths.push((path.to_string(), RULE_PARAM_TYPE_NARROWED));
            }
            returned_property_restrictions(before, after, path, depth + 1, paths);
        }
        (
            TypeShape::Generic {
                name: before_name,
                args: before,
            },
            TypeShape::Generic {
                name: after_name,
                args: after,
            },
        ) if before_name == after_name => {
            if before_name == "Readonly" {
                if let (Some(before), Some(after)) = (before.first(), after.first()) {
                    // Readonly removes writes to the outer container, while its
                    // elements can still expose nested mutable contracts.
                    match (before, after) {
                        (TypeShape::Object(before), TypeShape::Object(after)) => {
                            for (name, before) in before {
                                if let Some(after) = after.get(name) {
                                    returned_property_restrictions(
                                        &before.shape,
                                        &after.shape,
                                        &make_path(path, name),
                                        depth + 1,
                                        paths,
                                    );
                                }
                            }
                        }
                        _ => {
                            for (before, after) in
                                before.children().into_iter().zip(after.children())
                            {
                                returned_property_restrictions(
                                    before,
                                    after,
                                    path,
                                    depth + 1,
                                    paths,
                                );
                            }
                        }
                    }
                }
                return;
            }
            if matches!(
                before_name.as_str(),
                "Map" | "Set" | "WeakMap" | "WeakSet" | "Record"
            ) && before
                .iter()
                .zip(after)
                .any(|(before, after)| !is_subtype_of(before, after, depth + 1))
            {
                paths.push((format!("{path}[]"), RULE_FIELD_TYPE_CHANGED));
            }
            for (before, after) in before.iter().zip(after) {
                returned_property_restrictions(before, after, path, depth + 1, paths);
            }
        }
        (TypeShape::Union(before), TypeShape::Union(after))
        | (TypeShape::Intersection(before), TypeShape::Intersection(after)) => {
            for (before, after) in before.iter().zip(after) {
                returned_property_restrictions(before, after, path, depth + 1, paths);
            }
        }
        _ => {}
    }
}

/// GEN-07: Detect `NoInfer<T>` wrapper removal at
/// function-parameter positions across paired function overloads.
///
/// Walks each pair of `Function` shapes (one from old, one from new) and,
/// at every shared parameter position, checks whether the old param's
/// shape is `TypeShape::NoInferWrap(inner)` while the new param's shape
/// equals the unwrapped `inner`. Each such position emits one Minor
/// `generic-inference-constraint-removed` finding.
///
/// Two shape sources are inspected:
///
/// 1. **Canonical shapes** (`TypeShape::NoInferWrap(inner)`): produced
///    when the param shape was filled by a path that ran through
///    `parse_type_string` (LSP hover enrichment, type-alias body
///    extraction in `compare_structural_diff`, or future paths). Direct
///    pattern-match.
/// 2. **Opaque text shapes** (`TypeShape::Opaque(text)`): produced by
///    `tree_query::build_function_shape_from_node` for non-LSP runs.
///    The text carries the verbatim source (`"NoInfer<T>"`); we route it
///    through `parse_type_string` so the GEN-07 special-case in
///    `parse_generic` produces `NoInferWrap(inner)`. Same pattern-match
///    then applies. This is the path that fires for the GEN-07
///    integration fixtures in tests where LSP is not available.
///
/// Caller-supplied `symbol_name` appears in the finding message;
/// `file`/`line`/`col` carry source location. Findings are appended to
/// the returned `Vec<Finding>`.
///
/// Return types are excluded: return-type NoInfer doesn't have the same consumer-
/// observable inference semantics as parameter NoInfer.
pub fn diff_function_param_noinfer_removed(
    old_overloads: &[TypeShape],
    new_overloads: &[TypeShape],
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Pair overloads positionally — non-LSP extraction yields one shape
    // per declared signature in source order, mirroring the original
    // function-overload list. If counts differ, walk the shorter prefix
    // (the count mismatch is REF-01 territory, not GEN-07's concern).
    let pair_count = old_overloads.len().min(new_overloads.len());
    for i in 0..pair_count {
        let before = old_overloads[i].without_type_parameters();
        let after = new_overloads[i].without_type_parameters();
        let mut nested = |before: &TypeShape, after: &TypeShape| {
            findings.extend(diff_function_param_noinfer_removed(
                std::slice::from_ref(before),
                std::slice::from_ref(after),
                symbol_name,
                file,
                line,
                col,
            ));
        };
        match (before, after) {
            (TypeShape::Object(before), TypeShape::Object(after)) => {
                for (name, before) in before {
                    if let Some(after) = after.get(name) {
                        nested(&before.shape, &after.shape);
                    }
                }
            }
            (TypeShape::Array(before), TypeShape::Array(after)) => nested(before, after),
            (TypeShape::Tuple(before), TypeShape::Tuple(after)) => {
                for (before, after) in before.iter().zip(after) {
                    nested(&before.shape, &after.shape);
                }
            }
            (
                TypeShape::Generic {
                    name: before_name,
                    args: before,
                },
                TypeShape::Generic {
                    name: after_name,
                    args: after,
                },
            ) if before_name == after_name => {
                for (before, after) in before.iter().zip(after) {
                    nested(before, after);
                }
            }
            (
                TypeShape::Function {
                    return_type: before,
                    ..
                },
                TypeShape::Function {
                    return_type: after, ..
                },
            ) => nested(before, after),
            _ => {}
        }
        let (
            TypeShape::Function {
                params: old_params, ..
            },
            TypeShape::Function {
                params: new_params, ..
            },
        ) = (before, after)
        else {
            continue; // not both Function — nothing to walk
        };

        // Per-param positional walk. Mismatched counts are the existing
        // REF-01 / SIG-01 territory; we walk only the shared prefix.
        let n = old_params.len().min(new_params.len());
        for pos in 0..n {
            let old_p = &old_params[pos];
            let new_p = &new_params[pos];
            if detect_noinfer_param_removal(&old_p.shape, &new_p.shape) {
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_GENERIC_INFERENCE_CONSTRAINT_REMOVED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "`NoInfer<T>` wrapper removed from parameter `{}` (position {}) of `{}` — consumer inference behavior may change",
                        new_p.name, pos, symbol_name
                    ),
                });
            } else if detect_noinfer_param_removal(&new_p.shape, &old_p.shape) {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: type_rules::RULE_GENERIC_INFERENCE_CONSTRAINT_ADDED,
                    file: file.to_path_buf(), line, col,
                    message: format!("`NoInfer<T>` added to parameter `{}` of `{symbol_name}` blocks previously accepted inference", new_p.name),
                });
            }
        }
    }

    findings
}

/// Helper for `diff_function_param_noinfer_removed`: returns `true`
/// when removing the `NoInfer<T>` wrapper somewhere inside `old_shape`
/// would produce `new_shape` (the wrapper(s) were removed but the inner
/// types are otherwise structurally equivalent).
///
/// Three shape positions are inspected, in order:
///
/// 1. **Direct canonical wrapper** — `old_shape = NoInferWrap(inner)`
///    and `new_shape == inner` unwrapped. Common when the param itself
///    is `NoInfer<T>`.
/// 2. **Opaque-text path** — both shapes are `Opaque(text)`; route both
///    through `parse_type_string` and recurse on the parsed forms. The
///    parser's GEN-07 special-case in `parse_generic` produces
///    `NoInferWrap` for verbatim `NoInfer<X>`, so the recursive check
///    finds the wrapper inside parsed callback signatures, etc.
/// 3. **Callback-shape recursion** — when both shapes are `Function`
///    (e.g., `(prev: NoInfer<T>) => NoInfer<T>` -> `(prev: T) => T`
///    parsed from the param's type-text), walk paired callback params
///    and the return type and recurse on each pair. This captures the
///    common TS 5.4 pattern where `NoInfer` wraps a callback's
///    parameter.
///
/// Returns `true` if AT LEAST ONE NoInfer-wrapper position differs in
/// the unwrap direction. The diff_function_param_noinfer_removed caller
/// emits one Minor finding per top-level param hit.
fn detect_noinfer_param_removal(old_shape: &TypeShape, new_shape: &TypeShape) -> bool {
    // 1. Direct canonical wrapper.
    if let TypeShape::NoInferWrap(old_inner) = old_shape {
        if !matches!(new_shape, TypeShape::NoInferWrap(_)) && **old_inner == *new_shape {
            return true;
        }
    }

    // 2. Opaque-text path — parse and recurse on the parsed result.
    if let (TypeShape::Opaque(old_text), TypeShape::Opaque(new_text)) = (old_shape, new_shape) {
        let old_parsed = type_parser::parse_type_string(old_text);
        let new_parsed = type_parser::parse_type_string(new_text);
        // Avoid infinite recursion: only recurse if parsing gave us a
        // structurally different shape (anything other than another
        // pair of Opaque on the same texts).
        if !matches!(
            (&old_parsed, &new_parsed),
            (TypeShape::Opaque(_), TypeShape::Opaque(_))
        ) && detect_noinfer_param_removal(&old_parsed, &new_parsed)
        {
            return true;
        }
    }

    // 3. Callback-shape recursion (parsed from a param's type-text).
    if let (
        TypeShape::Function {
            params: old_p,
            return_type: old_r,
            ..
        },
        TypeShape::Function {
            params: new_p,
            return_type: new_r,
            ..
        },
    ) = (old_shape, new_shape)
    {
        let n = old_p.len().min(new_p.len());
        for i in 0..n {
            if detect_noinfer_param_removal(&old_p[i].shape, &new_p[i].shape) {
                return true;
            }
        }
        if detect_noinfer_param_removal(old_r, new_r) {
            return true;
        }
    }

    false
}

/// Build a dot-separated path for nested field access.
fn make_path(prefix: &str, field: &str) -> String {
    if prefix.is_empty() {
        field.to_string()
    } else {
        format!("{prefix}.{field}")
    }
}

/// Build a display name for finding messages that includes the field path when present.
/// e.g. symbol_name="MyType", field_path="config.timeout" → "MyType.config.timeout"
/// e.g. symbol_name="MyType", field_path="" → "MyType"
fn make_display_name(symbol_name: &str, field_path: &str) -> String {
    if field_path.is_empty() {
        symbol_name.to_string()
    } else {
        format!("{symbol_name}.{field_path}")
    }
}

/// TYPE-01: Emits `type-narrowing-widened` (Minor) when the old
/// version of an exported binding carried a `satisfies T` clause and the new
/// version does not.
///
/// Emission conditions:
/// - `old_sym.satisfies_type.is_some()` AND `new_sym.satisfies_type.is_none()`
///   fires exactly 1 Minor finding for the symbol.
/// - Otherwise emits nothing.
///
/// Degraded-mode handling: when the `Satisfies.narrowed` slot is `None` on
/// the old symbol (LSP was unavailable during enrichment, or the hover did
/// not surface a narrowed type), the finding message embeds
/// `limitation: satisfies-narrowed-type-not-verified` so downstream consumers
/// know narrowing direction was not confirmed. Never silently drop coverage
pub fn diff_satisfies_types(
    old_sym: &js_model::ExportSymbol,
    new_sym: &js_model::ExportSymbol,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    use crate::languages::typescript::compare::rules::RULE_TYPE_NARROWING_WIDENED;
    let mut findings: Vec<Finding> = Vec::new();
    let old_has_satisfies = matches!(
        old_sym.satisfies_type.as_ref(),
        Some(TypeShape::Satisfies { .. })
    );
    let new_has_satisfies = matches!(
        new_sym.satisfies_type.as_ref(),
        Some(TypeShape::Satisfies { .. })
    );
    if !old_has_satisfies || new_has_satisfies {
        return findings;
    }
    if let (
        Some(TypeShape::Satisfies {
            narrowed: Some(before),
            ..
        }),
        Some(after),
    ) = (&old_sym.satisfies_type, new_sym.overloads.first())
    {
        if is_subtype_of(after, before, 0) {
            return findings;
        }
    }
    // `satisfies` was present, now absent.
    let narrowed_was_verified = matches!(
        old_sym.satisfies_type.as_ref(),
        Some(TypeShape::Satisfies {
            narrowed: Some(_),
            ..
        })
    );
    let message = if narrowed_was_verified {
        format!(
            "`satisfies T` removed from `{}` \u{2014} narrowed type was surfaced in the public signature; consumers that relied on the narrowing will observe the broader type",
            new_sym.name
        )
    } else {
        format!(
            "`satisfies T` removed from `{}` \u{2014} narrowing direction not confirmed (limitation: satisfies-narrowed-type-not-verified)",
            new_sym.name
        )
    };
    findings.push(Finding {
        severity: Severity::Minor,
        rule: RULE_TYPE_NARROWING_WIDENED,
        file: file.to_path_buf(),
        line,
        col,
        message,
    });
    // DOC-01: paired Info finding when degraded-mode emit fires
    // (i.e., narrowing direction was not LSP-verified). Original Minor message
    // retains the substring for backward-compat with existing fixture assertions.
    if !narrowed_was_verified {
        findings.push(Finding {
            severity: Severity::Info,
            rule: type_rules::RULE_LIMITATION_SATISFIES_NARROWED,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "narrowing direction not LSP-confirmed for `{}`",
                new_sym.name
            ),
        });
    }
    findings
}

/// INF-04: emit `type-reference-removed` findings for
/// `typeof import('./y').Z` references whose target symbol is absent
/// from the new version.
///
/// Two branches, gated by `lsp_was_available`:
///
/// - **LSP-available (Major):** old/new versions both ran the LSP
///   second pass. For each ref, we compare the resolved-target
///   side. If the OLD side resolved to a Location and the NEW side did
///   NOT (`Some(&None)` or `None`), the reference target was removed —
///   fires Major `type-reference-removed`.
/// - **LSP-unavailable (Minor):** at least one side was Degraded. We
///   cannot verify removal but must NOT silently drop coverage. Emit
///   one Minor finding per ref with `(limitation: lsp-required)` in the
///   message to make the missing evidence explicit.
///
/// Shape mirrors `diff_satisfies_types` (above) — the same
/// "optional LSP-derived slot determines verified-vs-limitation branch"
/// pattern.
#[allow(dead_code, clippy::too_many_arguments)]
pub fn diff_typeof_import_refs(
    old_sym: &js_model::ExportSymbol,
    new_sym: &js_model::ExportSymbol,
    old_exports: &js_model::ExportMap,
    new_exports: &js_model::ExportMap,
    old_key: &js_model::ExportKey,
    new_key: &js_model::ExportKey,
    lsp_was_available: bool,
    file: &Path,
    findings: &mut Vec<Finding>,
) {
    use crate::languages::typescript::compare::rules::RULE_TYPE_REFERENCE_REMOVED;

    if old_sym.typeof_import_refs.is_empty() {
        return;
    }
    for (idx, r) in old_sym.typeof_import_refs.iter().enumerate() {
        if !lsp_was_available {
            // Degraded path: emit limitation-annotated advisory (/21
            // pattern). Severity = Minor — a verifiable Major requires LSP
            // confirmation that the target is actually missing in the new
            // version.
            findings.push(Finding {
                severity: Severity::Minor,
                rule: RULE_TYPE_REFERENCE_REMOVED,
                file: file.to_path_buf(),
                line: new_sym.line,
                col: new_sym.col,
                message: format!(
                    "type reference `typeof import('{}').{}` on `{}` could not be verified (limitation: lsp-required)",
                    r.specifier, r.property, new_sym.name
                ),
            });
            // DOC-01: paired Info for the lsp-required limitation.
            findings.push(Finding {
                severity: Severity::Info,
                rule: type_rules::RULE_LIMITATION_LSP_REQUIRED,
                file: file.to_path_buf(),
                line: new_sym.line,
                col: new_sym.col,
                message: format!(
                    "typeof-import target `{}.{}` could not be resolved (LSP unavailable)",
                    r.specifier, r.property
                ),
            });
            continue;
        }

        // Verified path — both old and new have run the LSP second pass.
        // Read the resolved targets out of the typeof_targets sidecar.
        let old_target = old_exports.typeof_target(old_key, idx);
        let new_target = new_exports.typeof_target(new_key, idx);

        let old_resolved = matches!(old_target, Some(loc_opt) if loc_opt.is_some());
        let new_resolved = matches!(new_target, Some(loc_opt) if loc_opt.is_some());

        if old_resolved && !new_resolved {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_TYPE_REFERENCE_REMOVED,
                file: file.to_path_buf(),
                line: new_sym.line,
                col: new_sym.col,
                message: format!(
                    "type reference `typeof import('{}').{}` on `{}` was removed \u{2014} `{}` is no longer exported from `{}`",
                    r.specifier, r.property, new_sym.name, r.property, r.specifier
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests;
