// Class diff comparison engine.
//
// Implements all 9 class-related breaking change rules (CLS-01..09):
// CLS-01: Added required constructor parameter (class-constructor-param-added)
// CLS-02: Method visibility narrowed (class-method-visibility-changed)
// CLS-03: Method removed from class (class-method-removed)
// CLS-04: Concrete class made abstract (class-made-abstract)
// CLS-05: Abstract method added to abstract class (class-abstract-method-added)
// CLS-06: Inheritance (extends) changed (class-inheritance-changed)
// CLS-07: Static member type changed (class-static-type-changed)
// CLS-08: Readonly modifier added or removed (class-readonly-changed)
// CLS-09: Interface removed from implements list (class-implements-removed)

use crate::core::findings::{Finding, Severity};
use crate::languages::javascript::compare::rules as runtime_rules;
use crate::languages::javascript::compare::rules::{
    RULE_CLASS_ABSTRACT_METHOD_ADDED, RULE_CLASS_ACCESSOR_CHANGED,
    RULE_CLASS_CONSTRUCTOR_PARAM_ADDED, RULE_CLASS_FIELD_MADE_PRIVATE,
    RULE_CLASS_FIELD_MADE_PUBLIC, RULE_CLASS_IMPLEMENTS_REMOVED, RULE_CLASS_INHERITANCE_CHANGED,
    RULE_CLASS_MADE_ABSTRACT, RULE_CLASS_METHOD_KIND_CHANGED, RULE_CLASS_METHOD_REMOVED,
    RULE_CLASS_METHOD_VISIBILITY_CHANGED, RULE_CLASS_READONLY_CHANGED,
    RULE_CLASS_STATIC_TYPE_CHANGED, RULE_CONSTRUCTOR_PARAM_MADE_REQUIRED,
};
use crate::languages::javascript::model::{ClassMember, ClassMemberKind, ClassShape, Visibility};
use crate::languages::typescript::compare::rules as type_rules;
use crate::languages::typescript::compare::rules::{
    RULE_DECORATOR_ADDED, RULE_DECORATOR_ARGUMENTS_CHANGED, RULE_DECORATOR_REMOVED,
};
use crate::languages::typescript::model::DecoratorSpec;
use std::collections::{HashMap, HashSet};
use std::path::Path;

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

/// CLS-10: Human-readable label for a member kind used in
/// decorator-diff messages (e.g., "method `Foo.bar`").
fn member_kind_label(kind: &ClassMemberKind) -> &'static str {
    match kind {
        ClassMemberKind::Method => "method",
        ClassMemberKind::Property => "property",
        ClassMemberKind::Getter => "getter",
        ClassMemberKind::Setter => "setter",
        ClassMemberKind::Constructor => "constructor",
    }
}

