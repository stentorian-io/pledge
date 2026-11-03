//! Private, static, and instance members occupy distinct public API slots.
//! Declared members retain their decorators when a static block assigns to them.

use pledge::core::findings::Finding;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::compare::rules::RULE_CLASS_FIELD_MADE_PRIVATE;
use pledge::languages::javascript::compare::structure::compare_structural_diff;
use pledge::languages::typescript::compare::rules::{
    RULE_DECORATOR_ADDED, RULE_DECORATOR_ARGUMENTS_CHANGED, RULE_DECORATOR_REMOVED,
};
use std::path::{Path, PathBuf};

const FIXTURE_ROOT: &str = "tests/fixtures";

fn fixture_dir(form: &str, side: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join(FIXTURE_ROOT).join(form).join(side)
}

fn run(form: &str) -> Vec<Finding> {
    let old_dir = fixture_dir(form, "old");
    let new_dir = fixture_dir(form, "new");
    let (old_exports, _) = discover_exports(&old_dir).unwrap();
    let (new_exports, _) = discover_exports(&new_dir).unwrap();
    let mut findings = compare_exports(&old_exports, &new_exports, &new_dir, true);
    findings.extend(compare_structural_diff(
        &old_dir,
        &new_dir,
        &old_exports,
        &new_exports,
    ));
    findings
}

/// regression: a decorator change confined to a `#`-private field
/// MUST NOT emit any public-surface decorator findings. If the
/// `!is_private` filter at src/languages/javascript/compare/classes.rs is ever
/// removed, the per-member loop would compare `@log #count` against
/// `@trace #count` and emit a `decorator-arguments-changed` (or
/// removed/added pair) — which would leak the private decorator change
/// to consumers.
#[test]
fn private_decorator_change_does_not_leak() {
    let findings = run("typescript/classes/identity/private-decorators");
    let decorator_hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == RULE_DECORATOR_ADDED
                || f.rule == RULE_DECORATOR_REMOVED
                || f.rule == RULE_DECORATOR_ARGUMENTS_CHANGED
        })
        .collect();
    assert_eq!(
        decorator_hits.len(),
        0,
        "WR-01 regression: private decorator change leaked as public finding(s); got: {:?}",
        decorator_hits
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

/// regression: removing `static foo` and adding instance `#foo`
/// on the same class MUST NOT pair as a public-to-private transition.
/// The slots differ in `is_static` (class-object vs prototype) — they
/// are not the same logical member. If the keying at
/// src/languages/javascript/compare/classes.rs ever drops `is_static` from the
/// tuple, the transition pass would observe `foo` on the public-old
/// side and `foo` (bare-name) on the private-new side and fire
/// `class-field-made-private` — which would be wrong.
#[test]
fn static_foo_and_instance_hash_foo_no_transition() {
    let findings = run("typescript/classes/identity/static-instance-collision");
    let transition_hits: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CLASS_FIELD_MADE_PRIVATE)
        .collect();
    assert_eq!(
        transition_hits.len(),
        0,
        "WR-02 regression: static foo + instance #foo paired as transition; got: {:?}",
        transition_hits
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

/// regression: a decorated `@log static bar` declared in the
/// class body MUST survive the `class_static_block` synthesis pass when
/// the same name is also written inside `static { Foo.bar = 'y'; }`.
/// If `class_body_wins` at src/languages/javascript/syntax/exports.rs is ever
/// removed, the undecorated synthesized ClassMember replaces the
/// decorated class-body entry on both sides — the decorator diff sees
/// no decorators on either side and emits nothing. The presence of a
/// decorator finding on `bar` proves the decorated class-body member
/// survived the synthesis pass unchanged.
#[test]
fn static_block_preserves_typed_class_body() {
    let findings = run("typescript/classes/identity/declared-static-members");
    let decorator_hits: Vec<_> = findings
        .iter()
        .filter(|f| {
            (f.rule == RULE_DECORATOR_ADDED
                || f.rule == RULE_DECORATOR_REMOVED
                || f.rule == RULE_DECORATOR_ARGUMENTS_CHANGED)
                && f.message.contains("bar")
        })
        .collect();
    assert!(
        !decorator_hits.is_empty(),
        "WR-03 regression: decorated class-body static `bar` was overwritten by undecorated synthesized member — decorator change lost; expected at least one decorator finding for `bar`, got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

fn package(source: &str, extension: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("package.json"), serde_json::json!({"name":"runtime-contracts","type":"module","exports":format!("./index.{extension}")}).to_string()).unwrap();
    std::fs::write(directory.path().join(format!("index.{extension}")), source).unwrap();
    directory
}

#[test]
fn static_decorator_changes_are_not_hidden_by_instance_namesakes() {
    let old = package("export class api { @before static use(){} use(){} }", "ts");
    let new = package("export class api { @after static use(){} use(){} }", "ts");
    let (before, _) = discover_exports(old.path()).unwrap();
    let (after, _) = discover_exports(new.path()).unwrap();
    let findings = compare_structural_diff(old.path(), new.path(), &before, &after);
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule.starts_with("decorator-")
                && finding.severity == pledge::core::findings::Severity::Major),
        "{findings:?}"
    );
}

#[test]
fn accessor_pairs_retain_their_identity_when_reordered() {
    for modifier in ["", "static "] {
        for decorator in ["", "@trace "] {
            let getter = format!("{decorator}{modifier}get value(): number {{ return 1; }}");
            let setter = format!("{modifier}set value(next: number) {{}}");
            let old = package(&format!("export class Api {{ {getter} {setter} }}"), "ts");
            let new = package(&format!("export class Api {{ {setter} {getter} }}"), "ts");
            let (before, warnings) = discover_exports(old.path()).unwrap();
            assert!(warnings.is_empty(), "{warnings:?}");
            let (after, warnings) = discover_exports(new.path()).unwrap();
            assert!(warnings.is_empty(), "{warnings:?}");

            let findings = compare_structural_diff(old.path(), new.path(), &before, &after);

            assert!(findings.is_empty(), "{modifier}{decorator}: {findings:?}");
        }
    }
}
