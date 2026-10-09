// Enum diff: detect breaking changes in TypeScript enum declarations.
//
// Covers ENUM-01..05:
// ENUM-01: enum member removed (enum-member-removed)
// ENUM-02: enum member value changed (enum-member-value-changed)
// ENUM-03: enum member reordered causing value shift (enum-member-reordered)
// ENUM-04: const enum to regular enum conversion (enum-const-to-regular)
// ENUM-05: numeric enum changed to string enum (enum-numeric-to-string)
//
// Key design decisions:
//: Evaluate actual numeric values (auto-increment). Flag reordering only when
// computed values actually shift. Explicit values identical after reorder = not flagged.
//: Non-evaluable initializers: compare initializer expression text.
// Identical text = unchanged; different text or absent = breaking.
// ENUM-02 and ENUM-03 do not double-fire for the same member:
// ENUM-02 fires for explicit value changes; ENUM-03 fires only for implicit value
// shifts caused by reordering (raw_value: None in both old and new).

use crate::core::findings::{Finding, Severity};
use crate::languages::typescript::compare::rules::{
    RULE_ENUM_CONST_TO_REGULAR, RULE_ENUM_MEMBER_ADDED, RULE_ENUM_MEMBER_REMOVED,
    RULE_ENUM_MEMBER_REORDERED, RULE_ENUM_MEMBER_VALUE_CHANGED, RULE_ENUM_NUMERIC_TO_STRING,
    RULE_ENUM_REGULAR_TO_CONST, RULE_ENUM_VALUE_SHIFTED,
};
use crate::languages::typescript::model as ts_model;
use crate::languages::typescript::model::{EnumShape, EnumValueKind};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Compare two enum shapes and return a list of findings for breaking changes.
///
/// Checks are performed in this order:
/// 1. ENUM-04: const-to-regular conversion (structural change)
/// 2. ENUM-05: numeric-to-string kind change (whole-enum type change)
/// 3. ENUM-01: member removed
/// 4. ENUM-02: member value changed (explicit value changes)
/// 5. ENUM-03: member reordered with implicit value shift (only if ENUM-05 did not fire)
pub fn compare_enum_shapes(
    old: &EnumShape,
    new: &EnumShape,
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    // ── ENUM-04: const enum → regular enum ──────────────────────────────────
    // This is a structural change independent of member values.
    if old.is_const && !new.is_const {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_ENUM_CONST_TO_REGULAR,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "enum `{}` changed from const enum to regular enum",
                symbol_name
            ),
        });
    }

    // ── ENUM-04b: regular enum -> const enum ──────────────────────────────────
    // Const enums are inlined at compile time — no runtime object exists.
    // Code using reverse mapping (MyEnum[0]) or typeof MyEnum breaks.
    if !old.is_const && new.is_const {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_ENUM_REGULAR_TO_CONST,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "enum `{}` changed from regular enum to const enum",
                symbol_name
            ),
        });
    }

    // ── ENUM-05: numeric → string enum ──────────────────────────────────────
    // Check if all old members are Numeric and all new members are StringLiteral.
    // This whole-enum type change is checked before per-member comparison to avoid
    // double-firing with ENUM-02 for individual members.
    let numeric_to_string = {
        let all_old_numeric = !old.members.is_empty()
            && old
                .members
                .iter()
                .all(|m| m.value_kind == EnumValueKind::Numeric);
        let all_new_string = !new.members.is_empty()
            && new
                .members
                .iter()
                .all(|m| m.value_kind == EnumValueKind::StringLiteral);
        all_old_numeric && all_new_string
    };

    if numeric_to_string {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_ENUM_NUMERIC_TO_STRING,
            file: file.to_path_buf(),
            line,
            col,
            message: format!("enum `{}` changed from numeric to string enum", symbol_name),
        });
    }

    // ── Build lookups ────────────────────────────────────────────────────────
    // Index old and new members by name for O(1) lookup.
    let old_by_name: HashMap<&str, (usize, &ts_model::EnumMember)> = old
        .members
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.as_str(), (i, m)))
        .collect();

    let new_by_name: HashMap<&str, (usize, &ts_model::EnumMember)> = new
        .members
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.as_str(), (i, m)))
        .collect();

    let new_names: HashSet<&str> = new_by_name.keys().copied().collect();

    // ── ENUM-01: member removed ──────────────────────────────────────────────
    for old_member in &old.members {
        if !new_names.contains(old_member.name.as_str()) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_ENUM_MEMBER_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "enum member `{}` was removed from `{}`",
                    old_member.name, symbol_name
                ),
            });
        }
    }

    // ── BEH-04: enum member added (Warning) ─────────────────────────────────
    // always Warning, regardless of const/regular.
    // Any new member may break exhaustive switch statements in consumers.
    let old_names: HashSet<&str> = old_by_name.keys().copied().collect();
    for new_member in &new.members {
        if !old_names.contains(new_member.name.as_str()) {
            findings.push(Finding {
                severity: Severity::Warning,
                rule: RULE_ENUM_MEMBER_ADDED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "enum member `{}` was added to `{}` -- may break exhaustive switch statements",
                    new_member.name, symbol_name
                ),
            });
        }
    }

    // ── ENUM-02: member value changed ────────────────────────────────────────
    // Track members where ENUM-02 already fired (to avoid ENUM-03 double-fire).
    //
    // Key heuristic: if both old and new have raw_value: None (implicit),
    // any value change came from reordering -> handled by ENUM-03, not ENUM-02.
    // ENUM-02 only fires when at least one side has an explicit raw_value.
    let mut enum02_fired: HashSet<&str> = HashSet::new();

    for old_member in &old.members {
        let name = old_member.name.as_str();
        let Some(&(_, new_member)) = new_by_name.get(name) else {
            continue; // Already handled by ENUM-01
        };

        // If both are implicitly numbered (no raw_value), skip ENUM-02 —
        // any value shift from reordering is covered by ENUM-03.
        if old_member.raw_value.is_none() && new_member.raw_value.is_none() {
            continue;
        }

        let value_changed = match (old_member.computed_value, new_member.computed_value) {
            // Both have computed numeric values — compare i64
            (Some(old_val), Some(new_val)) => {
                if old_val != new_val {
                    Some((format!("{}", old_val), format!("{}", new_val)))
                } else {
                    None
                }
            }
            // At least one is non-evaluable, so compare the raw text.
            _ => {
                let old_raw = old_member.raw_value.as_deref();
                let new_raw = new_member.raw_value.as_deref();
                if old_raw != new_raw {
                    let old_display = old_raw.unwrap_or("<implicit>");
                    let new_display = new_raw.unwrap_or("<implicit>");
                    Some((old_display.to_string(), new_display.to_string()))
                } else {
                    None
                }
            }
        };

        if let Some((old_val, new_val)) = value_changed {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_ENUM_MEMBER_VALUE_CHANGED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "enum member `{}` value changed from {} to {} in `{}`",
                    name, old_val, new_val, symbol_name
                ),
            });
            enum02_fired.insert(name);
        }
    }

    // ── ENUM-03 + BEH-03: reordering vs insertion-induced value shift ────────
    // Mutual exclusion: a member fires EITHER ENUM-03 OR BEH-03, never both.
    //
    // ENUM-03 (Major): member was reordered among existing members
    // BEH-03 (Warning): member's implicit value shifted due to new member insertion
    //
    // Detection: compute relative position among members present in BOTH old and new.
    // If relative position changed -> reordered (ENUM-03).
    // If relative position same but computed_value changed -> shifted by insertion (BEH-03).
    if !numeric_to_string {
        // Build ordered list of member names present in both old and new
        let old_shared_order: Vec<&str> = old
            .members
            .iter()
            .filter(|m| new_by_name.contains_key(m.name.as_str()))
            .map(|m| m.name.as_str())
            .collect();
        let new_shared_order: Vec<&str> = new
            .members
            .iter()
            .filter(|m| old_by_name.contains_key(m.name.as_str()))
            .map(|m| m.name.as_str())
            .collect();

        // Build relative position maps (position among shared members only)
        let old_relative: HashMap<&str, usize> = old_shared_order
            .iter()
            .enumerate()
            .map(|(i, &name)| (name, i))
            .collect();
        let new_relative: HashMap<&str, usize> = new_shared_order
            .iter()
            .enumerate()
            .map(|(i, &name)| (name, i))
            .collect();

        for old_member in &old.members {
            let name = old_member.name.as_str();

            // Skip if ENUM-02 already fired for this member
            if enum02_fired.contains(name) {
                continue;
            }

            let Some(&(_, new_member)) = new_by_name.get(name) else {
                continue; // Removed — handled by ENUM-01
            };

            // Both must have implicit values (no explicit raw_value)
            if old_member.raw_value.is_some() || new_member.raw_value.is_some() {
                continue;
            }

            // Computed values must have actually changed
            let (old_val, new_val) = match (old_member.computed_value, new_member.computed_value) {
                (Some(ov), Some(nv)) if ov != nv => (ov, nv),
                _ => continue,
            };

            // Determine if the member was reordered (ENUM-03) or shifted by insertion (BEH-03)
            let old_rel = old_relative.get(name);
            let new_rel = new_relative.get(name);

            let relative_position_changed = match (old_rel, new_rel) {
                (Some(&or), Some(&nr)) => or != nr,
                _ => false,
            };

            if relative_position_changed {
                // ENUM-03: reordered among existing members
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_ENUM_MEMBER_REORDERED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "enum member `{}` was reordered causing value change from {} to {} in `{}`",
                        name, old_val, new_val, symbol_name
                    ),
                });
            } else {
                // BEH-03: value shifted by insertion of new member
                findings.push(Finding {
                    severity: Severity::Warning,
                    rule: RULE_ENUM_VALUE_SHIFTED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "enum member `{}` implicit value shifted from {} to {} in `{}` due to member insertion",
                        name, old_val, new_val, symbol_name
                    ),
                });
            }
        }
    }

    findings
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
