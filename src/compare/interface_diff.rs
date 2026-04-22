// Interface member structural diff engine.
//
// Implements interface-specific breaking change rules (REQ-14-05):
//   interface-member-removed: method/property removed from interface (Major)
//   interface-member-added: required property/method added to interface (Major for implementors)
//   interface-param-count-changed: method required param count changed (Major)

use std::collections::HashMap;
use std::path::Path;

use crate::discover::tree_query::IndexSignature;
use crate::types::{ClassMember, ClassMemberKind, ConstructSignature, Finding, Severity};
use super::subtype::{
    RULE_CONSTRUCT_SIGNATURE_REMOVED,
    RULE_INTERFACE_GENERIC_CONSTRAINT_TIGHTENED, RULE_INTERFACE_INDEX_SIGNATURE_CHANGED,
    RULE_INTERFACE_MEMBER_ADDED, RULE_INTERFACE_MEMBER_REMOVED,
    RULE_INTERFACE_PARAM_COUNT_CHANGED, RULE_INTERFACE_READONLY_CHANGED,
};

/// Compare interface members from old and new versions.
///
/// - `old_members`: Members extracted from old interface(s) (merged if declaration merging)
/// - `new_members`: Members extracted from new interface(s) (merged if declaration merging)
/// - `symbol_name`: Interface name for finding messages
/// - `file`, `line`, `col`: Source location
pub fn compare_interface_members(
    old_members: &[ClassMember],
    new_members: &[ClassMember],
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    let old_map: HashMap<&str, &ClassMember> =
        old_members.iter().map(|m| (m.name.as_str(), m)).collect();
    let new_map: HashMap<&str, &ClassMember> =
        new_members.iter().map(|m| (m.name.as_str(), m)).collect();

    // interface-member-removed: member in old but not in new
    for name in old_map.keys() {
        if !new_map.contains_key(name) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_INTERFACE_MEMBER_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "member `{}` was removed from interface `{}`",
                    name, symbol_name
                ),
            });
        }
    }

    // interface-member-added: member in new but not in old
    // Required members are Major (implementors must add them)
    // Optional members are Minor (implementors unaffected)
    for (name, new_member) in &new_map {
        if !old_map.contains_key(name) {
            // Phase 17: use proper is_optional field (replaces type_signature=="optional" stopgap)
            let is_optional = new_member.is_optional;
            findings.push(Finding {
                severity: if is_optional { Severity::Minor } else { Severity::Major },
                rule: RULE_INTERFACE_MEMBER_ADDED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "{} member `{}` was added to interface `{}`",
                    if is_optional { "optional" } else { "required" },
                    name,
                    symbol_name
                ),
            });
        }
    }

    // interface-param-count-changed: method param count changed
    // GAP-16-03: interface-readonly-changed
    // GAP-16-10: interface-generic-constraint-tightened
    for (name, old_member) in &old_map {
        if let Some(new_member) = new_map.get(name) {
            if matches!(old_member.kind, ClassMemberKind::Method)
                && matches!(new_member.kind, ClassMemberKind::Method)
                && old_member.param_count != new_member.param_count
            {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INTERFACE_PARAM_COUNT_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "method `{}` in interface `{}` changed required params from {} to {}",
                        name, symbol_name, old_member.param_count, new_member.param_count
                    ),
                });
            }

            // GAP-16-03: interface property readonly changed
            if matches!(old_member.kind, ClassMemberKind::Property)
                && old_member.is_readonly != new_member.is_readonly
            {
                let direction = if !old_member.is_readonly && new_member.is_readonly {
                    "added"
                } else {
                    "removed"
                };
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INTERFACE_READONLY_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "property `{}` readonly modifier {} in interface `{}`",
                        name, direction, symbol_name
                    ),
                });
            }

            // GAP-16-10: interface method generic constraint tightening
            // Heuristic: if method type signatures both exist and differ, and the new
            // signature has a tighter `extends` bound, flag it.
            if matches!(old_member.kind, ClassMemberKind::Method)
                && matches!(new_member.kind, ClassMemberKind::Method)
            {
                if let (Some(old_sig), Some(new_sig)) =
                    (&old_member.type_signature, &new_member.type_signature)
                {
                    if old_sig != new_sig && has_tighter_generic_constraint(old_sig, new_sig) {
                        findings.push(Finding {
                            severity: Severity::Major,
                            rule: RULE_INTERFACE_GENERIC_CONSTRAINT_TIGHTENED,
                            file: file.to_path_buf(),
                            line,
                            col,
                            message: format!(
                                "method `{}` generic constraint tightened in interface `{}`",
                                name, symbol_name
                            ),
                        });
                    }
                }
            }
        }
    }

    findings
}

