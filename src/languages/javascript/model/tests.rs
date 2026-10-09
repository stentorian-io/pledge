use crate::core::findings;
use crate::languages::javascript::compare::rules as runtime_rules;
use crate::languages::javascript::model::*;
use crate::languages::typescript::compare::rules as type_rules;
use crate::languages::typescript::model as ts_model;
use std::path::PathBuf;

#[test]
fn test_rule_class_constructor_param_added_value() {
    assert_eq!(
        runtime_rules::RULE_CLASS_CONSTRUCTOR_PARAM_ADDED,
        "class-constructor-param-added"
    );
}

#[test]
fn test_rule_enum_member_removed_value() {
    assert_eq!(type_rules::RULE_ENUM_MEMBER_REMOVED, "enum-member-removed");
}

#[test]
fn test_rule_generic_param_added_value() {
    assert_eq!(type_rules::RULE_GENERIC_PARAM_ADDED, "generic-param-added");
}

#[test]
fn test_class_shape_construction() {
    let member = ClassMember {
        name: "doWork".to_string(),
        kind: ClassMemberKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly: false,
        param_count: 1,
        type_signature: None,
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
        is_private: false,
    };
    let shape = ClassShape {
        members: vec![member],
        is_abstract: false,
        extends_text: Some("Base".to_string()),
        implements_list: vec!["IFoo".to_string()],
        type_params: vec![],
        constructor_param_optionality: vec![],
        decorators: Vec::new(),
        has_dynamic_static_key: false,
    };
    assert_eq!(shape.members.len(), 1);
    assert!(!shape.is_abstract);
    assert_eq!(shape.extends_text.as_deref(), Some("Base"));
    assert_eq!(shape.implements_list.len(), 1);
}

#[test]
fn test_enum_shape_construction() {
    let member = ts_model::EnumMember {
        name: "Foo".to_string(),
        raw_value: None,
        computed_value: Some(0),
        value_kind: ts_model::EnumValueKind::Numeric,
    };
    let shape = ts_model::EnumShape {
        members: vec![member],
        is_const: true,
    };
    assert!(shape.is_const);
    assert_eq!(shape.members.len(), 1);
}

#[test]
fn test_type_param_construction() {
    let param = ts_model::TypeParam {
        name: "T".to_string(),
        constraint_text: Some("string".to_string()),
        default_text: Some("never".to_string()),
        constraint_shape: None,
        is_const: false,
        variance: None,
    };
    assert_eq!(param.name, "T");
    assert_eq!(param.constraint_text.as_deref(), Some("string"));
    assert_eq!(param.default_text.as_deref(), Some("never"));
    assert!(param.constraint_shape.is_none());
}

#[test]
fn test_visibility_public_is_default() {
    // Default visibility is Visibility::Public (TypeScript default)
    let vis = Visibility::Public;
    assert_eq!(vis, Visibility::Public);
    assert_ne!(vis, Visibility::Protected);
    assert_ne!(vis, Visibility::Private);
}

#[test]
fn test_enum_value_kind_variants() {
    let numeric = ts_model::EnumValueKind::Numeric;
    let string_lit = ts_model::EnumValueKind::StringLiteral;
    let other = ts_model::EnumValueKind::OtherExpr;
    assert_eq!(numeric, ts_model::EnumValueKind::Numeric);
    assert_eq!(string_lit, ts_model::EnumValueKind::StringLiteral);
    assert_eq!(other, ts_model::EnumValueKind::OtherExpr);
}

#[test]
fn test_class_member_kind_variants() {
    assert_eq!(ClassMemberKind::Constructor, ClassMemberKind::Constructor);
    assert_eq!(ClassMemberKind::Method, ClassMemberKind::Method);
    assert_eq!(ClassMemberKind::Property, ClassMemberKind::Property);
    assert_ne!(ClassMemberKind::Constructor, ClassMemberKind::Method);
    assert_eq!(ClassMemberKind::Getter, ClassMemberKind::Getter);
    assert_eq!(ClassMemberKind::Setter, ClassMemberKind::Setter);
    assert_ne!(ClassMemberKind::Getter, ClassMemberKind::Method);
    assert_ne!(ClassMemberKind::Setter, ClassMemberKind::Getter);
}

#[test]
fn typeof_import_ref_constructs_cleanly() {
    // INF-04: TypeofImportRef sidecar struct construction +
    // default `typeof_import_refs == Vec::new()` on ExportSymbol.
    let r = ts_model::TypeofImportRef {
        specifier: "./y".into(),
        property: "Z".into(),
        property_line: 1,
        property_col: 40,
    };
    assert_eq!(r.specifier, "./y");
    assert_eq!(r.property, "Z");
    assert_eq!(r.property_line, 1);
    assert_eq!(r.property_col, 40);
}

