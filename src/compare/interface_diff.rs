// Interface member structural diff engine.
//
// Implements interface-specific breaking change rules (REQ-14-05):
//   interface-member-removed: method/property removed from interface (Major)
//   interface-member-added: required property/method added to interface (Major for implementors)
//   interface-param-count-changed: method required param count changed (Major)

use std::collections::HashMap;
use std::path::Path;

use crate::types::{ClassMember, ClassMemberKind, Finding, Severity};
use super::subtype::{
    RULE_INTERFACE_GENERIC_CONSTRAINT_TIGHTENED, RULE_INTERFACE_MEMBER_ADDED,
    RULE_INTERFACE_MEMBER_REMOVED, RULE_INTERFACE_PARAM_COUNT_CHANGED,
    RULE_INTERFACE_READONLY_CHANGED,
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
            let is_optional = new_member.type_signature.as_deref() == Some("optional");
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
    let end = rest
        .find(|c: char| c == '>' || c == ')' || c == ',')
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
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
            type_signature: Some("optional".to_string()),
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
}