/// Heuristic check for generic constraint tightening.
///
/// Compares extends clauses in type signatures. A constraint is considered
/// tightened if: (a) old had no extends but new does, or (b) both have extends
/// but the new constraint text is different (conservative: any change is flagged).
fn has_tighter_generic_constraint(old_sig: &str, new_sig: &str) -> bool {
    let old_has_extends = old_sig.contains(" extends ");
    let new_has_extends = new_sig.contains(" extends ");
    if !old_has_extends && new_has_extends {
        // Constraint added where none existed
        return true;
    }
    if old_has_extends && new_has_extends {
        // Both have extends — extract and compare the constraint text
        if let (Some(old_c), Some(new_c)) =
            (extract_extends_constraint(old_sig), extract_extends_constraint(new_sig))
        {
            return old_c != new_c;
        }
    }
    false
}

/// Extract the constraint text following " extends " in a type signature string.
/// Returns the text up to the first `>`, `)`, or `,` delimiter (trimmed).
fn extract_extends_constraint(sig: &str) -> Option<String> {
    let idx = sig.find(" extends ")?;
    let rest = &sig[idx + 9..];
    let end = rest.find(['>', ')', ',']).unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

/// Compare index signatures on an interface between old and new versions.
///
/// GAP-16-04: Detects removal of index signatures and value type changes.
/// This is the interface-level equivalent of REF-07 (which handles symbol-level).
pub fn compare_interface_index_signatures(
    old_sigs: &[IndexSignature],
    new_sigs: &[IndexSignature],
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    for old_sig in old_sigs {
        let matching_new = new_sigs
            .iter()
            .find(|ns| ns.key_type_text == old_sig.key_type_text);
        match matching_new {
            None => {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INTERFACE_INDEX_SIGNATURE_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "index signature `[key: {}]: {}` was removed from interface `{}`",
                        old_sig.key_type_text, old_sig.value_type_text, symbol_name
                    ),
                });
            }
            Some(new_sig) if new_sig.value_type_text != old_sig.value_type_text => {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_INTERFACE_INDEX_SIGNATURE_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "index signature value type changed from `{}` to `{}` in interface `{}`",
                        old_sig.value_type_text, new_sig.value_type_text, symbol_name
                    ),
                });
            }
            _ => {}
        }
    }

    findings
}

