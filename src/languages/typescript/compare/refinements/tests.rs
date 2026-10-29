use crate::languages::typescript::compare::refinements::*;
use crate::languages::typescript::compare::rules as type_rules;
use crate::languages::typescript::types::shape::{ParamShape, PrimitiveKind};
use std::path::PathBuf;

fn make_sym(name: &str, kind: ExportKind, decl_kind: Option<DeclarationKind>) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind,
        file: PathBuf::from("test.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: decl_kind.map(|k| vec![k]).unwrap_or_default(),
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    }
}

/// helper for constructing symbols with multi-kind decl_kinds
/// (declaration-merging hybrids like [Class, Namespace]).
fn make_sym_multi(name: &str, kind: ExportKind, decl_kinds: Vec<DeclarationKind>) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind,
        file: PathBuf::from("test.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds,
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    }
}

#[test]
fn test_ref04_interface_to_type_alias() {
    let old = make_sym(
        "Config",
        ExportKind::Named,
        Some(DeclarationKind::Interface),
    );
    let new = make_sym(
        "Config",
        ExportKind::Named,
        Some(DeclarationKind::TypeAlias),
    );
    let findings = compare_ref_changes(
        &old,
        &new,
        "Config",
        &PathBuf::from("test.ts"),
        1,
        1,
        &[],
        &[],
    );
    assert!(findings
        .iter()
        .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED));
}

#[test]
fn test_ref05_namespace_to_variable() {
    let old = make_sym("MyNS", ExportKind::Named, Some(DeclarationKind::Namespace));
    let new = make_sym("MyNS", ExportKind::Named, Some(DeclarationKind::Variable));
    let findings = compare_ref_changes(
        &old,
        &new,
        "MyNS",
        &PathBuf::from("test.ts"),
        1,
        1,
        &[],
        &[],
    );
    assert!(findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER));
}

// ── REF-05 hybrid + empty-decl_kinds guard ──────────

