//! INF-05 (Phase 19): triple-slash directive diff emission.
//!
//! Consumed by `compare_exports_core` in `src/compare/mod.rs`, which
//! iterates the union of files in `old.directives` and `new.directives`
//! and feeds each per-file directive list pair through `compare_directives`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::compare::subtype::{
    RULE_DIRECTIVE_ADDED, RULE_DIRECTIVE_CHANGED, RULE_DIRECTIVE_REMOVED,
    RULE_DIRECTIVE_REORDERED,
};
use crate::discover::comment_scan::DirectiveRecord;
use crate::types::{Finding, Severity};

/// Compare two ordered directive lists from the same file pair.
///
/// Diff algorithm (post-research decision D-04, Minor on reorder):
///
/// 0. Fast-path: if `old` and `new` have the same multiset of (name, attrs)
///    tuples but a different document order, emit exactly one Minor
///    `directive-reordered` and stop. Pure reorders never surface as 1
///    `directive-added` + 1 `directive-removed`.
/// 1. Exact-match pairing: if `old[i]` equals `new[j]` element-wise (name +
///    attrs), they are the "same" directive. Pair them off greedily.
/// 2. Fuzzy-match by `(name, attrs.keys())` on the unpaired residue:
///    same name and same attribute-key set but at least one value
///    differing — one `directive-changed` per paired residue.
/// 3. Unpaired residue after step 2: `directive-added` for those only in
///    new, `directive-removed` for those only in old.
///
/// All findings are reported at `(line 1, col 1)` of `file`. Directives
/// are a file-level concept and we intentionally do not track per-directive
/// line/col beyond the `start_byte` used for ordering.
pub fn compare_directives(
    old: &[DirectiveRecord],
    new: &[DirectiveRecord],
    file: &Path,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    // Step 0: pure-reorder fast path.
    // If the sorted lists are equal element-wise but the document-ordered
    // lists differ, the only change is order.
    if old.len() == new.len() && !old.is_empty() {
        let mut old_sorted: Vec<&DirectiveRecord> = old.iter().collect();
        let mut new_sorted: Vec<&DirectiveRecord> = new.iter().collect();
        old_sorted.sort_by(|a, b| (&a.name, &a.attrs).cmp(&(&b.name, &b.attrs)));
        new_sorted.sort_by(|a, b| (&a.name, &a.attrs).cmp(&(&b.name, &b.attrs)));
        let sorted_equal = old_sorted
            .iter()
            .zip(new_sorted.iter())
            .all(|(a, b)| a.name == b.name && a.attrs == b.attrs);
        let ordered_equal = old
            .iter()
            .zip(new.iter())
            .all(|(a, b)| a.name == b.name && a.attrs == b.attrs);
        if sorted_equal && !ordered_equal {
            findings.push(Finding {
                severity: Severity::Minor,
                rule: RULE_DIRECTIVE_REORDERED,
                file: file.to_path_buf(),
                line: 1,
                col: 1,
                message: format!(
                    "triple-slash directives reordered ({} directives)",
                    old.len()
                ),
            });
            return findings;
        }
    }

    // Step 1: exact-match pairing.
    let mut old_used = vec![false; old.len()];
    let mut new_used = vec![false; new.len()];
    for (i, o) in old.iter().enumerate() {
        for (j, n) in new.iter().enumerate() {
            if !new_used[j] && !old_used[i] && o.name == n.name && o.attrs == n.attrs {
                old_used[i] = true;
                new_used[j] = true;
                break;
            }
        }
    }

    // Collect unpaired residuals (preserve document order for deterministic output).
    let old_residual: Vec<&DirectiveRecord> = old
        .iter()
        .enumerate()
        .filter_map(|(i, d)| if !old_used[i] { Some(d) } else { None })
        .collect();
    let new_residual: Vec<&DirectiveRecord> = new
        .iter()
        .enumerate()
        .filter_map(|(j, d)| if !new_used[j] { Some(d) } else { None })
        .collect();

    // Step 2: fuzzy-match by (name, attrs.keys()).
    // Same name + same set of attribute keys + at least one value diff → change.
    let mut old_changed = vec![false; old_residual.len()];
    let mut new_changed = vec![false; new_residual.len()];
    for (i, o) in old_residual.iter().enumerate() {
        for (j, n) in new_residual.iter().enumerate() {
            if !new_changed[j] && !old_changed[i] && o.name == n.name {
                let o_keys: Vec<&String> = o.attrs.keys().collect();
                let n_keys: Vec<&String> = n.attrs.keys().collect();
                if o_keys == n_keys {
                    old_changed[i] = true;
                    new_changed[j] = true;
                    findings.push(Finding {
                        severity: Severity::Major,
                        rule: RULE_DIRECTIVE_CHANGED,
                        file: file.to_path_buf(),
                        line: 1,
                        col: 1,
                        message: format!(
                            "triple-slash directive `{}` attribute(s) changed: {} -> {}",
                            o.name,
                            render_attrs(&o.attrs),
                            render_attrs(&n.attrs)
                        ),
                    });
                    break;
                }
            }
        }
    }

    // Step 3: report true add/remove for still-unpaired residuals.
    for (i, o) in old_residual.iter().enumerate() {
        if !old_changed[i] {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_DIRECTIVE_REMOVED,
                file: file.to_path_buf(),
                line: 1,
                col: 1,
                message: format!(
                    "triple-slash directive removed: <{} {}/>",
                    o.name,
                    render_attrs(&o.attrs)
                ),
            });
        }
    }
    for (j, n) in new_residual.iter().enumerate() {
        if !new_changed[j] {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_DIRECTIVE_ADDED,
                file: file.to_path_buf(),
                line: 1,
                col: 1,
                message: format!(
                    "triple-slash directive added: <{} {}/>",
                    n.name,
                    render_attrs(&n.attrs)
                ),
            });
        }
    }

    findings
}

fn render_attrs(attrs: &BTreeMap<String, String>) -> String {
    attrs
        .iter()
        .map(|(k, v)| format!("{}=\"{}\"", k, v))
        .collect::<Vec<_>>()
        .join(" ")
}
