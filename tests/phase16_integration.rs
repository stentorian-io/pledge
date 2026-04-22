//! Integration tests for Phase 16 — Detection Gaps and Interface Diff Hardening.
//!
//! These tests verify end-to-end behavior of all Phase 16 requirements:
//!   GAP-16-01: Optional parameter becoming required fires param-made-required
//!   GAP-16-02: Conditional/mapped/indexed/keyof type parsing (TypeShape variants)
//!   GAP-16-03: Interface property readonly change fires interface-readonly-changed
//!   GAP-16-04: Interface index signature change fires interface-index-signature-changed
//!   GAP-16-05: Nested namespace expansion (A.B, A.B.baz dotted names)
//!   GAP-16-06: Subpath wildcard expansion in exports field
//!   GAP-16-07: Exports array fallback — first existing file wins
//!   GAP-16-08: Nested destructuring export detection
//!   GAP-16-09: CJS exports aliasing pattern (exports = module.exports = {...})
//!   GAP-16-10: Interface method generic constraint tightening
//!   GAP-16-11: typesVersions wildcard expansion discovers .d.ts files
//!   GAP-16-12: browser field in package.json contributes entry point
//!
//! All tests exercise the public API at the same level as existing integration
//! tests in phase14_integration.rs and phase15_integration.rs.

use interlint::compare::interface_diff::{compare_interface_index_signatures, compare_interface_members};
use interlint::compare::type_diff::diff_types;
use interlint::discover::entry_points::resolve_entry_points;
use interlint::discover::tree_query::{parse_exports, IndexSignature, SourceLanguage};
use interlint::types::parser::parse_type_string;
use interlint::types::shape::{ParamShape, TypeShape};
use interlint::types::{ClassMember, ClassMemberKind, Visibility};
use std::path::PathBuf;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn make_property(name: &str, is_readonly: bool) -> ClassMember {
    ClassMember {
        name: name.to_string(),
        kind: ClassMemberKind::Property,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly,
        param_count: 0,
        type_signature: None,
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
    }
}

fn make_method(name: &str, param_count: usize, type_signature: Option<&str>) -> ClassMember {
    ClassMember {
        name: name.to_string(),
        kind: ClassMemberKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_readonly: false,
        param_count,
        type_signature: type_signature.map(|s| s.to_string()),
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
    }
}

fn make_index_sig(key: &str, val: &str) -> IndexSignature {
    IndexSignature {
        key_type_text: key.to_string(),
        value_type_text: val.to_string(),
    }
}

fn make_param(name: &str, optional: bool, shape: TypeShape) -> ParamShape {
    ParamShape {
        name: name.to_string(),
        optional,
        rest: false,
        shape,
    }
}

// ─── GAP-16-01: Optional parameter becoming required ────────────────────────

