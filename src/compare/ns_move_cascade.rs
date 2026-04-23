//! IDN-02 (Phase 18): Namespace-move cascade consolidation.
//!
//! Per D-09 (exact-subtree match): fires ONLY when every dotted-name member
//! under the old namespace path has a shape-equal counterpart under the new
//! namespace path. Partial overlap, threshold-based matching, and fuzzy
//! consolidation are explicitly rejected — partial moves stay on individual
//! add/remove findings.
//!
//! Per D-10 + D-11: one Major `namespace-member-moved` finding per moved
//! member. Moving a member from `A.B.x` to `A.D.x` breaks any consumer writing
//! the old dotted path.
//!
//! Per D-12: depth = 1 level deep, consistent with Phase 14's namespace-member
//! expansion decision. Two-dotted moves (`A.B.x` → `A.C.x`) match; anything
//! deeper falls back to individual add/remove findings.
//!
//! Per D-08 bucket-scoping (also applied here): consolidation only within the
//! same (subpath, condition, space) bucket.
//!
//! Warning 9: suppression of the base `export-removed` / `export-added`
//! findings is guarded by explicit rule identity AND structural
//! (file, line, col) position match against an `ExportSymbol` belonging to
//! a consolidated namespace-move pair (see WR-01 fix). Future rules with
//! similar message shapes cannot be accidentally swallowed.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::compare::subtype::RULE_NAMESPACE_MEMBER_MOVED;
use crate::types::{DeclarationKind, ExportKey, ExportMap, ExportSymbol, Finding, Severity};

/// IDN-02 post-pass: consolidate namespace-move cascades.
///
/// # Arguments
/// - `old`, `new`: the compared ExportMaps.
/// - `base_findings`: the findings vector produced by `compare_exports_core`
///   (no sort, no prior post-pass — post-IDN-01 is acceptable too).
///
/// # Returns
/// A new findings vector: base findings with matched export-removed /
/// export-added pairs suppressed, plus one Major `namespace-member-moved`
/// per moved member.
#[allow(dead_code)]
pub fn consolidate_namespace_moves(
    old: &ExportMap,
    new: &ExportMap,
    base_findings: Vec<Finding>,
) -> Vec<Finding> {
    // Step 1: collect dotted-name (depth 1) members only present on one side
    // and group by (bucket, parent). bucket = (subpath, condition, space).
    let old_only: Vec<&ExportKey> = old.keys().filter(|k| !new.contains_key(*k)).collect();
    let new_only: Vec<&ExportKey> = new.keys().filter(|k| !old.contains_key(*k)).collect();

    let old_groups = group_depth1_dotted(&old_only);
    let new_groups = group_depth1_dotted(&new_only);

    // moved_pairs: (old_key, new_key) where the namespace rename is confirmed
    // AND this member has a shape-equal counterpart.
    let mut moved_pairs: Vec<(ExportKey, ExportKey)> = Vec::new();

    // For each old-side namespace group, try to find a new-side namespace with
    // exact-subtree match (D-09).
    for ((bucket, old_ns), old_members) in &old_groups {
        for ((new_bucket, new_ns), new_members) in &new_groups {
            if bucket != new_bucket {
                continue;
            }
            if old_ns == new_ns {
                // Same namespace — would be per-member diff, not a namespace move.
                continue;
            }
            // Exact-subtree match: same set of member leaf names.
            if old_members.len() != new_members.len() {
                continue;
            }
            let old_leaves: std::collections::HashSet<&str> =
                old_members.iter().map(|k| leaf_of(&k.name)).collect();
            let new_leaves: std::collections::HashSet<&str> =
                new_members.iter().map(|k| leaf_of(&k.name)).collect();
            if old_leaves != new_leaves {
                continue;
            }

            // Shape-equality for each paired member (D-09).
            let mut per_member_pairs: Vec<(ExportKey, ExportKey)> = Vec::new();
            let mut all_shape_equal = true;
            for old_key in old_members {
                let leaf = leaf_of(&old_key.name);
                let new_key_match = new_members
                    .iter()
                    .find(|k| leaf_of(&k.name) == leaf)
                    .expect("leaf existence ensured by leaves set equality");
                let old_sym = match old.get(old_key) {
                    Some(s) => s,
                    None => {
                        all_shape_equal = false;
                        break;
                    }
                };
                let new_sym = match new.get(new_key_match) {
                    Some(s) => s,
                    None => {
                        all_shape_equal = false;
                        break;
                    }
                };
                if !shape_equal(old_sym, new_sym) {
                    all_shape_equal = false;
                    break;
                }
                per_member_pairs.push(((*old_key).clone(), (*new_key_match).clone()));
            }

            if all_shape_equal {
                moved_pairs.extend(per_member_pairs);
                // Each old namespace consolidates to at most one new namespace.
                break;
            }
        }
    }

    // WR-01 fix: Structural (file, line, col) HashSet for suppression.
    // Eliminates the fragile message-substring dependency (see TODO at top
    // of file) — a Finding is suppressed iff its exact source position
    // matches an `old_sym` (for removals) or `new_sym` (for additions) of a
    // consolidated namespace-move pair.
    //
    // Warning 9: still rule-type-guarded. Only export-removed / export-added
    // are eligible for suppression.
    let suppress_removed_positions: HashSet<(PathBuf, u32, u32)> = moved_pairs
        .iter()
        .filter_map(|(ok, _)| old.get(ok).map(|s| (s.file.clone(), s.line, s.col)))
        .collect();
    let suppress_added_positions: HashSet<(PathBuf, u32, u32)> = moved_pairs
        .iter()
        .filter_map(|(_, nk)| new.get(nk).map(|s| (s.file.clone(), s.line, s.col)))
        .collect();

    let mut result: Vec<Finding> = base_findings
        .into_iter()
        .filter(|f| {
            if f.rule != "export-removed" && f.rule != "export-added" {
                return true;
            }
            let is_suppressed_removal = f.rule == "export-removed"
                && suppress_removed_positions.contains(&(f.file.clone(), f.line, f.col));
            let is_suppressed_addition = f.rule == "export-added"
                && suppress_added_positions.contains(&(f.file.clone(), f.line, f.col));
            !(is_suppressed_removal || is_suppressed_addition)
        })
        .collect();

    // Emit one Major namespace-member-moved finding per moved member.
    for (old_key, new_key) in moved_pairs {
        // Emit at the new-side location (consumer impact surfaces on the new version).
        let (file, line, col) = match new.get(&new_key) {
            Some(s) => (s.file.clone(), s.line, s.col),
            None => match old.get(&old_key) {
                Some(s) => (s.file.clone(), s.line, s.col),
                None => continue,
            },
        };
        result.push(Finding {
            severity: Severity::Major,
            rule: RULE_NAMESPACE_MEMBER_MOVED,
            file,
            line,
            col,
            message: format!(
                "namespace member `{}` moved to `{}`",
                old_key.name, new_key.name
            ),
        });
    }

    result
}