#[test]
fn test_ref05_hybrid_class_namespace_does_not_fire() {
    //  /: class+namespace hybrid in new must NOT fire
    // RULE_NAMESPACE_TO_OTHER — the namespace facet is preserved.
    let old = make_sym_multi("X", ExportKind::Named, vec![DeclarationKind::Namespace]);
    let new = make_sym_multi(
        "X",
        ExportKind::Named,
        vec![DeclarationKind::Class, DeclarationKind::Namespace],
    );
    let findings = compare_ref_changes(&old, &new, "X", &PathBuf::from("x.ts"), 1, 1, &[], &[]);
    assert!(
        !findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER),
        "hybrid class+namespace must not fire RULE_NAMESPACE_TO_OTHER; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn test_ref05_pure_namespace_to_class_fires() {
    // Old: pure namespace. New: pure class (no namespace kind). Must fire REF-05.
    let old = make_sym_multi("X", ExportKind::Named, vec![DeclarationKind::Namespace]);
    let new = make_sym_multi("X", ExportKind::Named, vec![DeclarationKind::Class]);
    let findings = compare_ref_changes(&old, &new, "X", &PathBuf::from("x.ts"), 1, 1, &[], &[]);
    assert!(
        findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER),
        "pure namespace→class must fire REF-05; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

#[test]
fn test_ref05_empty_new_decl_kinds_does_not_fire() {
    // empty new.decl_kinds (e.g., re-export with unknown kind) is
    // the pre-classification gap — this was decl_kind: None and
    // REF-05 did NOT fire. Must preserve that semantics.
    let old = make_sym_multi("X", ExportKind::Named, vec![DeclarationKind::Namespace]);
    let new = make_sym_multi("X", ExportKind::Named, Vec::new());
    let findings = compare_ref_changes(&old, &new, "X", &PathBuf::from("x.ts"), 1, 1, &[], &[]);
    assert!(
            !findings.iter().any(|f| f.rule == RULE_NAMESPACE_TO_OTHER),
            "empty new.decl_kinds must NOT fire REF-05 (Warning 4 pre-classification-gap semantics); got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
}

#[test]
fn test_ref03_type_guard_widened() {
    let mut old = make_sym("isStr", ExportKind::Named, Some(DeclarationKind::Function));
    old.type_guard_type = Some("string".to_string());
    let new = make_sym("isStr", ExportKind::Named, Some(DeclarationKind::Function));
    let findings = compare_ref_changes(
        &old,
        &new,
        "isStr",
        &PathBuf::from("test.ts"),
        1,
        1,
        &[],
        &[],
    );
    assert!(findings.iter().any(|f| f.rule == RULE_TYPE_GUARD_WIDENED));
}

#[test]
fn test_ref07_index_sig_removed() {
    let old = make_sym("Dict", ExportKind::Named, Some(DeclarationKind::Interface));
    let new = make_sym("Dict", ExportKind::Named, Some(DeclarationKind::Interface));
    let old_sigs = vec![IndexSignature {
        key_type_text: "string".into(),
        value_type_text: "number".into(),
    }];
    let findings = compare_ref_changes(
        &old,
        &new,
        "Dict",
        &PathBuf::from("test.ts"),
        1,
        1,
        &old_sigs,
        &[],
    );
    assert!(findings
        .iter()
        .any(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED));
}

#[test]
fn test_ref07_index_sig_value_changed() {
    let old = make_sym("Dict", ExportKind::Named, Some(DeclarationKind::Interface));
    let new = make_sym("Dict", ExportKind::Named, Some(DeclarationKind::Interface));
    let old_sigs = vec![IndexSignature {
        key_type_text: "string".into(),
        value_type_text: "number".into(),
    }];
    let new_sigs = vec![IndexSignature {
        key_type_text: "string".into(),
        value_type_text: "string".into(),
    }];
    let findings = compare_ref_changes(
        &old,
        &new,
        "Dict",
        &PathBuf::from("test.ts"),
        1,
        1,
        &old_sigs,
        &new_sigs,
    );
    assert!(findings
        .iter()
        .any(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED));
    let f = findings
        .iter()
        .find(|f| f.rule == RULE_INDEX_SIGNATURE_CHANGED)
        .unwrap();
    assert!(f.message.contains("number") && f.message.contains("string"));
}

#[test]
fn test_ref08_default_export_kind_changed() {
    let old = make_sym(
        "default",
        ExportKind::Default,
        Some(DeclarationKind::Function),
    );
    let new = make_sym("default", ExportKind::Default, Some(DeclarationKind::Class));
    let findings = compare_ref_changes(
        &old,
        &new,
        "default",
        &PathBuf::from("test.ts"),
        1,
        1,
        &[],
        &[],
    );
    assert!(findings
        .iter()
        .any(|f| f.rule == RULE_DEFAULT_EXPORT_CHANGED));
}

#[test]
fn test_ref01_overload_removed() {
    let mut old = make_sym("foo", ExportKind::Named, Some(DeclarationKind::Function));
    old.overloads = vec![
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Opaque("string".into()),
            }],
            return_type: Box::new(TypeShape::Opaque("string".into())),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![
                ParamShape {
                    name: "x".into(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Opaque("number".into()),
                },
                ParamShape {
                    name: "y".into(),
                    optional: false,
                    rest: false,
                    shape: TypeShape::Opaque("number".into()),
                },
            ],
            return_type: Box::new(TypeShape::Opaque("number".into())),
        },
    ];
    let mut new = make_sym("foo", ExportKind::Named, Some(DeclarationKind::Function));
    new.overloads = vec![TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "x".into(),
            optional: false,
            rest: false,
            shape: TypeShape::Opaque("string".into()),
        }],
        return_type: Box::new(TypeShape::Opaque("string".into())),
    }];
    // Old had arity 1 and arity 2; new only has arity 1 -> arity 2 overload removed
    let findings =
        compare_ref_changes(&old, &new, "foo", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
    assert!(findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED));
}

#[test]
fn test_ref02_rest_param_removed() {
    let mut old = make_sym("bar", ExportKind::Named, Some(DeclarationKind::Function));
    old.overloads = vec![TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "args".into(),
            optional: false,
            rest: true,
            shape: TypeShape::Opaque("string[]".into()),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
    }];
    let mut new = make_sym("bar", ExportKind::Named, Some(DeclarationKind::Function));
    new.overloads = vec![TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "x".into(),
            optional: false,
            rest: false,
            shape: TypeShape::Opaque("string".into()),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
    }];
    let findings =
        compare_ref_changes(&old, &new, "bar", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
    assert!(findings.iter().any(|f| f.rule == RULE_REST_PARAM_NARROWED));
}

#[test]
fn test_ref01_overload_same_arity_different_types() {
    // Old has two arity-1 overloads: fn(string) and fn(number).
    // New only has fn(string). The fn(number) overload should be detected as removed.
    let mut old = make_sym("foo", ExportKind::Named, Some(DeclarationKind::Function));
    old.overloads = vec![
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    ];
    let mut new = make_sym("foo", ExportKind::Named, Some(DeclarationKind::Function));
    new.overloads = vec![TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "x".into(),
            optional: false,
            rest: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
    }];
    // fn(number) overload is gone — must fire overload-removed
    let findings =
        compare_ref_changes(&old, &new, "foo", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
    assert!(
        findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED),
        "Expected overload-removed for fn(number) removal, got: {:?}",
        findings
    );
}