/// Compare construct signatures between old and new interface versions (TLF-01).
///
/// A construct signature is removed when old has `new(...)` that new doesn't have.
/// Matched by param_count (arity). Removal is Major.
pub fn compare_construct_signatures(
    old_sigs: &[ConstructSignature],
    new_sigs: &[ConstructSignature],
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    for old_sig in old_sigs {
        let matched = new_sigs.iter().any(|n| n.param_count == old_sig.param_count);
        if !matched {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CONSTRUCT_SIGNATURE_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "construct signature with {} parameter(s) was removed from `{}`",
                    old_sig.param_count, symbol_name
                ),
            });
        }
    }

    findings
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ClassMemberKind, Visibility};
    use std::path::PathBuf;

    fn file() -> PathBuf {
        PathBuf::from("src/index.ts")
    }

    fn make_method(name: &str, param_count: usize) -> ClassMember {
        ClassMember {
            name: name.to_string(),
            kind: ClassMemberKind::Method,
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_readonly: false,
            param_count,
            type_signature: None,
            is_optional: false,
            is_accessor: false,
            this_param_type: None,
        }
    }

    fn make_required_prop(name: &str) -> ClassMember {
        ClassMember {
            name: name.to_string(),
            kind: ClassMemberKind::Property,
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_readonly: false,
            param_count: 0,
            type_signature: None,
            is_optional: false,
            is_accessor: false,
            this_param_type: None,
        }
    }

    fn make_optional_prop(name: &str) -> ClassMember {
        ClassMember {
            name: name.to_string(),
            kind: ClassMemberKind::Property,
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_readonly: false,
            param_count: 0,
            type_signature: None,
            is_optional: true,
            is_accessor: false,
            this_param_type: None,
        }
    }

    fn run(old: &[ClassMember], new: &[ClassMember]) -> Vec<Finding> {
        compare_interface_members(old, new, "MyInterface", &file(), 1, 0)
    }

    fn rules(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.rule).collect()
    }

    // ── interface-member-removed ──────────────────────────────────────────────

    #[test]
    fn test_member_removed_is_major() {
        let old = vec![make_method("foo", 1)];
        let new: Vec<ClassMember> = vec![];
        let findings = run(&old, &new);
        assert!(
            rules(&findings).contains(&"interface-member-removed"),
            "Removed member should fire interface-member-removed, got: {:?}",
            rules(&findings)
        );
        assert_eq!(
            findings[0].severity,
            Severity::Major,
            "interface-member-removed should be Major"
        );
        assert!(
            findings[0].message.contains("`foo`"),
            "Message should include member name"
        );
    }

    // ── interface-member-added (required) ─────────────────────────────────────

    #[test]
    fn test_required_member_added_is_major() {
        let old: Vec<ClassMember> = vec![];
        let new = vec![make_required_prop("bar")];
        let findings = run(&old, &new);
        assert!(
            rules(&findings).contains(&"interface-member-added"),
            "Added required member should fire interface-member-added, got: {:?}",
            rules(&findings)
        );
        assert_eq!(
            findings[0].severity,
            Severity::Major,
            "Required member added to interface should be Major"
        );
        assert!(
            findings[0].message.contains("required"),
            "Message should say 'required'"
        );
    }

    // ── interface-member-added (optional) ─────────────────────────────────────

    #[test]
    fn test_optional_member_added_is_minor() {
        let old: Vec<ClassMember> = vec![];
        let new = vec![make_optional_prop("baz")];
        let findings = run(&old, &new);
        assert!(
            rules(&findings).contains(&"interface-member-added"),
            "Added optional member should fire interface-member-added, got: {:?}",
            rules(&findings)
        );
        assert_eq!(
            findings[0].severity,
            Severity::Minor,
            "Optional member added to interface should be Minor"
        );
        assert!(
            findings[0].message.contains("optional"),
            "Message should say 'optional'"
        );
    }

    // ── interface-param-count-changed ─────────────────────────────────────────

    #[test]
    fn test_method_param_count_changed_is_major() {
        let old = vec![make_method("doWork", 1)];
        let new = vec![make_method("doWork", 2)];
        let findings = run(&old, &new);
        assert!(
            rules(&findings).contains(&"interface-param-count-changed"),
            "Changed param count should fire interface-param-count-changed, got: {:?}",
            rules(&findings)
        );
        assert_eq!(
            findings[0].severity,
            Severity::Major,
            "interface-param-count-changed should be Major"
        );
        assert!(
            findings[0].message.contains("1") && findings[0].message.contains("2"),
            "Message should include old and new param counts"
        );
    }

    // ── Identical members produce no findings ─────────────────────────────────

    #[test]
    fn test_identical_members_no_findings() {
        let members = vec![
            make_method("foo", 2),
            make_required_prop("bar"),
            make_optional_prop("baz"),
        ];
        let findings = run(&members, &members);
        assert!(
            findings.is_empty(),
            "Identical interface members should produce no findings, got: {:?}",
            rules(&findings)
        );
    }

    // ── interface-readonly-changed ────────────────────────────────────────────

    fn make_readonly_prop(name: &str) -> ClassMember {
        ClassMember {
            name: name.to_string(),
            kind: ClassMemberKind::Property,
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_readonly: true,
            param_count: 0,
            type_signature: None,
            is_optional: false,
            is_accessor: false,
            this_param_type: None,
        }
    }

    #[test]
    fn test_readonly_added_to_property_is_major() {
        let old = vec![make_required_prop("count")];
        let new = vec![make_readonly_prop("count")];
        let findings = run(&old, &new);
        assert!(
            rules(&findings).contains(&"interface-readonly-changed"),
            "Adding readonly should fire interface-readonly-changed, got: {:?}",
            rules(&findings)
        );
        let f = findings.iter().find(|f| f.rule == "interface-readonly-changed").unwrap();
        assert_eq!(f.severity, Severity::Major);
        assert!(f.message.contains("added"), "Message should say 'added'");
        assert!(f.message.contains("`count`"), "Message should include property name");
    }

    #[test]
    fn test_readonly_removed_from_property_is_major() {
        let old = vec![make_readonly_prop("count")];
        let new = vec![make_required_prop("count")];
        let findings = run(&old, &new);
        assert!(
            rules(&findings).contains(&"interface-readonly-changed"),
            "Removing readonly should fire interface-readonly-changed, got: {:?}",
            rules(&findings)
        );
        let f = findings.iter().find(|f| f.rule == "interface-readonly-changed").unwrap();
        assert_eq!(f.severity, Severity::Major);
        assert!(f.message.contains("removed"), "Message should say 'removed'");
    }

    #[test]
    fn test_readonly_unchanged_no_finding() {
        let old = vec![make_readonly_prop("x")];
        let new = vec![make_readonly_prop("x")];
        let findings = run(&old, &new);
        assert!(
            !rules(&findings).contains(&"interface-readonly-changed"),
            "Unchanged readonly should not fire, got: {:?}",
            rules(&findings)
        );
    }

    // ── interface-generic-constraint-tightened ────────────────────────────────

    fn make_method_with_sig(name: &str, sig: &str) -> ClassMember {
        ClassMember {
            name: name.to_string(),
            kind: ClassMemberKind::Method,
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_readonly: false,
            param_count: 1,
            type_signature: Some(sig.to_string()),
            is_optional: false,
            is_accessor: false,
            this_param_type: None,
        }
    }

    #[test]
    fn test_generic_constraint_tightened_fires() {
        // Old: no extends, new: adds extends constraint
        let old = vec![make_method_with_sig("process", "<T>(item: T) => void")];
        let new = vec![make_method_with_sig("process", "<T extends object>(item: T) => void")];
        let findings = run(&old, &new);
        assert!(
            rules(&findings).contains(&"interface-generic-constraint-tightened"),
            "Tightened generic constraint should fire, got: {:?}",
            rules(&findings)
        );
        let f = findings
            .iter()
            .find(|f| f.rule == "interface-generic-constraint-tightened")
            .unwrap();
        assert_eq!(f.severity, Severity::Major);
        assert!(f.message.contains("`process`"));
    }

    #[test]
    fn test_generic_constraint_unchanged_no_finding() {
        let sig = "<T extends object>(item: T) => void";
        let old = vec![make_method_with_sig("process", sig)];
        let new = vec![make_method_with_sig("process", sig)];
        let findings = run(&old, &new);
        assert!(
            !rules(&findings).contains(&"interface-generic-constraint-tightened"),
            "Unchanged constraint should not fire, got: {:?}",
            rules(&findings)
        );
    }

    // ── compare_interface_index_signatures ───────────────────────────────────

    fn run_index(
        old: &[IndexSignature],
        new: &[IndexSignature],
    ) -> Vec<Finding> {
        compare_interface_index_signatures(old, new, "MyInterface", &file(), 1, 0)
    }

    fn make_sig(key: &str, val: &str) -> IndexSignature {
        IndexSignature {
            key_type_text: key.to_string(),
            value_type_text: val.to_string(),
        }
    }

    #[test]
    fn test_index_signature_removed_fires() {
        let old = vec![make_sig("string", "number")];
        let new: Vec<IndexSignature> = vec![];
        let findings = run_index(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "interface-index-signature-changed");
        assert_eq!(findings[0].severity, Severity::Major);
        assert!(
            findings[0].message.contains("removed"),
            "Message should say 'removed'"
        );
    }

    #[test]
    fn test_index_signature_value_type_changed_fires() {
        let old = vec![make_sig("string", "number")];
        let new = vec![make_sig("string", "string")];
        let findings = run_index(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "interface-index-signature-changed");
        assert_eq!(findings[0].severity, Severity::Major);
        assert!(
            findings[0].message.contains("number") && findings[0].message.contains("string"),
            "Message should show old and new type"
        );
    }

    #[test]
    fn test_index_signature_unchanged_no_finding() {
        let sigs = vec![make_sig("string", "number")];
        let findings = run_index(&sigs, &sigs);
        assert!(
            findings.is_empty(),
            "Unchanged index signature should produce no finding, got: {:?}",
            findings
        );
    }

    #[test]
    fn test_multiple_index_signatures_one_removed() {
        let old = vec![make_sig("string", "number"), make_sig("number", "string")];
        let new = vec![make_sig("string", "number")];
        let findings = run_index(&old, &new);
        assert_eq!(
            findings.len(),
            1,
            "Only the removed signature should fire, got: {:?}",
            findings
        );
        assert!(findings[0].message.contains("number"));
    }

    // ── TLF-01: construct signature removed ──────────────────────────────────
    #[test]
    fn tlf_01_construct_signature_removed_is_major() {
        use crate::types::ConstructSignature;
        let old_sigs = vec![ConstructSignature {
            param_count: 1,
            return_text: Some("Foo".into()),
            is_abstract: false,
        }];
        let new_sigs: Vec<ConstructSignature> = vec![];
        let findings = compare_construct_signatures(&old_sigs, &new_sigs, "MyInterface", &file(), 1, 1);
        assert_eq!(findings.len(), 1, "removed construct signature should fire; got: {:?}", findings);
        assert_eq!(findings[0].rule, crate::compare::subtype::RULE_CONSTRUCT_SIGNATURE_REMOVED);
        assert_eq!(findings[0].severity, crate::types::Severity::Major);
    }

    #[test]
    fn tlf_01_construct_signature_unchanged_no_finding() {
        use crate::types::ConstructSignature;
        let sig = ConstructSignature { param_count: 1, return_text: Some("Foo".into()), is_abstract: false };
        let findings = compare_construct_signatures(&[sig.clone()], &[sig], "MyInterface", &file(), 1, 1);
        assert!(findings.is_empty(), "identical construct signatures should produce no findings");
    }
}
