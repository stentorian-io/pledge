// Enum diff: detect breaking changes in TypeScript enum declarations.
//
// Covers ENUM-01..05:
//   ENUM-01: enum member removed (enum-member-removed)
//   ENUM-02: enum member value changed (enum-member-value-changed)
//   ENUM-03: enum member reordered causing value shift (enum-member-reordered)
//   ENUM-04: const enum to regular enum conversion (enum-const-to-regular)
//   ENUM-05: numeric enum changed to string enum (enum-numeric-to-string)
//
// Key design decisions:
//   D-04: Evaluate actual numeric values (auto-increment). Flag reordering only when
//         computed values actually shift. Explicit values identical after reorder = not flagged.
//   D-05: Non-evaluable initializers: compare initializer expression text.
//         Identical text = unchanged; different text or absent = breaking.
//   ENUM-02 and ENUM-03 do not double-fire for the same member:
//         ENUM-02 fires for explicit value changes; ENUM-03 fires only for implicit value
//         shifts caused by reordering (raw_value: None in both old and new).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::types::{EnumShape, EnumValueKind, Finding, Severity};

use super::subtype::{
    RULE_ENUM_CONST_TO_REGULAR, RULE_ENUM_MEMBER_ADDED, RULE_ENUM_MEMBER_REMOVED,
    RULE_ENUM_MEMBER_REORDERED, RULE_ENUM_MEMBER_VALUE_CHANGED, RULE_ENUM_NUMERIC_TO_STRING,
    RULE_ENUM_VALUE_SHIFTED,
};

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
            message: format!(
                "enum `{}` changed from numeric to string enum",
                symbol_name
            ),
        });
    }

    // ── Build lookups ────────────────────────────────────────────────────────
    // Index old and new members by name for O(1) lookup.
    let old_by_name: HashMap<&str, (usize, &crate::types::EnumMember)> = old
        .members
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.as_str(), (i, m)))
        .collect();

    let new_by_name: HashMap<&str, (usize, &crate::types::EnumMember)> = new
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
    // Per D-05: always Warning, regardless of const/regular.
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
    // Key heuristic (from plan): if both old and new have raw_value: None (implicit),
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
            // At least one is non-evaluable — compare raw text per D-05
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
mod tests {
    use super::*;
    use crate::types::{EnumMember, EnumShape, EnumValueKind};
    use std::path::PathBuf;

    fn make_file() -> PathBuf {
        PathBuf::from("test.ts")
    }

    fn numeric_member(name: &str, computed: i64) -> EnumMember {
        EnumMember {
            name: name.to_string(),
            raw_value: None,
            computed_value: Some(computed),
            value_kind: EnumValueKind::Numeric,
        }
    }

    fn explicit_numeric_member(name: &str, raw: &str, computed: i64) -> EnumMember {
        EnumMember {
            name: name.to_string(),
            raw_value: Some(raw.to_string()),
            computed_value: Some(computed),
            value_kind: EnumValueKind::Numeric,
        }
    }

    fn string_member(name: &str, value: &str) -> EnumMember {
        EnumMember {
            name: name.to_string(),
            raw_value: Some(value.to_string()),
            computed_value: None,
            value_kind: EnumValueKind::StringLiteral,
        }
    }

    fn other_expr_member(name: &str, raw: &str) -> EnumMember {
        EnumMember {
            name: name.to_string(),
            raw_value: Some(raw.to_string()),
            computed_value: None,
            value_kind: EnumValueKind::OtherExpr,
        }
    }

    fn enum_shape(members: Vec<EnumMember>, is_const: bool) -> EnumShape {
        EnumShape { members, is_const }
    }

    fn run(old: &EnumShape, new: &EnumShape) -> Vec<Finding> {
        compare_enum_shapes(old, new, "MyEnum", &make_file(), 1, 1)
    }

    // ── ENUM-01: member removed ──────────────────────────────────────────────