/// CLS-10: Diff two decorator lists between old and new.
///
/// Algorithm:
/// - Step 1: pair exactly matching (name + args) decorators — those are unchanged.
/// - Step 2: among unpaired, match by `name` — emit one `decorator-arguments-changed`
///   per matched pair (same name, different args). This avoids the bug where a
///   single `@Route('/a')` → `@Route('/b')` would spuriously emit one
///   `decorator-removed` + one `decorator-added`.
/// - Step 3: remaining unmatched old → `decorator-removed`; remaining unmatched
///   new → `decorator-added`.
///
/// `owner_description` is human text like "class `Foo`" or "method `Foo.bar`"
/// used verbatim in finding messages.
fn diff_decorators(
    old: &[DecoratorSpec],
    new: &[DecoratorSpec],
    owner_description: &str,
    file: &Path,
    line: u32,
    col: u32,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut old_used = vec![false; old.len()];
    let mut new_used = vec![false; new.len()];

    // Step 1: exact (name, args) equality pairing.
    for (i, o) in old.iter().enumerate() {
        for (j, n) in new.iter().enumerate() {
            if !new_used[j] && !old_used[i] && o == n {
                old_used[i] = true;
                new_used[j] = true;
                break;
            }
        }
    }

    // Step 2: name-match pairing for arguments-changed emission.
    for (i, o) in old.iter().enumerate() {
        if old_used[i] {
            continue;
        }
        for (j, n) in new.iter().enumerate() {
            if !new_used[j] && o.name == n.name {
                old_used[i] = true;
                new_used[j] = true;
                let old_args = o.args.as_deref().unwrap_or("");
                let new_args = n.args.as_deref().unwrap_or("");
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_DECORATOR_ARGUMENTS_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "decorator `@{}` arguments changed on {}: `{}` -> `{}`",
                        o.name, owner_description, old_args, new_args
                    ),
                });
                // DOC-01: paired Info — decorator runtime effect is opaque.
                findings.push(Finding {
                    severity: Severity::Info,
                    rule: type_rules::RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "decorator runtime effect on {} is opaque to static analysis",
                        owner_description
                    ),
                });
                break;
            }
        }
    }

    // Step 3a: residual old-only → removed.
    for (i, o) in old.iter().enumerate() {
        if !old_used[i] {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_DECORATOR_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!("decorator `@{}` removed from {}", o.name, owner_description),
            });
            // DOC-01: paired Info — decorator runtime effect is opaque.
            findings.push(Finding {
                severity: Severity::Info,
                rule: type_rules::RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "decorator runtime effect on {} is opaque to static analysis",
                    owner_description
                ),
            });
        }
    }

    // Step 3b: residual new-only → added.
    for (j, n) in new.iter().enumerate() {
        if !new_used[j] {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_DECORATOR_ADDED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!("decorator `@{}` added to {}", n.name, owner_description),
            });
            // DOC-01: paired Info — decorator runtime effect is opaque.
            findings.push(Finding {
                severity: Severity::Info,
                rule: type_rules::RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "decorator runtime effect on {} is opaque to static analysis",
                    owner_description
                ),
            });
        }
    }

    findings
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
        let old_extends = old.extends_text.as_deref().unwrap_or("(none)");
        let new_extends = new.extends_text.as_deref().unwrap_or("(none)");
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
    // CLS-11: filter out `#`-prefixed private members from the
    // public-surface comparison maps. Private members are diffed in the
    // dedicated public↔private transition pass below; including them here
    // would let a rename-in-private-space (`#foo` → `#bar`) leak as a
    // spurious CLS-03 `class-method-removed`.
    let old_members: HashMap<(&str, bool), &ClassMember> = old
        .members
        .iter()
        .filter(|m| !m.is_private)
        .map(|m| ((m.name.as_str(), m.is_static), m))
        .collect();
    let new_members: HashMap<(&str, bool), &ClassMember> = new
        .members
        .iter()
        .filter(|m| !m.is_private)
        .map(|m| ((m.name.as_str(), m.is_static), m))
        .collect();

    // ── CLS-11: public↔private visibility transition ──────────────
    // Pair old public members with new private members (and vice versa) by
    // name-modulo-`#`. Fires Major `class-field-made-private` on public-to-
    // private transitions (consumers referring to `repo.cache` now break) and
    // Minor `class-field-made-public` on private-to-public transitions (new
    // surface exposed). Risk 2: silent when the same bare name exists on BOTH
    // the public AND private bucket on the SAME side (class has both `foo`
    // and `#foo` simultaneously — distinct runtime slots; no transition).
    fn bare_member_name(m: &ClassMember) -> &str {
        m.name.strip_prefix('#').unwrap_or(&m.name)
    }
    let old_pub: Vec<&ClassMember> = old.members.iter().filter(|m| !m.is_private).collect();
    let old_priv: Vec<&ClassMember> = old.members.iter().filter(|m| m.is_private).collect();
    let new_pub: Vec<&ClassMember> = new.members.iter().filter(|m| !m.is_private).collect();
    let new_priv: Vec<&ClassMember> = new.members.iter().filter(|m| m.is_private).collect();

    // Key matching on (bare_name, is_static) — `static foo` and instance
    // `#foo` are distinct runtime slots (class-object vs prototype); pairing
    // them as a public↔private transition would be wrong. Only same-
    // static-ness pairs represent the same logical slot.
    let old_pub_keys: HashSet<(&str, bool)> = old_pub
        .iter()
        .map(|m| (bare_member_name(m), m.is_static))
        .collect();
    let old_priv_keys: HashSet<(&str, bool)> = old_priv
        .iter()
        .map(|m| (bare_member_name(m), m.is_static))
        .collect();
    let new_pub_keys: HashSet<(&str, bool)> = new_pub
        .iter()
        .map(|m| (bare_member_name(m), m.is_static))
        .collect();
    let new_priv_keys: HashSet<(&str, bool)> = new_priv
        .iter()
        .map(|m| (bare_member_name(m), m.is_static))
        .collect();

    // Suppression set keyed on (bare_name, is_static) — mirrors the match key
    // above. CLS-03 removal suppression reconstructs the tuple from each
    // old-side entry's `is_static` flag before querying this set.
    let mut transitioned_keys: HashSet<(String, bool)> = HashSet::new();

    // Direction 1: old public `foo` → new private `#foo` → Major.
    for m in &old_pub {
        let k = (bare_member_name(m), m.is_static);
        if old_priv_keys.contains(&k) {
            continue; // Risk 2: both-on-old side — class has both public and private slot
        }
        if new_pub_keys.contains(&k) {
            continue; // still public on new — not a transition
        }
        if new_priv_keys.contains(&k) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CLASS_FIELD_MADE_PRIVATE,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "field `{}` on class `{}` became private (`#{}`) — consumers referencing this member will break",
                    k.0, symbol_name, k.0,
                ),
            });
            // DOC-01: paired Info — private-brand access is opaque.
            findings.push(Finding {
                severity: Severity::Info,
                rule: runtime_rules::RULE_LIMITATION_PRIVATE_BRAND_ACCESS,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "consumer reliance on `#{}` private-brand checks on `{}` cannot be statically verified",
                    k.0, symbol_name
                ),
            });
            transitioned_keys.insert((k.0.to_string(), k.1));
        }
    }

    // Direction 2: old private `#foo` → new public `foo` → Minor.
    for m in &old_priv {
        let k = (bare_member_name(m), m.is_static);
        if old_pub_keys.contains(&k) {
            continue; // Risk 2: both-on-old side
        }
        if new_priv_keys.contains(&k) {
            continue; // still private on new — not a transition
        }
        if new_pub_keys.contains(&k) {
            findings.push(Finding {
                severity: Severity::Minor,
                rule: RULE_CLASS_FIELD_MADE_PUBLIC,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "field `#{}` on class `{}` became public (`{}`) — previously-hidden surface is now observable",
                    k.0, symbol_name, k.0,
                ),
            });
            // DOC-01: paired Info — private-brand access is opaque.
            findings.push(Finding {
                severity: Severity::Info,
                rule: runtime_rules::RULE_LIMITATION_PRIVATE_BRAND_ACCESS,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "consumer reliance on `#{}` private-brand checks on `{}` cannot be statically verified",
                    k.0, symbol_name
                ),
            });
            transitioned_keys.insert((k.0.to_string(), k.1));
        }
    }

    // CLS-03: Method or constructor removed (also covers getters and setters)
    // Breaking: call sites that invoke the method/getter/setter will fail to compile.
    for (key, old_member) in &old_members {
        let name = key.0;
        if old_member.visibility == Visibility::Private {
            continue;
        }
        // CLS-11 suppression: if this (bare_name, is_static) slot
        // transitioned public↔private, the CLS-11 block above already emitted
        // the canonical finding. Skip CLS-03 to avoid double-reporting the
        // same conceptual change at two rule-IDs. Mirrors the REF-09 → REF-07
        // the loop in TypeScript refinement comparison.
        //
        // The key must include `is_static` so a removed `static foo` with an
        // added instance `#foo` (different runtime slots) does NOT suppress
        // the genuine CLS-03 removal of the static.
        let bare = name.strip_prefix('#').unwrap_or(name);
        if transitioned_keys.contains(&(bare.to_string(), old_member.is_static)) {
            continue;
        }
        // CLS-03: Method/Constructor/Getter/Setter removal.
        // CLS-12: additionally fires for static Property removal —
        // synthesized static-block members (ClassMemberKind::Property with
        // is_static = true) and top-level `static bar = 1;` declarations
        // surface here. Non-static property removal remains silent, matching
        // existing CLS-* semantics where instance-property diff is handled by
        // other rules (CLS-07, etc.).
        let is_removable_kind = matches!(
            old_member.kind,
            ClassMemberKind::Method
                | ClassMemberKind::Constructor
                | ClassMemberKind::Getter
                | ClassMemberKind::Setter
        ) || (matches!(old_member.kind, ClassMemberKind::Property)
            && old_member.is_static);
        if is_removable_kind && !new_members.contains_key(key) {
            let noun = if matches!(old_member.kind, ClassMemberKind::Property) {
                "static property"
            } else {
                "method"
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CLASS_METHOD_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "{} `{}` was removed from class `{}`",
                    noun, name, symbol_name
                ),
            });
        }
    }

    // A setter's apparent TypeScript property type does not prove the property
    // remains readable at runtime after its paired getter is removed.
    for getter in old.members.iter().filter(|member| {
        matches!(member.kind, ClassMemberKind::Getter)
            && !member.is_private
            && member.visibility != Visibility::Private
    }) {
        let same_slot = |member: &&ClassMember| {
            member.name == getter.name && member.is_static == getter.is_static
        };
        if new
            .members
            .iter()
            .filter(same_slot)
            .any(|member| matches!(member.kind, ClassMemberKind::Setter))
            && !new.members.iter().filter(same_slot).any(|member| {
                matches!(
                    member.kind,
                    ClassMemberKind::Getter | ClassMemberKind::Property
                )
            })
        {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CLASS_METHOD_REMOVED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "getter `{}` was removed from class `{symbol_name}` while its setter remains",
                    getter.name
                ),
            });
        }
    }

    // CLS-05: Abstract method added (only meaningful on abstract classes)
    // Breaking: any concrete subclass that doesn't implement the new abstract
    // method becomes invalid.
    for (key, new_member) in &new_members {
        let name = key.0;
        if new_member.is_abstract && !old_members.contains_key(key) {
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
    for (key, old_member) in &old_members {
        let name = key.0;
        if let Some(new_member) = new_members.get(key) {
            if old_member.visibility == Visibility::Private
                && new_member.visibility == Visibility::Private
            {
                continue;
            }
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

            // ECF-10: Method kind changed (method->getter, getter->method, etc.)
            // Breaking: call site syntax changes — `obj.foo()` vs `obj.foo`
            if old_member.kind != new_member.kind
                && !matches!(old_member.kind, ClassMemberKind::Constructor)
                && !matches!(new_member.kind, ClassMemberKind::Constructor)
            {
                let kind_str = |k: &ClassMemberKind| match k {
                    ClassMemberKind::Constructor => "constructor",
                    ClassMemberKind::Method => "method",
                    ClassMemberKind::Property => "property",
                    ClassMemberKind::Getter => "getter",
                    ClassMemberKind::Setter => "setter",
                };
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_CLASS_METHOD_KIND_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "`{}` in class `{}` changed from {} to {}",
                        name,
                        symbol_name,
                        kind_str(&old_member.kind),
                        kind_str(&new_member.kind)
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

            // TLF-06: accessor keyword changed
            // Breaking: changes call-site semantics (property access vs accessor protocol).
            // Severity: Warning (not Major) because callers still use property syntax.
            if old_member.is_accessor != new_member.is_accessor {
                let change = if new_member.is_accessor {
                    "added"
                } else {
                    "removed"
                };
                findings.push(Finding {
                    severity: Severity::Warning,
                    rule: RULE_CLASS_ACCESSOR_CHANGED,
                    file: file.to_path_buf(),
                    line,
                    col,
                    message: format!(
                        "accessor keyword {} on `{}` in class `{}`",
                        change, name, symbol_name
                    ),
                });
            }
        }
    }

    // IFT-03: Per-position constructor parameter optional→required check.
    // Compares old and new constructor_param_optionality vectors. For each
    // position where old was optional (true) and new is required (false), emit
    // constructor-param-made-required (Major).
    let old_opt = &old.constructor_param_optionality;
    let new_opt = &new.constructor_param_optionality;
    for (i, (old_o, new_o)) in old_opt.iter().zip(new_opt.iter()).enumerate() {
        if *old_o && !*new_o {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CONSTRUCTOR_PARAM_MADE_REQUIRED,
                file: file.to_path_buf(),
                line,
                col,
                message: format!(
                    "constructor parameter at position {} was made required in class `{}` (was optional)",
                    i, symbol_name
                ),
            });
        }
    }

    // ── CLS-10: class-level decorator diff ───────────────────────
    {
        let class_desc = format!("class `{}`", symbol_name);
        findings.extend(diff_decorators(
            &old.decorators,
            &new.decorators,
            &class_desc,
            file,
            line,
            col,
        ));
    }

    // ── CLS-10: per-member decorator diff ────────────────────────
    // Pair members by name (same key used elsewhere in this function). Only
    // diff decorators for members present on both sides — decorator-only
    // findings on a removed/added member would duplicate class-method-removed
    // (CLS-03) / class-method-added noise.
    //
    // Approach A private filter (CLS-11 / ): skip `#`-prefixed private
    // members on BOTH sides — decorator churn on `@log #count` is not
    // consumer-observable and must not leak into public-surface findings.
    {
        let old_by_name: std::collections::HashMap<(&str, bool), &ClassMember> = old
            .members
            .iter()
            .filter(|m| !m.is_private && m.visibility != Visibility::Private)
            .map(|m| ((m.name.as_str(), m.is_static), m))
            .collect();
        for new_member in &new.members {
            if new_member.is_private || new_member.visibility == Visibility::Private {
                continue;
            }
            if let Some(old_member) =
                old_by_name.get(&(new_member.name.as_str(), new_member.is_static))
            {
                let owner = format!(
                    "{} `{}.{}`",
                    member_kind_label(&new_member.kind),
                    symbol_name,
                    new_member.name
                );
                findings.extend(diff_decorators(
                    &old_member.decorators,
                    &new_member.decorators,
                    &owner,
                    file,
                    line,
                    col,
                ));
            }
        }
    }

    // ── CLS-12: class static initialization block limitation ─────
    // When either old or new had a computed-key assignment (`Foo[x] =...`)
    // in a `static {... }` block, surface a `limitation:` annotation so
    // consumers know dynamic-key members are not diffed. No new rule-ID —
    // host as Severity::Minor under RULE_CLASS_STATIC_TYPE_CHANGED with the
    // limitation ID embedded in the message and a paired informational finding.
    if old.has_dynamic_static_key || new.has_dynamic_static_key {
        findings.push(Finding {
            severity: Severity::Warning,
            rule: "analysis-incomplete",
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "computed member keys or conditional static assignments on class `{symbol_name}` require runtime information"
            ),
        });
        findings.push(Finding {
            severity: Severity::Minor,
            rule: RULE_CLASS_STATIC_TYPE_CHANGED,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "class `{}` contains unresolved computed keys or conditional static assignments \u{2014} the affected surface is not fully diffed (limitation: dynamic-static-member-key)",
                symbol_name,
            ),
        });
        // DOC-01: paired Info — dynamic-key limitation promoted to first-class.
        findings.push(Finding {
            severity: Severity::Info,
            rule: runtime_rules::RULE_LIMITATION_DYNAMIC_STATIC_MEMBER_KEY,
            file: file.to_path_buf(),
            line,
            col,
            message: format!(
                "unresolved keys or conditional static-block assignments on class `{}` are not fully enumerated",
                symbol_name
            ),
        });
    }

    findings
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