#[test]
fn gap_16_01_optional_to_required_param_fires_rule() {
    // Old: (x?: string) => void  (optional param)
    // New: (x: string) => void   (required param)
    // Expected: param-made-required Major finding
    let old_shape = TypeShape::Function {
        params: vec![make_param("x", true, TypeShape::Opaque("string".to_string()))],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };
    let new_shape = TypeShape::Function {
        params: vec![make_param("x", false, TypeShape::Opaque("string".to_string()))],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };

    let findings = diff_types(&old_shape, &new_shape, "myFunc", &test_file(), 1, 0);

    assert!(
        findings.iter().any(|f| f.rule == "param-made-required"),
        "optional->required param should fire param-made-required; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );

    let f = findings.iter().find(|f| f.rule == "param-made-required").unwrap();
    assert_eq!(
        f.severity,
        interlint::types::Severity::Major,
        "param-made-required should be Major"
    );
    assert!(
        f.message.contains("x"),
        "message should mention param name 'x'; got: {}",
        f.message
    );
}

#[test]
fn gap_16_01_required_to_optional_does_not_fire() {
    // Removing the required constraint is safe (callers can still pass the arg)
    let old_shape = TypeShape::Function {
        params: vec![make_param("x", false, TypeShape::Opaque("string".to_string()))],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };
    let new_shape = TypeShape::Function {
        params: vec![make_param("x", true, TypeShape::Opaque("string".to_string()))],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };

    let findings = diff_types(&old_shape, &new_shape, "myFunc", &test_file(), 1, 0);

    assert!(
        !findings.iter().any(|f| f.rule == "param-made-required"),
        "required->optional change should NOT fire param-made-required; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn gap_16_01_multiple_params_middle_made_required() {
    // Old: (a: string, b?: number, c?: boolean) => void
    // New: (a: string, b: number, c?: boolean) => void
    // Expected: param-made-required for 'b', no finding for unchanged params
    let old_shape = TypeShape::Function {
        params: vec![
            make_param("a", false, TypeShape::Opaque("string".to_string())),
            make_param("b", true, TypeShape::Opaque("number".to_string())),
            make_param("c", true, TypeShape::Opaque("boolean".to_string())),
        ],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };
    let new_shape = TypeShape::Function {
        params: vec![
            make_param("a", false, TypeShape::Opaque("string".to_string())),
            make_param("b", false, TypeShape::Opaque("number".to_string())),
            make_param("c", true, TypeShape::Opaque("boolean".to_string())),
        ],
        return_type: Box::new(TypeShape::Opaque("void".to_string())),
    };

    let findings = diff_types(&old_shape, &new_shape, "myFunc", &test_file(), 1, 0);

    let param_required_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "param-made-required")
        .collect();
    assert_eq!(
        param_required_findings.len(),
        1,
        "should have exactly 1 param-made-required finding for 'b'; got: {:?}",
        param_required_findings
    );
    assert!(
        param_required_findings[0].message.contains("b"),
        "finding should mention param 'b'; got: {}",
        param_required_findings[0].message
    );
}

// ─── GAP-16-02: Conditional/mapped/indexed type parsing ─────────────────────

#[test]
fn gap_16_02_conditional_type_parsing() {
    let shape = parse_type_string("T extends string ? number : boolean");
    assert!(
        matches!(shape, TypeShape::Conditional { .. }),
        "Should parse as Conditional, got {:?}",
        shape
    );
}

#[test]
fn gap_16_02_mapped_type_parsing() {
    let shape = parse_type_string("{ [K in keyof T]: string }");
    assert!(
        matches!(shape, TypeShape::Mapped { .. }),
        "Should parse as Mapped, got {:?}",
        shape
    );
}

#[test]
fn gap_16_02_indexed_access_type_parsing() {
    let shape = parse_type_string("T[\"key\"]");
    assert!(
        matches!(shape, TypeShape::Indexed { .. }),
        "Should parse as Indexed, got {:?}",
        shape
    );
}

#[test]
fn gap_16_02_keyof_type_parsing() {
    let shape = parse_type_string("keyof T");
    match &shape {
        TypeShape::Generic { name, args } => {
            assert_eq!(name, "keyof", "keyof should be parsed as Generic with name 'keyof'");
            assert_eq!(args.len(), 1, "keyof T should have 1 arg");
        }
        _ => panic!("Should parse keyof as Generic, got {:?}", shape),
    }
}

#[test]
fn gap_16_02_conditional_nested_parsing() {
    // More complex conditional type
    let shape = parse_type_string("T extends Array<infer U> ? U : never");
    assert!(
        matches!(shape, TypeShape::Conditional { .. }),
        "Nested conditional should parse as Conditional, got {:?}",
        shape
    );
}

// ─── GAP-16-03: Interface property readonly changes ──────────────────────────

#[test]
fn gap_16_03_readonly_added_fires_rule() {
    // Old: count: number (not readonly)
    // New: readonly count: number
    let old = vec![make_property("count", false)];
    let new = vec![make_property("count", true)];

    let findings = compare_interface_members(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings.iter().any(|f| f.rule == "interface-readonly-changed"),
        "Adding readonly should fire interface-readonly-changed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
    let f = findings.iter().find(|f| f.rule == "interface-readonly-changed").unwrap();
    assert_eq!(f.severity, interlint::types::Severity::Major, "should be Major");
    assert!(f.message.contains("added"), "message should say 'added'; got: {}", f.message);
    assert!(f.message.contains("count"), "message should mention property name; got: {}", f.message);
}

#[test]
fn gap_16_03_readonly_removed_fires_rule() {
    // Old: readonly count: number
    // New: count: number (readonly removed)
    let old = vec![make_property("count", true)];
    let new = vec![make_property("count", false)];

    let findings = compare_interface_members(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings.iter().any(|f| f.rule == "interface-readonly-changed"),
        "Removing readonly should fire interface-readonly-changed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
    let f = findings.iter().find(|f| f.rule == "interface-readonly-changed").unwrap();
    assert_eq!(f.severity, interlint::types::Severity::Major, "should be Major");
    assert!(f.message.contains("removed"), "message should say 'removed'; got: {}", f.message);
}

#[test]
fn gap_16_03_readonly_unchanged_no_finding() {
    // Both sides readonly — no finding expected
    let members = vec![make_property("x", true)];
    let findings = compare_interface_members(&members, &members, "MyInterface", &test_file(), 1, 0);

    assert!(
        !findings.iter().any(|f| f.rule == "interface-readonly-changed"),
        "Unchanged readonly should not fire; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

// ─── GAP-16-04: Interface index signature changes ────────────────────────────

#[test]
fn gap_16_04_index_signature_removed_fires_rule() {
    // Old: [key: string]: number
    // New: (no index signature)
    let old = vec![make_index_sig("string", "number")];
    let new: Vec<IndexSignature> = vec![];

    let findings = compare_interface_index_signatures(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert_eq!(findings.len(), 1, "removing index sig should produce 1 finding");
    assert_eq!(
        findings[0].rule, "interface-index-signature-changed",
        "rule should be interface-index-signature-changed; got: {}",
        findings[0].rule
    );
    assert_eq!(findings[0].severity, interlint::types::Severity::Major);
    assert!(
        findings[0].message.contains("removed"),
        "message should say 'removed'; got: {}",
        findings[0].message
    );
}

#[test]
fn gap_16_04_index_signature_value_type_changed_fires_rule() {
    // Old: [key: string]: number
    // New: [key: string]: string
    let old = vec![make_index_sig("string", "number")];
    let new = vec![make_index_sig("string", "string")];

    let findings = compare_interface_index_signatures(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert_eq!(
        findings.len(),
        1,
        "changing index sig value type should produce 1 finding"
    );
    assert_eq!(findings[0].rule, "interface-index-signature-changed");
    assert!(
        findings[0].message.contains("number") && findings[0].message.contains("string"),
        "message should show old and new value types; got: {}",
        findings[0].message
    );
}

#[test]
fn gap_16_04_index_signature_unchanged_no_finding() {
    let sigs = vec![make_index_sig("string", "number")];
    let findings = compare_interface_index_signatures(&sigs, &sigs, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings.is_empty(),
        "unchanged index signature should produce no finding; got: {:?}",
        findings
    );
}

// ─── GAP-16-05: Nested namespace expansion ───────────────────────────────────

#[test]
fn gap_16_05_nested_namespace_two_levels() {
    let source = b"export namespace A { export namespace B { export function baz(): void {} } }";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "A.B"),
        "Should have 'A.B' for nested namespace; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "A.B.baz"),
        "Should have 'A.B.baz' for doubly-nested function; got: {:?}",
        names
    );
}

#[test]
fn gap_16_05_nested_namespace_includes_parent() {
    let source = b"export namespace A { export namespace B { export const x = 1; } }";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    // Top-level namespace A should also be present
    assert!(
        exports.iter().any(|e| e.name == "A"),
        "Top-level namespace 'A' should also be exported; got: {:?}",
        names
    );
}

// ─── GAP-16-06: Subpath wildcard expansion ───────────────────────────────────

#[test]
fn gap_16_06_subpath_wildcard_expansion() {
    let pkg_dir = tempfile::tempdir().unwrap();

    // Create dist directory with JS files
    std::fs::create_dir_all(pkg_dir.path().join("dist")).unwrap();
    std::fs::write(pkg_dir.path().join("dist/alpha.js"), b"// alpha").unwrap();
    std::fs::write(pkg_dir.path().join("dist/beta.js"), b"// beta").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "exports": {
            "./*": "./dist/*.js"
        }
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();

    let paths: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        paths.contains(&"alpha.js".to_string()),
        "subpath wildcard ./* should expand to include alpha.js; got: {:?}",
        paths
    );
    assert!(
        paths.contains(&"beta.js".to_string()),
        "subpath wildcard ./* should expand to include beta.js; got: {:?}",
        paths
    );
}

// ─── GAP-16-07: Exports array fallback order ─────────────────────────────────

#[test]
fn gap_16_07_exports_array_first_existing_wins() {
    let pkg_dir = tempfile::tempdir().unwrap();

    // Only exists.js and also-exists.js are on disk; not-exist.js is absent
    std::fs::write(pkg_dir.path().join("exists.js"), b"// exists").unwrap();
    std::fs::write(pkg_dir.path().join("also-exists.js"), b"// also").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "exports": {
            ".": ["./not-exist.js", "./exists.js", "./also-exists.js"]
        }
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();
    let paths: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    // not-exist.js should be filtered out; exists.js should be present
    assert!(
        paths.contains(&"exists.js".to_string()),
        "first existing file should be in results; got: {:?}",
        paths
    );
    assert!(
        !paths.contains(&"not-exist.js".to_string()),
        "non-existent file should be filtered out; got: {:?}",
        paths
    );
}

// ─── GAP-16-08: Nested destructuring ─────────────────────────────────────────

#[test]
fn gap_16_08_nested_object_destructuring() {
    let source = b"export const { a: { b, c } } = obj;";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "b"),
        "Should capture 'b' from nested destructuring; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "c"),
        "Should capture 'c' from nested destructuring; got: {:?}",
        names
    );
}

#[test]
fn gap_16_08_nested_array_destructuring() {
    let source = b"export const [a, [b, c]] = arr;";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "a"),
        "Should capture 'a'; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "b"),
        "Should capture 'b' from nested array destructuring; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "c"),
        "Should capture 'c' from nested array destructuring; got: {:?}",
        names
    );
}