    #[test]
    fn test_enum01_member_removed() {
        // old has {A, B, C}, new has {A, C} — B is removed
        let old = enum_shape(
            vec![
                numeric_member("A", 0),
                numeric_member("B", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let new = enum_shape(
            vec![numeric_member("A", 0), numeric_member("C", 1)],
            false,
        );
        let findings = run(&old, &new);
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_REMOVED
                && f.message.contains("B")
                && f.severity == Severity::Major),
            "Expected enum-member-removed for B, got: {:?}",
            findings
        );
    }

    // ── ENUM-02: value changed (string) ──────────────────────────────────────

    #[test]
    fn test_enum02_string_value_changed() {
        // old A="hello", new A="world" -> Finding with rule "enum-member-value-changed"
        let old = enum_shape(vec![string_member("A", "\"hello\"")], false);
        let new = enum_shape(vec![string_member("A", "\"world\"")], false);
        let findings = run(&old, &new);
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED
                && f.message.contains("A")
                && f.severity == Severity::Major),
            "Expected enum-member-value-changed for string change, got: {:?}",
            findings
        );
        let f = findings
            .iter()
            .find(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED)
            .unwrap();
        assert!(
            f.message.contains("hello") && f.message.contains("world"),
            "Message should include old and new values: {}",
            f.message
        );
    }

    // ── ENUM-02: value changed (numeric) ─────────────────────────────────────

    #[test]
    fn test_enum02_numeric_value_changed() {
        // old A=1, new A=2 -> Finding with rule "enum-member-value-changed"
        let old = enum_shape(vec![explicit_numeric_member("A", "1", 1)], false);
        let new = enum_shape(vec![explicit_numeric_member("A", "2", 2)], false);
        let findings = run(&old, &new);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED && f.message.contains("A")),
            "Expected enum-member-value-changed for numeric change, got: {:?}",
            findings
        );
    }

    // ── ENUM-03: reordered with implicit value shift ──────────────────────────

    #[test]
    fn test_enum03_reorder_with_value_shift() {
        // old {A, B, C} (implicit values 0,1,2), new {B, A, C} (implicit values 0,1,2)
        // After reorder: A was at index 0 (value 0), now at index 1 (value 1) -> shift
        //                B was at index 1 (value 1), now at index 0 (value 0) -> shift
        let old = enum_shape(
            vec![
                numeric_member("A", 0),
                numeric_member("B", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let new = enum_shape(
            vec![
                numeric_member("B", 0),
                numeric_member("A", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let findings = run(&old, &new);
        // A: was 0, now 1 -> reordered
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED
                && f.message.contains("A")
                && f.severity == Severity::Major),
            "Expected enum-member-reordered for A, got: {:?}",
            findings
        );
        // B: was 1, now 0 -> reordered
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED && f.message.contains("B")),
            "Expected enum-member-reordered for B, got: {:?}",
            findings
        );
    }

    // ── ENUM-03 negative: explicit values unchanged = no finding ──────────────

    #[test]
    fn test_enum03_negative_explicit_values_unchanged() {
        // old {A=10, B=20}, new {B=20, A=10} — explicit values identical after reordering
        // Per D-04: should NOT be flagged
        let old = enum_shape(
            vec![
                explicit_numeric_member("A", "10", 10),
                explicit_numeric_member("B", "20", 20),
            ],
            false,
        );
        let new = enum_shape(
            vec![
                explicit_numeric_member("B", "20", 20),
                explicit_numeric_member("A", "10", 10),
            ],
            false,
        );
        let findings = run(&old, &new);
        assert!(
            !findings
                .iter()
                .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED),
            "Expected no enum-member-reordered for identical explicit values, got: {:?}",
            findings
        );
        assert!(
            findings.is_empty(),
            "Expected no findings for explicit value reorder, got: {:?}",
            findings
        );
    }

    // ── ENUM-04: const to regular ─────────────────────────────────────────────

    #[test]
    fn test_enum04_const_to_regular() {
        // old is_const=true, new is_const=false -> Finding with rule "enum-const-to-regular"
        let old = enum_shape(vec![numeric_member("A", 0)], true);
        let new = enum_shape(vec![numeric_member("A", 0)], false);
        let findings = run(&old, &new);
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_CONST_TO_REGULAR
                && f.severity == Severity::Major),
            "Expected enum-const-to-regular finding, got: {:?}",
            findings
        );
    }

    // ── ENUM-05: numeric to string ────────────────────────────────────────────

    #[test]
    fn test_enum05_numeric_to_string() {
        // old members all Numeric kind, new members all StringLiteral kind
        let old = enum_shape(
            vec![numeric_member("A", 0), numeric_member("B", 1)],
            false,
        );
        let new = enum_shape(
            vec![string_member("A", "\"foo\""), string_member("B", "\"bar\"")],
            false,
        );
        let findings = run(&old, &new);
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_NUMERIC_TO_STRING
                && f.severity == Severity::Major),
            "Expected enum-numeric-to-string finding, got: {:?}",
            findings
        );
    }

    // ── No findings when identical ────────────────────────────────────────────

    #[test]
    fn test_no_findings_when_identical() {
        let shape = enum_shape(
            vec![
                numeric_member("A", 0),
                explicit_numeric_member("B", "10", 10),
                string_member("C", "\"hello\""),
            ],
            false,
        );
        let findings = run(&shape, &shape);
        assert!(
            findings.is_empty(),
            "Expected no findings for identical enums, got: {:?}",
            findings
        );
    }

    // ── ENUM-02 with non-evaluable initializers (D-05) ────────────────────────

    #[test]
    fn test_enum02_non_evaluable_text_differs() {
        // old A="Foo.Bar", new A="Foo.Baz" — text differs per D-05 -> Finding
        let old = enum_shape(vec![other_expr_member("A", "Foo.Bar")], false);
        let new = enum_shape(vec![other_expr_member("A", "Foo.Baz")], false);
        let findings = run(&old, &new);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED && f.message.contains("A")),
            "Expected enum-member-value-changed for non-evaluable text change, got: {:?}",
            findings
        );
        let f = findings
            .iter()
            .find(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED)
            .unwrap();
        assert!(
            f.message.contains("Foo.Bar") && f.message.contains("Foo.Baz"),
            "Message should include old and new values: {}",
            f.message
        );
    }

    // ── ENUM-02 non-evaluable identical text = no finding ────────────────────

    #[test]
    fn test_enum02_non_evaluable_text_same_no_finding() {
        // old A="Foo.Bar", new A="Foo.Bar" — text same per D-05 -> No finding
        let old = enum_shape(vec![other_expr_member("A", "Foo.Bar")], false);
        let new = enum_shape(vec![other_expr_member("A", "Foo.Bar")], false);
        let findings = run(&old, &new);
        assert!(
            findings.is_empty(),
            "Expected no findings for identical non-evaluable values, got: {:?}",
            findings
        );
    }

    // ── BEH-04: enum member added ────────────────────────────────────────────

    #[test]
    fn test_beh04_member_added() {
        let old = enum_shape(
            vec![numeric_member("A", 0), numeric_member("B", 1)],
            false,
        );
        let new = enum_shape(
            vec![
                numeric_member("A", 0),
                numeric_member("B", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let findings = run(&old, &new);
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_ADDED
                && f.message.contains("C")
                && f.severity == Severity::Warning),
            "Expected enum-member-added Warning for C, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_beh04_multiple_members_added() {
        let old = enum_shape(vec![numeric_member("A", 0)], false);
        let new = enum_shape(
            vec![
                numeric_member("A", 0),
                numeric_member("B", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let findings = run(&old, &new);
        let added: Vec<_> = findings.iter().filter(|f| f.rule == RULE_ENUM_MEMBER_ADDED).collect();
        assert_eq!(added.len(), 2, "Expected 2 enum-member-added findings, got: {:?}", added);
    }

    #[test]
    fn test_beh04_no_finding_when_identical() {
        let shape = enum_shape(
            vec![numeric_member("A", 0), numeric_member("B", 1)],
            false,
        );
        let findings = run(&shape, &shape);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_ADDED),
            "Expected no enum-member-added findings for identical enum"
        );
    }

    #[test]
    fn test_beh04_const_enum_member_added() {
        // D-05: Warning regardless of const/regular
        let old = enum_shape(vec![numeric_member("A", 0)], true);
        let new = enum_shape(
            vec![numeric_member("A", 0), numeric_member("B", 1)],
            true,
        );
        let findings = run(&old, &new);
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_ADDED
                && f.severity == Severity::Warning),
            "Expected enum-member-added Warning for const enum, got: {:?}",
            findings
        );
    }

    // ── BEH-03: enum value shifted by insertion ──────────────────────────────

    #[test]
    fn test_beh03_value_shifted_by_insertion() {
        // old: {A=0, B=1, C=2} (all implicit)
        // new: {X=0, A=1, B=2, C=3} (X inserted at front)
        // A, B, C shifted by insertion -> BEH-03 Warning (not ENUM-03)
        let old = enum_shape(
            vec![
                numeric_member("A", 0),
                numeric_member("B", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let new = enum_shape(
            vec![
                numeric_member("X", 0),
                numeric_member("A", 1),
                numeric_member("B", 2),
                numeric_member("C", 3),
            ],
            false,
        );
        let findings = run(&old, &new);
        // A, B, C should all fire BEH-03 (shifted by insertion of X)
        let shifted: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == RULE_ENUM_VALUE_SHIFTED)
            .collect();
        assert_eq!(shifted.len(), 3, "Expected 3 enum-value-shifted findings, got: {:?}", shifted);
        assert!(shifted.iter().all(|f| f.severity == Severity::Warning));
        // ENUM-03 should NOT fire (no reordering)
        assert!(
            !findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED),
            "ENUM-03 should not fire for insertion-shifted members"
        );
    }

    #[test]
    fn test_beh03_no_double_fire_with_enum03() {
        // old: {A=0, B=1, C=2} (implicit)
        // new: {B=0, A=1, C=2} (reordered A and B — not insertion)
        // A and B should fire ENUM-03 (Major), NOT BEH-03
        let old = enum_shape(
            vec![
                numeric_member("A", 0),
                numeric_member("B", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let new = enum_shape(
            vec![
                numeric_member("B", 0),
                numeric_member("A", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let findings = run(&old, &new);
        // ENUM-03 fires for A and B
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED && f.message.contains("A")),
            "Expected ENUM-03 for A"
        );
        assert!(
            findings.iter().any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED && f.message.contains("B")),
            "Expected ENUM-03 for B"
        );
        // BEH-03 should NOT fire
        assert!(
            !findings.iter().any(|f| f.rule == RULE_ENUM_VALUE_SHIFTED),
            "BEH-03 should not fire when ENUM-03 fires, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_beh03_explicit_values_not_affected() {
        // old: {A=10, B=20} (explicit)
        // new: {X=0, A=10, B=20} (insertion, but A and B have explicit values)
        // No BEH-03 because values are explicit (unchanged)
        let old = enum_shape(
            vec![
                explicit_numeric_member("A", "10", 10),
                explicit_numeric_member("B", "20", 20),
            ],
            false,
        );
        let new = enum_shape(
            vec![
                numeric_member("X", 0),
                explicit_numeric_member("A", "10", 10),
                explicit_numeric_member("B", "20", 20),
            ],
            false,
        );
        let findings = run(&old, &new);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_ENUM_VALUE_SHIFTED),
            "BEH-03 should not fire for explicit-value members, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_beh03_unshifted_member_no_finding() {
        // old: {A=0, B=1} (implicit)
        // new: {A=0, B=1, C=2} (C added at end — A and B not shifted)
        // A and B values unchanged -> no BEH-03
        let old = enum_shape(
            vec![numeric_member("A", 0), numeric_member("B", 1)],
            false,
        );
        let new = enum_shape(
            vec![
                numeric_member("A", 0),
                numeric_member("B", 1),
                numeric_member("C", 2),
            ],
            false,
        );
        let findings = run(&old, &new);
        assert!(
            !findings.iter().any(|f| f.rule == RULE_ENUM_VALUE_SHIFTED),
            "BEH-03 should not fire when values are unchanged, got: {:?}",
            findings
        );
    }

    // ── No double-fire: ENUM-02 and ENUM-03 are mutually exclusive ────────────

    #[test]
    fn test_no_double_fire_enum02_and_enum03() {
        // If a member had explicit values that changed, ENUM-02 fires, not ENUM-03.
        // old {A=1, B=2}, new {B=5, A=3} — explicit values changed
        let old = enum_shape(
            vec![
                explicit_numeric_member("A", "1", 1),
                explicit_numeric_member("B", "2", 2),
            ],
            false,
        );
        let new = enum_shape(
            vec![
                explicit_numeric_member("B", "5", 5),
                explicit_numeric_member("A", "3", 3),
            ],
            false,
        );
        let findings = run(&old, &new);
        // ENUM-02 should fire for both A and B (explicit value changes)
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED && f.message.contains("A")),
            "Expected ENUM-02 for A"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.rule == RULE_ENUM_MEMBER_VALUE_CHANGED && f.message.contains("B")),
            "Expected ENUM-02 for B"
        );
        // ENUM-03 should NOT fire since ENUM-02 handled these
        assert!(
            !findings
                .iter()
                .any(|f| f.rule == RULE_ENUM_MEMBER_REORDERED),
            "ENUM-03 should not fire when ENUM-02 fired for same members, got: {:?}",
            findings
        );
    }
}
