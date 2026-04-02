// Class diff comparison engine.
//
// Implements all 9 class-related breaking change rules (CLS-01..09):
//   CLS-01: Added required constructor parameter (class-constructor-param-added)
//   CLS-02: Method visibility narrowed (class-method-visibility-changed)
//   CLS-03: Method removed from class (class-method-removed)
//   CLS-04: Concrete class made abstract (class-made-abstract)
//   CLS-05: Abstract method added to abstract class (class-abstract-method-added)
//   CLS-06: Inheritance (extends) changed (class-inheritance-changed)
//   CLS-07: Static member type changed (class-static-type-changed)
//   CLS-08: Readonly modifier added or removed (class-readonly-changed)
//   CLS-09: Interface removed from implements list (class-implements-removed)

use std::collections::HashMap;
use std::path::Path;

use crate::types::{ClassMember, ClassMemberKind, ClassShape, Finding, Severity, Visibility};
use super::subtype::{
    RULE_CLASS_ABSTRACT_METHOD_ADDED, RULE_CLASS_CONSTRUCTOR_PARAM_ADDED,
    RULE_CLASS_IMPLEMENTS_REMOVED, RULE_CLASS_INHERITANCE_CHANGED, RULE_CLASS_MADE_ABSTRACT,
    RULE_CLASS_METHOD_REMOVED, RULE_CLASS_METHOD_VISIBILITY_CHANGED, RULE_CLASS_READONLY_CHANGED,
    RULE_CLASS_STATIC_TYPE_CHANGED,
};

/// Returns true if visibility was narrowed (a breaking change for consumers).
///
/// Narrowing: Public -> Protected, Public -> Private, Protected -> Private.
/// Widening (Protected -> Public, etc.) is never breaking.
fn is_visibility_narrowed(old: &Visibility, new: &Visibility) -> bool {
    matches!(
        (old, new),
        (Visibility::Public, Visibility::Protected)
            | (Visibility::Public, Visibility::Private)
            | (Visibility::Protected, Visibility::Private)
    )
}