#[test]
fn test_ref04_class_to_function() {
    let old = make_sym("Foo", ExportKind::Named, Some(DeclarationKind::Class));
    let new = make_sym("Foo", ExportKind::Named, Some(DeclarationKind::Function));
    let findings =
        compare_ref_changes(&old, &new, "Foo", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
        "Expected declaration-kind-changed for class->function, got: {:?}",
        findings
    );
}

#[test]
fn test_ref04_enum_to_interface() {
    let old = make_sym("MyEnum", ExportKind::Named, Some(DeclarationKind::Enum));
    let new = make_sym(
        "MyEnum",
        ExportKind::Named,
        Some(DeclarationKind::Interface),
    );
    let findings = compare_ref_changes(
        &old,
        &new,
        "MyEnum",
        &PathBuf::from("test.ts"),
        1,
        1,
        &[],
        &[],
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
        "Expected declaration-kind-changed for enum->interface, got: {:?}",
        findings
    );
}

#[test]
fn test_ref04_function_to_function_no_finding() {
    // Same kind -> no declaration-kind-changed finding
    let old = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
    let new = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
    let findings = compare_ref_changes(&old, &new, "fn", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
        "Expected no declaration-kind-changed for function->function, got: {:?}",
        findings
    );
}

// ── TLF-03: this-param type changed ──────────────────────────────────────

#[test]
fn this_param_changed_fires_major() {
    let findings = compare_this_param(
        Some("HTMLElement"),
        Some("Element"),
        "onClick",
        &PathBuf::from("test.ts"),
        1,
        1,
    );
    assert_eq!(
        findings.len(),
        1,
        "changed this-param should fire; got: {:?}",
        findings
    );
    assert_eq!(findings[0].rule, type_rules::RULE_THIS_PARAM_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
}

#[test]
fn added_receiver_requirement_fires_major() {
    let findings = compare_this_param(
        None,
        Some("HTMLElement"),
        "onClick",
        &PathBuf::from("test.ts"),
        1,
        1,
    );
    assert_eq!(
        findings.len(),
        1,
        "added receiver requirement should fire; got: {:?}",
        findings
    );
    assert_eq!(findings[0].rule, type_rules::RULE_THIS_PARAM_CHANGED);
}

#[test]
fn this_param_unchanged_no_finding() {
    let findings = compare_this_param(
        Some("HTMLElement"),
        Some("HTMLElement"),
        "onClick",
        &PathBuf::from("test.ts"),
        1,
        1,
    );
    assert!(
        findings.is_empty(),
        "identical this-param should produce no findings"
    );
}

// ── TLF-04: assertion target widened ─────────────────────────────────────

#[test]
fn assertion_removed_fires_major() {
    let findings = compare_assertion(
        Some("string"),
        None,
        "assertIsString",
        &PathBuf::from("test.ts"),
        1,
        1,
    );
    assert_eq!(
        findings.len(),
        1,
        "removed assertion should fire; got: {:?}",
        findings
    );
    assert_eq!(findings[0].rule, type_rules::RULE_ASSERTION_WIDENED);
    assert_eq!(findings[0].severity, Severity::Major);
}

#[test]
fn assertion_unchanged_no_finding() {
    let findings = compare_assertion(
        Some("string"),
        Some("string"),
        "assertIsString",
        &PathBuf::from("test.ts"),
        1,
        1,
    );
    assert!(
        findings.is_empty(),
        "identical assertion should produce no findings"
    );
}

#[test]
fn no_assertion_no_finding() {
    let findings = compare_assertion(None, None, "fn", &PathBuf::from("test.ts"), 1, 1);
    assert!(
        findings.is_empty(),
        "no assertion on either side should produce no findings"
    );
}

#[test]
fn test_ref02_rest_param_widened_no_finding() {
    // Old has rest param Opaque("string[]"), new has rest param Primitive(Any).
    // Widening to any[] is safe — should NOT fire rest-param-narrowed.
    let mut old = make_sym("bar", ExportKind::Named, Some(DeclarationKind::Function));
    old.overloads = vec![TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "args".into(),
            optional: false,
            rest: true,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
    }];
    let mut new = make_sym("bar", ExportKind::Named, Some(DeclarationKind::Function));
    new.overloads = vec![TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "args".into(),
            optional: false,
            rest: true,
            shape: TypeShape::Primitive(PrimitiveKind::Any),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
    }];
    let findings =
        compare_ref_changes(&old, &new, "bar", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
    assert!(
        !findings.iter().any(|f| f.rule == RULE_REST_PARAM_NARROWED),
        "Expected no rest-param-narrowed when widening to any, got: {:?}",
        findings
    );
}

// ── SEM-05: REF-04/REF-08 dedup ──────────────────────────────────────────

