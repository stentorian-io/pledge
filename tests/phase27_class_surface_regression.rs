//! Phase 27 regression tests: lock the three Phase 21 WR fixes.
//!
//! - WR-01 (commit 8c416ce): CLS-10 per-member decorator diff filters
//!   `!is_private` on both sides. Regression: `@log #count` →
//!   `@trace #count` must produce ZERO decorator findings.
//! - WR-02 (commit 2df8175): CLS-11 transition match keys on
//!   `(bare_name, is_static)`. Regression: `static foo` removed +
//!   instance `#foo` added on the same class must NOT fire
//!   class-field-made-private.
//! - WR-03 (commit d8f4bad): CLS-12 synthesized member from
//!   `static_block` preserves a class-body declaration's decorators.
//!   Regression: `@log static bar` (class-body) + `Foo.bar = 'y'`
//!   (static block) → `@trace static bar` must fire a decorator
//!   finding for `bar`. Without `class_body_wins`, the undecorated
//!   synthesized member replaces the decorated class-body entry,
//!   silently losing the decorator change.

use std::path::{Path, PathBuf};

use interlint::compare::subtype::{
    RULE_CLASS_FIELD_MADE_PRIVATE, RULE_DECORATOR_ADDED, RULE_DECORATOR_ARGUMENTS_CHANGED,
    RULE_DECORATOR_REMOVED,
};
use interlint::compare::{compare_exports, compare_structural_diff};
use interlint::discover::discover_exports;
use interlint::types::Finding;

const FIXTURE_ROOT: &str = "tests/fixtures/phase-27";

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

/// WR-01 regression: a decorator change confined to a `#`-private field
/// MUST NOT emit any public-surface decorator findings. If the
/// `!is_private` filter at src/compare/class_diff.rs:664-673 is ever
/// removed, the per-member loop would compare `@log #count` against
/// `@trace #count` and emit a `decorator-arguments-changed` (or
/// removed/added pair) — which would leak the private decorator change
/// to consumers.
#[test]
fn wr_01_private_decorator_change_does_not_leak() {
    let findings = run("cls-10-private-decorator-leak");
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

/// WR-02 regression: removing `static foo` and adding instance `#foo`
/// on the same class MUST NOT pair as a public-to-private transition.
/// The slots differ in `is_static` (class-object vs prototype) — they
/// are not the same logical member. If the keying at
/// src/compare/class_diff.rs:306-321 ever drops `is_static` from the
/// tuple, the transition pass would observe `foo` on the public-old
/// side and `foo` (bare-name) on the private-new side and fire
/// `class-field-made-private` — which would be wrong.
#[test]
fn wr_02_static_foo_and_instance_hash_foo_no_transition() {
    let findings = run("cls-11-static-vs-instance-collision");
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

/// WR-03 regression: a decorated `@log static bar` declared in the
/// class body MUST survive the `class_static_block` synthesis pass when
/// the same name is also written inside `static { Foo.bar = 'y'; }`.
/// If `class_body_wins` at src/discover/tree_query.rs:5256-5265 is ever
/// removed, the undecorated synthesized ClassMember replaces the
/// decorated class-body entry on both sides — the decorator diff sees
/// no decorators on either side and emits nothing. The presence of a
/// decorator finding on `bar` proves the decorated class-body member
/// survived the synthesis pass unchanged.
#[test]
fn wr_03_static_block_preserves_typed_class_body() {
    let findings = run("cls-12-typed-class-body-preserved");
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