/// Return the leaf (last dotted segment) of a name like `A.x` → `x`, `A.B.x` → `B.x`.
///
/// For IDN-02 depth-1 matching we only look at the immediate parent split:
/// `A.x` splits to parent `A` + leaf `x`. `A.B.x` is deeper than depth 1 and
/// is filtered out by `group_depth1_dotted`.
fn leaf_of(name: &str) -> &str {
    name.rsplit_once('.').map(|(_, leaf)| leaf).unwrap_or(name)
}

/// Group dotted-name keys by `(bucket, parent_namespace)`. Bucket preserves
/// subpath/condition/space identity; parent is the portion before the final
/// `.`. Only depth-1 dotted names (`A.x`, `Menu.Item` — exactly one `.`) are
/// included; deeper paths (`A.B.x`) fall back to individual findings per D-12.
#[allow(clippy::type_complexity)]
fn group_depth1_dotted<'a>(
    keys: &[&'a ExportKey],
) -> HashMap<
    (
        (&'a str, &'a Option<String>, &'a crate::types::SymbolSpace),
        String,
    ),
    Vec<&'a ExportKey>,
> {
    let mut groups: HashMap<
        (
            (&'a str, &'a Option<String>, &'a crate::types::SymbolSpace),
            String,
        ),
        Vec<&'a ExportKey>,
    > = HashMap::new();
    for key in keys {
        let name = &key.name;
        // Exactly one `.` — depth 1 per D-12.
        let dot_count = name.chars().filter(|c| *c == '.').count();
        if dot_count != 1 {
            continue;
        }
        let (parent, _leaf) = match name.split_once('.') {
            Some(pair) => pair,
            None => continue,
        };
        // Augmentation synthetic keys (like `augment:global:Foo`) must NOT be
        // treated as namespace members — they have special routing.
        if name.starts_with("augment:") {
            continue;
        }
        let bucket = (key.subpath.as_str(), &key.condition, &key.space);
        groups
            .entry((bucket, parent.to_string()))
            .or_default()
            .push(*key);
    }
    groups
}

/// Shape-equality for namespace members: compare declaration-kind set and
/// type signature when present. Intentionally coarse — IDN-02 is about
/// preserving consumer impact semantics (A.B.x → A.D.x breaks dotted-path
/// consumers regardless of minor type tweaks), so we require decl_kinds to
/// match and type_signature to match when BOTH sides provide one.
fn shape_equal(old_sym: &ExportSymbol, new_sym: &ExportSymbol) -> bool {
    // Both must carry the same set of DeclarationKinds (Vec<_>, order-insensitive).
    if old_sym.decl_kinds.len() != new_sym.decl_kinds.len() {
        return false;
    }
    // Simple order-insensitive comparison: each old kind appears in new.
    for k in &old_sym.decl_kinds {
        if !new_sym.decl_kinds.contains(k) {
            return false;
        }
    }
    // Type signature when present on both sides must match.
    match (&old_sym.type_signature, &new_sym.type_signature) {
        (Some(a), Some(b)) if a != b => return false,
        _ => {}
    }
    // Other fields intentionally ignored — async/generator/release_tag changes
    // are reported by their own rules.
    let _ = DeclarationKind::Unknown; // silence unused-import warning in some builds
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, SymbolSpace,
    };
    use std::path::PathBuf;

    fn k(name: &str) -> ExportKey {
        ExportKey::new(".", None, SymbolSpace::Value, name)
    }

    fn make_sym(name: &str, file: PathBuf, line: u32, kinds: Vec<DeclarationKind>) -> ExportSymbol {
        ExportSymbol {
            name: name.to_string(),
            kind: ExportKind::Named,
            file,
            line,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: kinds,
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            compound_members: Vec::new(),
        }
    }

    // Test 1: exact-subtree match, depth 1, two members — collapses to 2 Major movies.
    #[test]
    fn test_idn02_exact_subtree_two_members_coalesces() {
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            k("A.x"),
            make_sym(
                "A.x",
                PathBuf::from("x.ts"),
                1,
                vec![DeclarationKind::Variable],
            ),
        );
        old.insert(
            k("A.y"),
            make_sym(
                "A.y",
                PathBuf::from("x.ts"),
                2,
                vec![DeclarationKind::Variable],
            ),
        );
        new.insert(
            k("D.x"),
            make_sym(
                "D.x",
                PathBuf::from("x.ts"),
                10,
                vec![DeclarationKind::Variable],
            ),
        );
        new.insert(
            k("D.y"),
            make_sym(
                "D.y",
                PathBuf::from("x.ts"),
                11,
                vec![DeclarationKind::Variable],
            ),
        );

        let base = vec![
            Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: PathBuf::from("x.ts"),
                line: 1,
                col: 1,
                message: "exported symbol `A.x` was removed".into(),
            },
            Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: PathBuf::from("x.ts"),
                line: 2,
                col: 1,
                message: "exported symbol `A.y` was removed".into(),
            },
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: PathBuf::from("x.ts"),
                line: 10,
                col: 1,
                message: "exported symbol `D.x` was added".into(),
            },
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: PathBuf::from("x.ts"),
                line: 11,
                col: 1,
                message: "exported symbol `D.y` was added".into(),
            },
        ];
        let out = consolidate_namespace_moves(&old, &new, base);
        assert_eq!(
            out.iter()
                .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
                .count(),
            2,
            "IDN-02: exactly 2 Major namespace-member-moved; got: {:?}",
            out
        );
        assert_eq!(
            out.iter().filter(|f| f.rule == "export-removed").count(),
            0,
            "base export-removed must be suppressed"
        );
        assert_eq!(
            out.iter().filter(|f| f.rule == "export-added").count(),
            0,
            "base export-added must be suppressed"
        );
        // All consolidated findings must be Major per D-11.
        for f in out.iter().filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED) {
            assert_eq!(f.severity, Severity::Major);
        }
    }

    // Test 2: partial overlap → no collapse (D-09 exact-subtree rejects partial).
    #[test]
    fn test_idn02_partial_overlap_no_collapse() {
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            k("A.x"),
            make_sym(
                "A.x",
                PathBuf::from("x.ts"),
                1,
                vec![DeclarationKind::Variable],
            ),
        );
        old.insert(
            k("A.y"),
            make_sym(
                "A.y",
                PathBuf::from("x.ts"),
                2,
                vec![DeclarationKind::Variable],
            ),
        );
        // New has only D.x (missing D.y) — partial.
        new.insert(
            k("D.x"),
            make_sym(
                "D.x",
                PathBuf::from("x.ts"),
                10,
                vec![DeclarationKind::Variable],
            ),
        );
        let out = consolidate_namespace_moves(&old, &new, vec![]);
        assert_eq!(
            out.iter()
                .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
                .count(),
            0
        );
    }

    // Test 3: extras on one side → no collapse.
    #[test]
    fn test_idn02_extras_on_one_side_no_collapse() {
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            k("A.x"),
            make_sym(
                "A.x",
                PathBuf::from("x.ts"),
                1,
                vec![DeclarationKind::Variable],
            ),
        );
        // New has D.x AND D.z — more members than old.
        new.insert(
            k("D.x"),
            make_sym(
                "D.x",
                PathBuf::from("x.ts"),
                10,
                vec![DeclarationKind::Variable],
            ),
        );
        new.insert(
            k("D.z"),
            make_sym(
                "D.z",
                PathBuf::from("x.ts"),
                11,
                vec![DeclarationKind::Variable],
            ),
        );
        let out = consolidate_namespace_moves(&old, &new, vec![]);
        assert_eq!(
            out.iter()
                .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
                .count(),
            0
        );
    }

    // Test 4: depth 2 → no collapse (D-12).
    #[test]
    fn test_idn02_depth_2_no_collapse() {
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            k("A.B.x"),
            make_sym(
                "A.B.x",
                PathBuf::from("x.ts"),
                1,
                vec![DeclarationKind::Variable],
            ),
        );
        new.insert(
            k("C.B.x"),
            make_sym(
                "C.B.x",
                PathBuf::from("x.ts"),
                10,
                vec![DeclarationKind::Variable],
            ),
        );
        let out = consolidate_namespace_moves(&old, &new, vec![]);
        assert_eq!(
            out.iter()
                .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
                .count(),
            0,
            "D-12: depth > 1 falls back to individual findings"
        );
    }

    // Test 5: different bucket → no collapse.
    #[test]
    fn test_idn02_different_bucket_no_collapse() {
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "A.x"),
            make_sym(
                "A.x",
                PathBuf::from("x.ts"),
                1,
                vec![DeclarationKind::Variable],
            ),
        );
        // Different subpath.
        new.insert(
            ExportKey::new("./sub", None, SymbolSpace::Value, "D.x"),
            make_sym(
                "D.x",
                PathBuf::from("x.ts"),
                10,
                vec![DeclarationKind::Variable],
            ),
        );
        let out = consolidate_namespace_moves(&old, &new, vec![]);
        assert_eq!(
            out.iter()
                .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
                .count(),
            0,
            "D-08 bucket-scoping: cross-subpath cannot be a namespace move"
        );
    }

    // Test 6: shape-different members → no collapse.
    #[test]
    fn test_idn02_shape_different_no_collapse() {
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            k("A.x"),
            make_sym(
                "A.x",
                PathBuf::from("x.ts"),
                1,
                vec![DeclarationKind::Variable],
            ),
        );
        // new D.x has Function kind — shape-mismatch with old Variable.
        new.insert(
            k("D.x"),
            make_sym(
                "D.x",
                PathBuf::from("x.ts"),
                10,
                vec![DeclarationKind::Function],
            ),
        );
        let out = consolidate_namespace_moves(&old, &new, vec![]);
        assert_eq!(
            out.iter()
                .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
                .count(),
            0,
            "D-09: shape inequality rejects consolidation"
        );
    }

    // Test 7 (Warning 9): rule-type guard prevents cross-contamination.
    #[test]
    fn test_idn02_warning9_rule_type_guard_prevents_cross_contamination() {
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            k("A.x"),
            make_sym(
                "A.x",
                PathBuf::from("x.ts"),
                1,
                vec![DeclarationKind::Variable],
            ),
        );
        old.insert(
            k("A.y"),
            make_sym(
                "A.y",
                PathBuf::from("x.ts"),
                2,
                vec![DeclarationKind::Variable],
            ),
        );
        new.insert(
            k("D.x"),
            make_sym(
                "D.x",
                PathBuf::from("x.ts"),
                10,
                vec![DeclarationKind::Variable],
            ),
        );
        new.insert(
            k("D.y"),
            make_sym(
                "D.y",
                PathBuf::from("x.ts"),
                11,
                vec![DeclarationKind::Variable],
            ),
        );

        let base = vec![
            Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: PathBuf::from("x.ts"),
                line: 1,
                col: 1,
                message: "exported symbol `A.x` was removed".into(),
            },
            Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: PathBuf::from("x.ts"),
                line: 2,
                col: 1,
                message: "exported symbol `A.y` was removed".into(),
            },
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: PathBuf::from("x.ts"),
                line: 10,
                col: 1,
                message: "exported symbol `D.x` was added".into(),
            },
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: PathBuf::from("x.ts"),
                line: 11,
                col: 1,
                message: "exported symbol `D.y` was added".into(),
            },
            // Unrelated rule that happens to mention A.x in its message — must survive.
            Finding {
                severity: Severity::Warning,
                rule: "hypothetical-audit-rule",
                file: PathBuf::from("x.ts"),
                line: 1,
                col: 1,
                message: "observation about `A.x` that is not a removal".into(),
            },
        ];
        let out = consolidate_namespace_moves(&old, &new, base);
        assert_eq!(
            out.iter()
                .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
                .count(),
            2
        );
        assert!(
            out.iter().any(|f| f.rule == "hypothetical-audit-rule"),
            "Warning 9: rule-type guard must preserve non-export-rule findings"
        );
    }
}
