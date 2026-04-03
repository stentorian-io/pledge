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
    RULE_INTERFACE_MEMBER_ADDED, RULE_INTERFACE_MEMBER_REMOVED,
    RULE_INTERFACE_PARAM_COUNT_CHANGED,
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
}