#[test]
fn ref_04_suppressed_when_ref_08_fires() {
    // Both REF-04 (decl kind flips) AND REF-08 (default export kind change) would fire.
    // SEM-05: only REF-08 should be emitted for the same symbol.
    let mut old = make_sym("default", ExportKind::Default, Some(DeclarationKind::Class));
    old.overloads = vec![];
    let mut new = make_sym(
        "default",
        ExportKind::Default,
        Some(DeclarationKind::Function),
    );
    new.overloads = vec![];
    let findings = compare_ref_changes(
        &old,
        &new,
        "default",
        &PathBuf::from("test.ts"),
        1,
        1,
        &[],
        &[],
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_DEFAULT_EXPORT_CHANGED),
        "SEM-05: REF-08 (default-export-changed) must fire; got: {:?}",
        findings
    );
    assert!(
            !findings.iter().any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
            "SEM-05: REF-04 (declaration-kind-changed) must be suppressed when REF-08 fires for same symbol; got: {:?}",
            findings
        );
}

#[test]
fn ref_04_fires_alone_when_ref_08_does_not() {
    // Only decl kind flip, NOT a default export — REF-04 alone fires normally.
    let old = make_sym(
        "Config",
        ExportKind::Named,
        Some(DeclarationKind::Interface),
    );
    let new = make_sym(
        "Config",
        ExportKind::Named,
        Some(DeclarationKind::TypeAlias),
    );
    let findings = compare_ref_changes(
        &old,
        &new,
        "Config",
        &PathBuf::from("test.ts"),
        1,
        1,
        &[],
        &[],
    );
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_DECLARATION_KIND_CHANGED),
        "SEM-05: REF-04 must fire when REF-08 does not; got: {:?}",
        findings
    );
}

// ── SEM-07: Shape-aware overload matching with consumed tracking ──────────

#[test]
fn unmatched_overload_fires_removed() {
    // Old: [fn(string), fn(number)]; New: [fn(string)]
    // fn(number) overload has no match in new → overload-removed
    let mut old = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
    old.overloads = vec![
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    ];
    let mut new = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
    new.overloads = vec![TypeShape::Function {
        contract: Default::default(),
        params: vec![ParamShape {
            name: "x".into(),
            optional: false,
            rest: false,
            shape: TypeShape::Primitive(PrimitiveKind::String),
        }],
        return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
    }];
    let findings = compare_ref_changes(&old, &new, "fn", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
    assert!(
        findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED),
        "SEM-07: fn(number) overload removal must fire overload-removed; got: {:?}",
        findings
    );
    // Should fire exactly once (one removed overload)
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_OVERLOAD_REMOVED)
            .count(),
        1,
        "SEM-07: exactly one overload-removed expected; got: {:?}",
        findings
    );
}

#[test]
fn matched_shapes_no_removal() {
    // Both sides: [fn(string)] — same shape, no removal
    let mut old = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
    old.overloads = vec![
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    ];
    let mut new = make_sym("fn", ExportKind::Named, Some(DeclarationKind::Function));
    new.overloads = vec![
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::String),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
        TypeShape::Function {
            contract: Default::default(),
            params: vec![ParamShape {
                name: "x".into(),
                optional: false,
                rest: false,
                shape: TypeShape::Primitive(PrimitiveKind::Number),
            }],
            return_type: Box::new(TypeShape::Primitive(PrimitiveKind::Void)),
        },
    ];
    let findings = compare_ref_changes(&old, &new, "fn", &PathBuf::from("test.ts"), 1, 1, &[], &[]);
    assert!(
        !findings.iter().any(|f| f.rule == RULE_OVERLOAD_REMOVED),
        "SEM-07: matching overloads on both sides should not fire overload-removed; got: {:?}",
        findings
    );
}

#[test]
fn later_overloads_preserve_returns_and_parameter_acceptance() {
    use crate::languages::typescript::types::parser::parse_type_string;

    for (before, after, breaking) in [
        ("(x: number) => number", "(x: number) => string", true),
        ("(x: number | boolean) => void", "(x: number) => void", true),
        (
            "(x: number) => string | number",
            "(x: number) => string",
            false,
        ),
        (
            "(x: number) => void",
            "(x: number | boolean) => void",
            false,
        ),
    ] {
        let first = parse_type_string("(x: string) => string");
        let mut old = make_sym("api", ExportKind::Named, Some(DeclarationKind::Function));
        old.overloads = vec![first.clone(), parse_type_string(before)];
        let mut new = old.clone();
        new.overloads = vec![first, parse_type_string(after)];

        let findings = compare_ref_changes(
            &old,
            &new,
            "api",
            &PathBuf::from("index.ts"),
            1,
            1,
            &[],
            &[],
        );

        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            breaking,
            "{before} -> {after}: {findings:?}"
        );
    }
}