// ─── GAP-16-09: CJS exports aliasing ─────────────────────────────────────────

#[test]
fn gap_16_09_cjs_exports_aliasing_pattern() {
    // exports = module.exports = { foo, bar };
    let source = b"exports = module.exports = { foo, bar };";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "foo"),
        "Should capture 'foo' from CJS aliasing pattern; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "bar"),
        "Should capture 'bar' from CJS aliasing pattern; got: {:?}",
        names
    );
}

#[test]
fn gap_16_09_cjs_aliasing_multiple_properties() {
    // exports = module.exports = { foo, bar, baz };
    // All three properties should be captured
    let source = b"exports = module.exports = { foo, bar, baz };";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        exports.iter().any(|e| e.name == "foo"),
        "Should capture 'foo' from CJS aliasing; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "bar"),
        "Should capture 'bar' from CJS aliasing; got: {:?}",
        names
    );
    assert!(
        exports.iter().any(|e| e.name == "baz"),
        "Should capture 'baz' from CJS aliasing; got: {:?}",
        names
    );
}

// ─── GAP-16-10: Interface generic constraint tightening ──────────────────────

#[test]
fn gap_16_10_generic_constraint_tightened_no_constraint_to_constraint() {
    // Old: <T>(value: T) => void  (no constraint)
    // New: <T extends string>(value: T) => void  (constraint added)
    let old = vec![make_method("process", 1, Some("<T>(value: T) => void"))];
    let new = vec![make_method("process", 1, Some("<T extends string>(value: T) => void"))];

    let findings = compare_interface_members(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings.iter().any(|f| f.rule == "interface-generic-constraint-tightened"),
        "Adding extends constraint should fire interface-generic-constraint-tightened; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
    let f = findings
        .iter()
        .find(|f| f.rule == "interface-generic-constraint-tightened")
        .unwrap();
    assert_eq!(f.severity, interlint::types::Severity::Major, "should be Major");
    assert!(
        f.message.contains("process"),
        "message should mention method name; got: {}",
        f.message
    );
}

#[test]
fn gap_16_10_generic_constraint_changed_fires_rule() {
    // Old: <T extends object>(value: T) => void
    // New: <T extends string>(value: T) => void
    let old = vec![make_method("transform", 1, Some("<T extends object>(value: T) => void"))];
    let new = vec![make_method("transform", 1, Some("<T extends string>(value: T) => void"))];

    let findings = compare_interface_members(&old, &new, "MyInterface", &test_file(), 1, 0);

    assert!(
        findings.iter().any(|f| f.rule == "interface-generic-constraint-tightened"),
        "Changing extends constraint should fire interface-generic-constraint-tightened; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn gap_16_10_no_constraint_unchanged_no_finding() {
    let sig = "<T>(value: T) => void";
    let members = vec![make_method("process", 1, Some(sig))];
    let findings = compare_interface_members(&members, &members, "MyInterface", &test_file(), 1, 0);

    assert!(
        !findings.iter().any(|f| f.rule == "interface-generic-constraint-tightened"),
        "Unchanged signature should not fire; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

// ─── GAP-16-11: typesVersions wildcard expansion ─────────────────────────────

#[test]
fn gap_16_11_types_versions_wildcard_discovers_d_ts_files() {
    let pkg_dir = tempfile::tempdir().unwrap();

    // Create dist/types directory with .d.ts files
    std::fs::create_dir_all(pkg_dir.path().join("dist/types")).unwrap();
    std::fs::write(
        pkg_dir.path().join("dist/types/index.d.ts"),
        b"export declare function helper(): void;\n",
    )
    .unwrap();
    std::fs::write(
        pkg_dir.path().join("dist/types/utils.d.ts"),
        b"export declare function util(): string;\n",
    )
    .unwrap();
    std::fs::write(pkg_dir.path().join("index.js"), b"exports.helper = function() {};\n").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "main": "./index.js",
        "typesVersions": {
            ">=4.0": { "*": ["dist/types/*"] }
        }
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();
    let file_names: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        file_names.contains(&"index.d.ts".to_string()),
        "typesVersions wildcard should expand to include index.d.ts; got: {:?}",
        file_names
    );
    assert!(
        file_names.contains(&"utils.d.ts".to_string()),
        "typesVersions wildcard should expand to include utils.d.ts; got: {:?}",
        file_names
    );
}

// ─── GAP-16-12: browser field ────────────────────────────────────────────────

#[test]
fn gap_16_12_browser_field_contributes_entry_point() {
    let pkg_dir = tempfile::tempdir().unwrap();

    std::fs::write(pkg_dir.path().join("browser.js"), b"// browser entry point").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "browser": "./browser.js"
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();
    let file_names: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        file_names.contains(&"browser.js".to_string()),
        "browser field should contribute browser.js as entry point; got: {:?}",
        file_names
    );
}

#[test]
fn gap_16_12_browser_field_combined_with_main() {
    let pkg_dir = tempfile::tempdir().unwrap();

    std::fs::write(pkg_dir.path().join("index.js"), b"// main").unwrap();
    std::fs::write(pkg_dir.path().join("browser.js"), b"// browser").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "main": "./index.js",
        "browser": "./browser.js"
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();
    let file_names: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        file_names.contains(&"index.js".to_string()),
        "main field should still be present; got: {:?}",
        file_names
    );
    assert!(
        file_names.contains(&"browser.js".to_string()),
        "browser field should also be present; got: {:?}",
        file_names
    );
}
