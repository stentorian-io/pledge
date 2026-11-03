//! Limitation identifiers compatibility checks.

use pledge::languages::javascript::compare::rules::{
    RULE_LIMITATION_DYNAMIC_STATIC_MEMBER_KEY, RULE_LIMITATION_PRIVATE_BRAND_ACCESS,
    RULE_LIMITATION_PROXY_DISPATCHED_EXPORTS, RULE_LIMITATION_SYMBOL_FOR_REGISTRY,
};
use pledge::languages::typescript::compare::rules::{
    RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT, RULE_LIMITATION_EXTERNAL_TYPE_REFERENCE,
    RULE_LIMITATION_LSP_REQUIRED, RULE_LIMITATION_NOINFER_VIA_LOCAL_ALIAS,
    RULE_LIMITATION_SATISFIES_NARROWED,
};

#[test]
fn all_nine_limitation_rule_ids_compile() {
    // The _ALL_LIMITATIONS const above provides the compile-time guard.
    // This test asserts the count at runtime as a triple-check.
    assert_eq!(
        _ALL_LIMITATIONS.len(),
        9,
        " DOC-01 enumerates 9 limitations"
    );
}

/// Compile-time enumeration of all 9 limitation rule-IDs. If any constant
/// is removed from subtype.rs, this binding fails to compile, surfacing the
/// regression at build time rather than runtime.
#[allow(dead_code)]
const _ALL_LIMITATIONS: &[&str] = &[
    RULE_LIMITATION_PROXY_DISPATCHED_EXPORTS,
    RULE_LIMITATION_DECORATOR_RUNTIME_EFFECT,
    RULE_LIMITATION_PRIVATE_BRAND_ACCESS,
    RULE_LIMITATION_SATISFIES_NARROWED,
    RULE_LIMITATION_LSP_REQUIRED,
    RULE_LIMITATION_DYNAMIC_STATIC_MEMBER_KEY,
    RULE_LIMITATION_EXTERNAL_TYPE_REFERENCE,
    RULE_LIMITATION_NOINFER_VIA_LOCAL_ALIAS,
    RULE_LIMITATION_SYMBOL_FOR_REGISTRY,
];