/// Compare two ClassShape structs and produce a list of breaking change findings.
///
/// - `old`: ClassShape from the previously published version
/// - `new`: ClassShape from the current version
/// - `symbol_name`: Name of the class (for finding messages)
/// - `file`: Source file path (for finding location)
/// - `line`, `col`: Source location (for finding location)
pub fn compare_class_shapes(
    old: &ClassShape,
    new: &ClassShape,
    symbol_name: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    // ── CLS-04: Concrete class made abstract ─────────────────────────────────
    // Breaking: consumers who do `new MyClass()` will get a compile error.
    if !old.is_abstract && new.is_abstract {
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_CLASS_MADE_ABSTRACT,
            file: file.to_path_buf(),
            line,
            col,
            message: format!("class `{}` was made abstract", symbol_name),
        });
    }

    // ── CLS-06: Inheritance (extends) changed ────────────────────────────────
    // Breaking: changes the prototype chain; consumers relying on inherited
    // members or instanceof checks will break.
    if old.extends_text != new.extends_text {
        let old_extends = old
            .extends_text
            .as_deref()
            .unwrap_or("(none)");
        let new_extends = new
            .extends_text
            .as_deref()
            .unwrap_or("(none)");
        findings.push(Finding {
            severity: Severity::Major,
            rule: RULE_CLASS_INHERITANCE_CHANGED,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "class `{}` inheritance changed from `{}` to `{}`",
                symbol_name, old_extends, new_extends
            ),
        });
    }

    // ── CLS-09: Interface removed from implements list ───────────────────────
    // Breaking: consumers who assign the class to variables typed as the
    // removed interface will get a compile error.
    let new_implements: std::collections::HashSet<&str> =
        new.implements_list.iter().map(|s| s.as_str()).collect();
    for interface_name in &old.implements_list {
        if !new_implements.contains(interface_name.as_str()) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CLASS_IMPLEMENTS_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "class `{}` no longer implements `{}`",
                    symbol_name, interface_name
                ),
            });
        }
    }

    // ── Member-level comparisons ─────────────────────────────────────────────
    // Build name-keyed maps for O(1) lookup.
    let old_members: HashMap<&str, &ClassMember> =
        old.members.iter().map(|m| (m.name.as_str(), m)).collect();
    let new_members: HashMap<&str, &ClassMember> =
        new.members.iter().map(|m| (m.name.as_str(), m)).collect();

    // CLS-03: Method or constructor removed
    // Breaking: call sites that invoke the method will fail to compile.
    for (name, old_member) in &old_members {
        if matches!(
            old_member.kind,
            ClassMemberKind::Method | ClassMemberKind::Constructor
        ) && !new_members.contains_key(name)
        {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CLASS_METHOD_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "method `{}` was removed from class `{}`",
                    name, symbol_name
                ),
            });
        }
    }

    // CLS-05: Abstract method added (only meaningful on abstract classes)
    // Breaking: any concrete subclass that doesn't implement the new abstract
    // method becomes invalid.
    for (name, new_member) in &new_members {
        if new_member.is_abstract && !old_members.contains_key(name) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CLASS_ABSTRACT_METHOD_ADDED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "abstract method `{}` was added to class `{}`",
                    name, symbol_name
                ),
            });
        }
    }

    // For members present in both old and new:
    for (name, old_member) in &old_members {
        if let Some(new_member) = new_members.get(name) {
            // CLS-01: Constructor required parameter count increased
            // Breaking: existing call sites that don't pass the new param will fail.
            if matches!(old_member.kind, ClassMemberKind::Constructor)
                && new_member.param_count > old_member.param_count
            {
                let diff = new_member.param_count - old_member.param_count;
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_CLASS_CONSTRUCTOR_PARAM_ADDED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "class `{}` constructor added {} required parameter(s) (was {}, now {})",
                        symbol_name, diff, old_member.param_count, new_member.param_count
                    ),
                });
            }

            // CLS-02: Visibility narrowed (Public->Protected, Public->Private, Protected->Private)
            // Breaking: consumers who called the member will get an access error.
            if is_visibility_narrowed(&old_member.visibility, &new_member.visibility) {
                let vis_str = |v: &Visibility| match v {
                    Visibility::Public => "public",
                    Visibility::Protected => "protected",
                    Visibility::Private => "private",
                };
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_CLASS_METHOD_VISIBILITY_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "method `{}` visibility changed from {} to {} in class `{}`",
                        name,
                        vis_str(&old_member.visibility),
                        vis_str(&new_member.visibility),
                        symbol_name
                    ),
                });
            }

            // CLS-07: Static member type changed
            // Breaking: consumers who assign or read the static member as the
            // old type will get a type error.
            if old_member.is_static
                && new_member.is_static
                && old_member.type_signature.is_some()
                && new_member.type_signature.is_some()
                && old_member.type_signature != new_member.type_signature
            {
                let old_type = old_member.type_signature.as_deref().unwrap();
                let new_type = new_member.type_signature.as_deref().unwrap();
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_CLASS_STATIC_TYPE_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "static member `{}` type changed from `{}` to `{}` in class `{}`",
                        name, old_type, new_type, symbol_name
                    ),
                });
            }

            // CLS-08: Readonly modifier added or removed on a property
            // Breaking in both directions: adding readonly prevents writes;
            // removing readonly may break consumers using readonly semantics.
            if matches!(old_member.kind, ClassMemberKind::Property)
                && old_member.is_readonly != new_member.is_readonly
            {
                let modifier_change = if !old_member.is_readonly && new_member.is_readonly {
                    "added"
                } else {
                    "removed"
                };
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_CLASS_READONLY_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "property `{}` readonly modifier {} in class `{}`",
                        name, modifier_change, symbol_name
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

    /// Build a ClassMember with sensible defaults.
    fn make_member(name: &str, kind: ClassMemberKind, vis: Visibility) -> ClassMember {
        ClassMember {
            name: name.to_string(),
            kind,
            visibility: vis,
            is_static: false,
            is_abstract: false,
            is_readonly: false,
            param_count: 0,
            type_signature: None,
        }
    }

    /// Build a minimal ClassShape containing the given members.
    fn make_class(members: Vec<ClassMember>) -> ClassShape {
        ClassShape {
            members,
            is_abstract: false,
            extends_text: None,
            implements_list: vec![],
            type_params: vec![],
        }
    }

    fn file() -> PathBuf {
        PathBuf::from("src/index.ts")
    }

    fn run(old: &ClassShape, new: &ClassShape) -> Vec<Finding> {
        compare_class_shapes(old, new, "MyClass", &file(), 1, 0)
    }

    fn rules(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.rule).collect()
    }

    // ── CLS-01: Constructor param added ──────────────────────────────────────

    #[test]
    fn test_cls01_constructor_param_added() {
        let mut old_ctor = make_member("constructor", ClassMemberKind::Constructor, Visibility::Public);
        old_ctor.param_count = 1;
        let mut new_ctor = make_member("constructor", ClassMemberKind::Constructor, Visibility::Public);
        new_ctor.param_count = 2;

        let old = make_class(vec![old_ctor]);
        let new = make_class(vec![new_ctor]);
        let findings = run(&old, &new);

        assert!(
            rules(&findings).contains(&"class-constructor-param-added"),
            "Expected class-constructor-param-added, got: {:?}",
            rules(&findings)
        );
        assert!(
            findings[0].message.contains("was 1, now 2"),
            "Message should include old and new param counts"
        );
    }

    #[test]
    fn test_cls01_negative_param_count_decreased() {
        // Fewer params is not breaking for callers — no CLS-01 finding.
        let mut old_ctor = make_member("constructor", ClassMemberKind::Constructor, Visibility::Public);
        old_ctor.param_count = 2;
        let mut new_ctor = make_member("constructor", ClassMemberKind::Constructor, Visibility::Public);
        new_ctor.param_count = 1;

        let old = make_class(vec![old_ctor]);
        let new = make_class(vec![new_ctor]);
        let findings = run(&old, &new);

        assert!(
            !rules(&findings).contains(&"class-constructor-param-added"),
            "Decreased param count should NOT fire class-constructor-param-added"
        );
    }

    // ── CLS-02: Method visibility changed ────────────────────────────────────

    #[test]
    fn test_cls02_public_to_protected() {
        let old_m = make_member("doWork", ClassMemberKind::Method, Visibility::Public);
        let new_m = make_member("doWork", ClassMemberKind::Method, Visibility::Protected);

        let old = make_class(vec![old_m]);
        let new = make_class(vec![new_m]);
        let findings = run(&old, &new);

        assert!(
            rules(&findings).contains(&"class-method-visibility-changed"),
            "Public->Protected should fire class-method-visibility-changed"
        );
    }

    #[test]
    fn test_cls02_public_to_private() {
        let old_m = make_member("doWork", ClassMemberKind::Method, Visibility::Public);
        let new_m = make_member("doWork", ClassMemberKind::Method, Visibility::Private);

        let old = make_class(vec![old_m]);
        let new = make_class(vec![new_m]);
        let findings = run(&old, &new);

        assert!(
            rules(&findings).contains(&"class-method-visibility-changed"),
            "Public->Private should fire class-method-visibility-changed"
        );
    }

    #[test]
    fn test_cls02_negative_protected_to_public() {
        // Widening visibility is safe — no finding.
        let old_m = make_member("doWork", ClassMemberKind::Method, Visibility::Protected);
        let new_m = make_member("doWork", ClassMemberKind::Method, Visibility::Public);

        let old = make_class(vec![old_m]);
        let new = make_class(vec![new_m]);
        let findings = run(&old, &new);

        assert!(
            !rules(&findings).contains(&"class-method-visibility-changed"),
            "Protected->Public (widening) should NOT fire class-method-visibility-changed"
        );
    }

    // ── CLS-03: Method removed ────────────────────────────────────────────────

    #[test]
    fn test_cls03_method_removed() {
        let old_m = make_member("foo", ClassMemberKind::Method, Visibility::Public);

        let old = make_class(vec![old_m]);
        let new = make_class(vec![]);
        let findings = run(&old, &new);

        assert!(
            rules(&findings).contains(&"class-method-removed"),
            "Removed method should fire class-method-removed"
        );
        assert!(
            findings.iter().any(|f| f.message.contains("`foo`")),
            "Message should include the method name"
        );
    }

    // ── CLS-04: Class made abstract ───────────────────────────────────────────

    #[test]
    fn test_cls04_concrete_to_abstract() {
        let mut new_shape = make_class(vec![]);
        new_shape.is_abstract = true;

        let old = make_class(vec![]);
        let findings = run(&old, &new_shape);

        assert!(
            rules(&findings).contains(&"class-made-abstract"),
            "Concrete->Abstract should fire class-made-abstract"
        );
    }

    #[test]
    fn test_cls04_negative_abstract_to_concrete() {
        // Abstract -> concrete is relaxing, not breaking.
        let mut old_shape = make_class(vec![]);
        old_shape.is_abstract = true;

        let new_shape = make_class(vec![]);
        let findings = run(&old_shape, &new_shape);

        assert!(
            !rules(&findings).contains(&"class-made-abstract"),
            "Abstract->Concrete should NOT fire class-made-abstract"
        );
    }

    // ── CLS-05: Abstract method added ────────────────────────────────────────

    #[test]
    fn test_cls05_abstract_method_added() {
        let mut new_m = make_member("bar", ClassMemberKind::Method, Visibility::Public);
        new_m.is_abstract = true;

        let mut new_shape = make_class(vec![new_m]);
        new_shape.is_abstract = true;

        let old = make_class(vec![]);
        let findings = run(&old, &new_shape);

        assert!(
            rules(&findings).contains(&"class-abstract-method-added"),
            "New abstract method should fire class-abstract-method-added"
        );
        assert!(
            findings.iter().any(|f| f.message.contains("`bar`")),
            "Message should include the method name"
        );
    }

    // ── CLS-06: Inheritance changed ──────────────────────────────────────────

    #[test]
    fn test_cls06_extends_changed() {
        let mut old_shape = make_class(vec![]);
        old_shape.extends_text = Some("Base".to_string());

        let mut new_shape = make_class(vec![]);
        new_shape.extends_text = Some("OtherBase".to_string());

        let findings = run(&old_shape, &new_shape);

        assert!(
            rules(&findings).contains(&"class-inheritance-changed"),
            "Changed extends should fire class-inheritance-changed"
        );
        assert!(
            findings.iter().any(|f| f.message.contains("Base") && f.message.contains("OtherBase")),
            "Message should include old and new extends"
        );
    }

    #[test]
    fn test_cls06_extends_removed() {
        let mut old_shape = make_class(vec![]);
        old_shape.extends_text = Some("Base".to_string());

        let new_shape = make_class(vec![]);
        // new_shape.extends_text = None (default)

        let findings = run(&old_shape, &new_shape);

        assert!(
            rules(&findings).contains(&"class-inheritance-changed"),
            "Removed extends should fire class-inheritance-changed"
        );
        assert!(
            findings.iter().any(|f| f.message.contains("(none)")),
            "Message should use (none) for missing extends"
        );
    }

    // ── CLS-07: Static type changed ──────────────────────────────────────────

    #[test]
    fn test_cls07_static_type_changed() {
        let mut old_m = make_member("count", ClassMemberKind::Property, Visibility::Public);
        old_m.is_static = true;
        old_m.type_signature = Some("number".to_string());

        let mut new_m = make_member("count", ClassMemberKind::Property, Visibility::Public);
        new_m.is_static = true;
        new_m.type_signature = Some("string".to_string());

        let old = make_class(vec![old_m]);
        let new = make_class(vec![new_m]);
        let findings = run(&old, &new);

        assert!(
            rules(&findings).contains(&"class-static-type-changed"),
            "Changed static member type should fire class-static-type-changed"
        );
        assert!(
            findings.iter().any(|f| f.message.contains("number") && f.message.contains("string")),
            "Message should include old and new types"
        );
    }

    // ── CLS-08: Readonly changed ──────────────────────────────────────────────

    #[test]
    fn test_cls08_readonly_added() {
        let old_m = make_member("name", ClassMemberKind::Property, Visibility::Public);
        let mut new_m = make_member("name", ClassMemberKind::Property, Visibility::Public);
        new_m.is_readonly = true;

        let old = make_class(vec![old_m]);
        let new = make_class(vec![new_m]);
        let findings = run(&old, &new);

        assert!(
            rules(&findings).contains(&"class-readonly-changed"),
            "Adding readonly should fire class-readonly-changed"
        );
        assert!(
            findings.iter().any(|f| f.message.contains("added")),
            "Message should say 'added'"
        );
    }

    #[test]
    fn test_cls08_readonly_removed() {
        let mut old_m = make_member("name", ClassMemberKind::Property, Visibility::Public);
        old_m.is_readonly = true;
        let new_m = make_member("name", ClassMemberKind::Property, Visibility::Public);

        let old = make_class(vec![old_m]);
        let new = make_class(vec![new_m]);
        let findings = run(&old, &new);

        assert!(
            rules(&findings).contains(&"class-readonly-changed"),
            "Removing readonly should fire class-readonly-changed"
        );
        assert!(
            findings.iter().any(|f| f.message.contains("removed")),
            "Message should say 'removed'"
        );
    }

    // ── CLS-09: Implements removed ───────────────────────────────────────────

    #[test]
    fn test_cls09_implements_removed() {
        let mut old_shape = make_class(vec![]);
        old_shape.implements_list = vec!["Serializable".to_string(), "Cloneable".to_string()];

        let mut new_shape = make_class(vec![]);
        new_shape.implements_list = vec!["Serializable".to_string()];

        let findings = run(&old_shape, &new_shape);

        assert!(
            rules(&findings).contains(&"class-implements-removed"),
            "Removed interface should fire class-implements-removed"
        );
        assert!(
            findings.iter().any(|f| f.message.contains("Cloneable")),
            "Message should include the removed interface name"
        );
        // Serializable was kept — should NOT appear in findings
        assert!(
            !findings.iter().any(|f| f.rule == "class-implements-removed" && f.message.contains("Serializable")),
            "Retained interface should NOT produce a finding"
        );
    }

    // ── Identical shapes produce no findings ──────────────────────────────────

    #[test]
    fn test_identical_shapes_no_findings() {
        let mut ctor = make_member("constructor", ClassMemberKind::Constructor, Visibility::Public);
        ctor.param_count = 2;
        let method = make_member("doWork", ClassMemberKind::Method, Visibility::Public);
        let mut prop = make_member("name", ClassMemberKind::Property, Visibility::Public);
        prop.is_readonly = true;

        let mut shape = make_class(vec![ctor, method, prop]);
        shape.extends_text = Some("Base".to_string());
        shape.implements_list = vec!["IFoo".to_string(), "IBar".to_string()];

        let findings = run(&shape, &shape);

        assert!(
            findings.is_empty(),
            "Identical shapes should produce no findings, got: {:?}",
            rules(&findings)
        );
    }
}