#[test]
fn export_symbol_default_typeof_refs_empty() {
    // INF-04: a freshly-constructed ExportSymbol has an empty
    // typeof_import_refs Vec by default.
    let sym = ExportSymbol {
        name: "foo".to_string(),
        kind: ExportKind::Named,
        file: PathBuf::from("test.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: Vec::new(),
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
    };
    assert!(sym.typeof_import_refs.is_empty());
}

#[test]
fn test_export_symbol_unchanged() {
    // Ensure existing ExportSymbol still compiles correctly after changes
    let sym = ExportSymbol {
        name: "foo".to_string(),
        kind: ExportKind::Named,
        file: PathBuf::from("src/index.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: Vec::new(),
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
    };
    assert_eq!(sym.name, "foo");
}

#[test]
fn test_severity_warning_sorts_last() {
    let mut severities = vec![
        findings::Severity::Warning,
        findings::Severity::Major,
        findings::Severity::Patch,
        findings::Severity::Minor,
    ];
    severities.sort();
    assert_eq!(
        severities,
        vec![
            findings::Severity::Major,
            findings::Severity::Minor,
            findings::Severity::Patch,
            findings::Severity::Warning
        ]
    );
}

#[test]
fn test_severity_info_sorts_last() {
    let mut severities = vec![
        findings::Severity::Info,
        findings::Severity::Warning,
        findings::Severity::Major,
        findings::Severity::Patch,
        findings::Severity::Minor,
    ];
    severities.sort();
    assert_eq!(
        severities,
        vec![
            findings::Severity::Major,
            findings::Severity::Minor,
            findings::Severity::Patch,
            findings::Severity::Warning,
            findings::Severity::Info
        ]
    );
}

#[test]
fn test_export_symbol_async_generator_fields() {
    let sym = ExportSymbol {
        name: "foo".to_string(),
        kind: ExportKind::Named,
        file: PathBuf::from("test.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: vec![DeclarationKind::Function],
        type_guard_type: None,
        is_async: true,
        is_generator: true,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    };
    assert!(sym.is_async);
    assert!(sym.is_generator);
}

#[test]
fn variance_variants_exist() {
    let _in_var = ts_model::Variance::In;
    let _out_var = ts_model::Variance::Out;
    let _inout_var = ts_model::Variance::InOut;
    assert_eq!(ts_model::Variance::In, ts_model::Variance::In);
    assert_ne!(ts_model::Variance::In, ts_model::Variance::Out);
    assert_ne!(ts_model::Variance::In, ts_model::Variance::InOut);
    assert_ne!(ts_model::Variance::Out, ts_model::Variance::InOut);
}

#[test]
fn class_member_is_accessor_defaults_false() {
    let m = ClassMember {
        name: "foo".into(),
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
        decorators: Vec::new(),
        is_private: false,
    };
    assert!(!m.is_accessor);
    assert!(!m.is_optional);
    assert!(m.this_param_type.is_none());
}

#[test]
fn construct_signature_struct_shape() {
    let cs = ts_model::ConstructSignature {
        param_count: 2,
        return_text: Some("Foo".into()),
        is_abstract: false,
    };
    assert_eq!(cs.param_count, 2);
    assert_eq!(cs.return_text.as_deref(), Some("Foo"));
    assert!(!cs.is_abstract);
}

#[test]
fn export_symbol_is_type_only_field_exists() {
    let sym = ExportSymbol {
        name: "mod".to_string(),
        kind: ExportKind::Named,
        file: PathBuf::from("test.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: Vec::new(),
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: true,
        is_type_only: true,
        release_tag: None,
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    };
    assert!(sym.is_type_only);
}

#[test]
fn tlf_type_param_variance_field_exists() {
    let tp = ts_model::TypeParam {
        name: "T".to_string(),
        constraint_text: None,
        default_text: None,
        constraint_shape: None,
        is_const: false,
        variance: Some(ts_model::Variance::In),
    };
    assert_eq!(tp.variance, Some(ts_model::Variance::In));
}

#[test]
fn tlf_release_tag_variants_exist() {
    let _ = ts_model::ReleaseTag::Internal;
    let _ = ts_model::ReleaseTag::Alpha;
    let _ = ts_model::ReleaseTag::Beta;
    let _ = ts_model::ReleaseTag::Public;
    let _ = ts_model::ReleaseTag::Deprecated;
    assert_eq!(
        ts_model::ReleaseTag::Internal,
        ts_model::ReleaseTag::Internal
    );
    assert_ne!(ts_model::ReleaseTag::Internal, ts_model::ReleaseTag::Public);
}

#[test]
fn decorator_spec_construction() {
    let d = ts_model::DecoratorSpec {
        name: "Component".to_string(),
        args: None,
    };
    assert_eq!(d.name, "Component");
    assert_eq!(d.args, None);
    let d2 = ts_model::DecoratorSpec {
        name: "Route".to_string(),
        args: Some("('/users')".to_string()),
    };
    assert_eq!(d2.args.as_deref(), Some("('/users')"));
}

// ───: ExportKey, SymbolSpace, decl_kinds ───────────────────────

#[test]
fn test_export_key_new_root_value_canonical_shape() {
    let k = ExportKey::new_root_value("foo");
    assert_eq!(k.subpath, ".");
    assert_eq!(k.condition, None);
    assert_eq!(k.space, SymbolSpace::Value);
    assert_eq!(k.name, "foo");
}

#[test]
fn test_export_key_hash_eq_same_key_equal() {
    let a = ExportKey::new("./sub", Some("import"), SymbolSpace::Value, "foo");
    let b = ExportKey::new("./sub", Some("import"), SymbolSpace::Value, "foo");
    assert_eq!(a, b);
    use std::collections::HashMap;
    let mut m: HashMap<ExportKey, u32> = HashMap::new();
    m.insert(a.clone(), 1);
    m.insert(b, 2);
    assert_eq!(m.len(), 1); // same key overwrites
    assert_eq!(m.get(&a), Some(&2));
}

#[test]
fn test_export_key_subpath_distinguishes() {
    let a = ExportKey::new(".", None, SymbolSpace::Value, "foo");
    let b = ExportKey::new("./sub", None, SymbolSpace::Value, "foo");
    assert_ne!(a, b);
}

#[test]
fn test_export_key_space_distinguishes() {
    let v = ExportKey::new(".", None, SymbolSpace::Value, "Foo");
    let t = ExportKey::new(".", None, SymbolSpace::Type, "Foo");
    assert_ne!(v, t);
}

#[test]
fn test_export_key_condition_distinguishes() {
    let import = ExportKey::new(".", Some("import"), SymbolSpace::Value, "foo");
    let require = ExportKey::new(".", Some("require"), SymbolSpace::Value, "foo");
    assert_ne!(import, require);
}

#[test]
fn test_symbol_space_variants_derive_hash() {
    use std::collections::HashSet;
    let mut s: HashSet<SymbolSpace> = HashSet::new();
    s.insert(SymbolSpace::Type);
    s.insert(SymbolSpace::Value);
    s.insert(SymbolSpace::Namespace);
    s.insert(SymbolSpace::Value); // duplicate insert
    assert_eq!(s.len(), 3);
}

#[test]
fn test_hybrid_decl_kinds_carries_multiple() {
    // Vec<DeclarationKind> preserves hybrid identity (class+namespace).
    let sym = ExportSymbol {
        name: "User".to_string(),
        kind: ExportKind::Named,
        file: std::path::PathBuf::from("user.ts"),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: vec![DeclarationKind::Class, DeclarationKind::Namespace],
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
    };
    assert_eq!(sym.decl_kinds.len(), 2);
    assert!(sym.decl_kinds.contains(&DeclarationKind::Class));
    assert!(sym.decl_kinds.contains(&DeclarationKind::Namespace));
}

// ───: ExportMap structural guard tests ──────────

fn make_sym(file: &str) -> ExportSymbol {
    ExportSymbol {
        name: "x".to_string(),
        kind: ExportKind::Named,
        file: PathBuf::from(file),
        line: 1,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: Vec::new(),
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

/// A directives entry whose key matches no ExportSymbol.file is
/// legal (pure-directive rollup chunk). Must NOT panic.
#[test]
fn directive_only_file_is_legal() {
    let mut map = ExportMap::new();
    // Insert a symbol for "src/x.ts"
    let key = ExportKey::new_root_value("x");
    map.inner.insert(key, make_sym("src/x.ts"));
    // Insert a directive for a DIFFERENT file (no matching ExportSymbol)
    map.directives
        .insert(PathBuf::from("dist/chunks/chunk-*.js"), vec![]);
    // debug_assert_invariants is not called by directives mutation (by design);
    // call it explicitly to assert it does not panic.
    map.debug_assert_invariants();
}

/// Overwriting the same (ExportKey, ExportSymbol) via inner.insert
/// (HashMap semantics: overwrite, not desync) must NOT panic.
#[test]
fn overwrite_inner_is_safe() {
    let mut map = ExportMap::new();
    let key = ExportKey::new_root_value("y");
    map.inner.insert(key.clone(), make_sym("src/y.ts"));
    // Overwrite with a different symbol at the same key
    map.inner.insert(key, make_sym("src/y2.ts"));
    map.debug_assert_invariants();
}

/// record_typeof_target called with a key that IS present in inner
/// must NOT panic.
#[test]
fn record_typeof_target_with_inner_present_ok() {
    let mut map = ExportMap::new();
    let key = ExportKey::new_root_value("z");
    map.inner.insert(key.clone(), make_sym("src/z.ts"));
    map.record_typeof_target(&key, 0, None);
    // record_typeof_target calls debug_assert_invariants internally;
    // reaching here means it did not panic.
}

/// typeof_targets referencing a key NOT in inner is a desync;
/// debug_assert_invariants MUST panic in debug builds.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "WR-01 invariant violated")]
fn orphan_typeof_target_panics() {
    let mut map = ExportMap::new();
    let key = ExportKey::new_root_value("orphan");
    // Inject directly into typeof_targets WITHOUT adding to inner
    map.typeof_targets.entry(key).or_default().insert(0, None);
    // This call must panic because typeof_targets has a key absent from inner
    map.debug_assert_invariants();
}
