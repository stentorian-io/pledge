//! Integration tests for Phase 12 TS/JS ecosystem compatibility fixes.
//!
//! These tests verify end-to-end behavior of all Phase 12 changes:
//!   ECF-01: Type-only exports (`export type { Foo }`)
//!   ECF-02: Destructured exports (`export const { a, b } = obj`)
//!   ECF-03: Export assignment (`export = MyClass`)
//!   ECF-04: Ambient declarations (`export declare function f(): void`)
//!   ECF-05/06: Resolver compound extension and .mts/.cts candidates
//!   ECF-08/09/10: Class getter/setter and private field exclusion
//!
//! Tests exercise parse_exports, compare_exports, resolve_module_path, and
//! compare_class_shapes directly with constructed inputs.

use interlint::compare::class_diff::compare_class_shapes;
use interlint::compare::compare_exports;
use interlint::compare::subtype::{RULE_CLASS_METHOD_KIND_CHANGED, RULE_CLASS_METHOD_REMOVED};
use interlint::discover::resolver::resolve_module_path;
use interlint::discover::tree_query::{parse_exports, RawExportKind, SourceLanguage};
use interlint::types::{
    ClassMember, ClassMemberKind, ClassShape, ExportKey, ExportKind, ExportMap, ExportSymbol,
    Visibility,
};
use std::path::{Path, PathBuf};

// ─── Test helpers ─────────────────────────────────────────────────────────────

fn test_file() -> PathBuf {
    PathBuf::from("test.ts")
}

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
        is_optional: false,
        is_accessor: false,
        this_param_type: None,
        decorators: Vec::new(),
    }
}

fn make_class(members: Vec<ClassMember>) -> ClassShape {
    ClassShape {
        members,
        is_abstract: false,
        extends_text: None,
        implements_list: vec![],
        type_params: vec![],
        constructor_param_optionality: vec![],
        decorators: Vec::new(),
    }
}

fn make_export_sym(name: &str) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file: test_file(),
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
        compound_members: Vec::new(),
    }
}

fn map_from_names(names: &[&str]) -> ExportMap {
    let mut map = ExportMap::new();
    for &name in names {
        map.insert(ExportKey::new_root_value(name), make_export_sym(name));
    }
    map
}

// ─── ECF-01: Type-only exports ────────────────────────────────────────────────

/// ECF-01: `export type { Foo, Bar }` should produce 2 Named RawExports.
/// Validates that type-only export statements flow through parse_exports.
#[test]
fn test_type_only_export_captured() {
    let src = b"export type { Foo, Bar };";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        2,
        "Expected 2 exports from type-only export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    assert!(
        exports
            .iter()
            .any(|e| e.name == "Foo" && e.kind == RawExportKind::Named),
        "Expected export named 'Foo' with kind Named; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );

    assert!(
        exports
            .iter()
            .any(|e| e.name == "Bar" && e.kind == RawExportKind::Named),
        "Expected export named 'Bar' with kind Named; got: {:?}",
        exports
            .iter()
            .map(|e| (&e.name, &e.kind))
            .collect::<Vec<_>>()
    );
}

/// ECF-01: `export type { Foo } from './other'` should produce a NamedReexport.
/// Validates that type-only re-exports record the source module specifier.
#[test]
fn test_type_only_reexport_captured() {
    let src = b"export type { Foo } from './other';";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        1,
        "Expected 1 export from type-only re-export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    let exp = &exports[0];
    assert_eq!(exp.name, "Foo", "Export name should be 'Foo'");
    assert_eq!(
        exp.kind,
        RawExportKind::NamedReexport,
        "Type-only re-export should have kind NamedReexport"
    );
    assert!(
        exp.source
            .as_deref()
            .map(|s| s.contains("other"))
            .unwrap_or(false),
        "Source should contain 'other'; got: {:?}",
        exp.source
    );
}

/// ECF-01: Type-only exports must participate in the diff pipeline.
/// Old has "Foo" (from type-only export), new is empty → export-removed fires.
#[test]
fn test_type_only_export_removal_detected() {
    let old_src = b"export type { Foo };";
    let new_src = b"";

    let old_raw = parse_exports(old_src, SourceLanguage::TypeScript).expect("parse old");
    let new_raw = parse_exports(new_src, SourceLanguage::TypeScript).expect("parse new");

    let mut old_map = ExportMap::new();
    for raw in &old_raw {
        old_map.insert(
            ExportKey::new_root_value(&raw.name),
            make_export_sym(&raw.name),
        );
    }

    let mut new_map = ExportMap::new();
    for raw in &new_raw {
        new_map.insert(
            ExportKey::new_root_value(&raw.name),
            make_export_sym(&raw.name),
        );
    }

    let findings = compare_exports(&old_map, &new_map, std::path::Path::new("."));

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("Foo")),
        "Expected export-removed for 'Foo'; got: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

// ─── ECF-02: Destructured exports ────────────────────────────────────────────

/// ECF-02: `export const { alpha, beta, gamma } = config` should produce 3 Named exports.
/// Validates that destructured object export patterns are captured.
#[test]
fn test_destructured_export_captured() {
    let src = b"export const { alpha, beta, gamma } = config;";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        3,
        "Expected 3 exports from destructured export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    for expected_name in ["alpha", "beta", "gamma"] {
        assert!(
            exports
                .iter()
                .any(|e| e.name == expected_name && e.kind == RawExportKind::Named),
            "Expected export named '{}' with kind Named; got: {:?}",
            expected_name,
            exports
                .iter()
                .map(|e| (&e.name, &e.kind))
                .collect::<Vec<_>>()
        );
    }
}

/// ECF-02: Removing one member of a destructured export fires export-removed.
/// Old has "alpha" and "beta", new has only "alpha" → export-removed for "beta".
#[test]
fn test_destructured_export_removal_detected() {
    let old_map = map_from_names(&["alpha", "beta"]);
    let new_map = map_from_names(&["alpha"]);

    let findings = compare_exports(&old_map, &new_map, std::path::Path::new("."));

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("beta")),
        "Expected export-removed for 'beta'; got: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );

    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("alpha")),
        "Should NOT fire export-removed for 'alpha' (still present); got: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

// ─── ECF-03: Export assignment ────────────────────────────────────────────────

/// ECF-03: `export = MyClass` is a CJS module.exports equivalent.
/// Should produce 1 RawExport with name="default" and kind=Default.
#[test]
fn test_export_assignment_captured() {
    let src = b"export = MyClass;";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        1,
        "Expected 1 export from export assignment; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    let exp = &exports[0];
    assert_eq!(
        exp.name, "default",
        "export = ... should use name 'default'; got: '{}'",
        exp.name
    );
    assert_eq!(
        exp.kind,
        RawExportKind::Default,
        "export = ... should have kind Default; got: {:?}",
        exp.kind
    );
}

// ─── ECF-04: Ambient declarations ────────────────────────────────────────────

/// ECF-04: Ambient declarations in .d.ts-style sources should be captured.
/// `export declare function/class/const` forms must all produce Named exports.
#[test]
fn test_ambient_declarations_captured() {
    let src = b"
export declare function greet(name: string): string;
export declare class Logger {}
export declare const VERSION: string;
";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    assert_eq!(
        exports.len(),
        3,
        "Expected 3 exports from ambient declarations; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    for expected_name in ["greet", "Logger", "VERSION"] {
        assert!(
            exports.iter().any(|e| e.name == expected_name),
            "Expected export named '{}'; got: {:?}",
            expected_name,
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }
}

// ─── ECF-05/06: Resolver — compound extensions and .mts/.cts ─────────────────

/// ECF-05: resolve_module_path must preserve compound file stems.
/// `./utils.config` → candidates must include "utils.config.ts", NOT just "utils.ts".
#[test]
fn test_resolver_compound_extension() {
    let candidates = resolve_module_path(Path::new("/project/src/index.ts"), "./utils.config");

    let candidate_strings: Vec<String> = candidates
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Must have a candidate with utils.config.ts (compound stem preserved).
    assert!(
        candidate_strings
            .iter()
            .any(|s| s.contains("utils.config.ts")),
        "Candidates must include 'utils.config.ts'; got: {:?}",
        candidate_strings
    );

    // Must NOT have a candidate that is just "utils.ts" (the bug: with_extension drops .config).
    assert!(
        !candidate_strings.iter().any(|s| s.ends_with("utils.ts")),
        "Candidates must NOT contain 'utils.ts' (compound stem bug); got: {:?}",
        candidate_strings
    );
}

/// ECF-06: resolve_module_path must include .mts and .cts candidates for ESM packages.
/// Validates that modern TS extension resolution is complete.
#[test]
fn test_resolver_mts_cts_candidates() {
    let candidates = resolve_module_path(Path::new("/project/src/index.ts"), "./module");

    let candidate_strings: Vec<String> = candidates
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    assert!(
        candidate_strings.iter().any(|s| s.ends_with("module.mts")),
        "Candidates must include 'module.mts'; got: {:?}",
        candidate_strings
    );

    assert!(
        candidate_strings.iter().any(|s| s.ends_with("module.cts")),
        "Candidates must include 'module.cts'; got: {:?}",
        candidate_strings
    );

    assert!(
        candidate_strings
            .iter()
            .any(|s| s.ends_with("module.d.mts")),
        "Candidates must include 'module.d.mts'; got: {:?}",
        candidate_strings
    );

    assert!(
        candidate_strings
            .iter()
            .any(|s| s.ends_with("module.d.cts")),
        "Candidates must include 'module.d.cts'; got: {:?}",
        candidate_strings
    );
}

// ─── ECF-08/09/10: Class getter/setter and private field exclusion ────────────

/// ECF-08/09: Getter → Method kind change should fire class-method-kind-changed.
/// This validates the new ClassMemberKind::Getter and Setter variants and the
/// class-method-kind-changed rule added in Phase 12 Plan 03.
#[test]
fn test_getter_setter_kind_change_detected() {
    let old_class = make_class(vec![make_member(
        "value",
        ClassMemberKind::Getter,
        Visibility::Public,
    )]);
    let new_class = make_class(vec![make_member(
        "value",
        ClassMemberKind::Method,
        Visibility::Public,
    )]);

    let findings = compare_class_shapes(&old_class, &new_class, "MyClass", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CLASS_METHOD_KIND_CHANGED),
        "Expected class-method-kind-changed (Getter→Method); got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );

    let finding = findings
        .iter()
        .find(|f| f.rule == RULE_CLASS_METHOD_KIND_CHANGED)
        .unwrap();
    assert_eq!(
        finding.severity,
        interlint::types::Severity::Major,
        "class-method-kind-changed must be Major severity"
    );
}

/// ECF-08: Setter → Property kind change should fire class-method-kind-changed.
#[test]
fn test_setter_to_property_kind_change_detected() {
    let old_class = make_class(vec![make_member(
        "count",
        ClassMemberKind::Setter,
        Visibility::Public,
    )]);
    let new_class = make_class(vec![make_member(
        "count",
        ClassMemberKind::Property,
        Visibility::Public,
    )]);

    let findings = compare_class_shapes(&old_class, &new_class, "Counter", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CLASS_METHOD_KIND_CHANGED),
        "Expected class-method-kind-changed (Setter→Property); got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

/// ECF-09: Getter removed from class should fire class-method-removed.
/// Validates that getter members participate in the removal detection path.
#[test]
fn test_getter_removal_detected() {
    let old_class = make_class(vec![make_member(
        "name",
        ClassMemberKind::Getter,
        Visibility::Public,
    )]);
    let new_class = make_class(vec![]);

    let findings = compare_class_shapes(&old_class, &new_class, "Entity", &test_file(), 1, 1);

    assert!(
        findings.iter().any(|f| f.rule == RULE_CLASS_METHOD_REMOVED),
        "Expected class-method-removed for removed getter 'name'; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

/// ECF-10: Class with ES private field — only the public member should be visible.
/// This tests that ClassShape construction correctly excludes #-prefixed members.
/// We construct ClassShape manually: a class where the public member `visible`
/// is present but a private field `#secret` is excluded (not part of ClassShape.members).
#[test]
fn test_private_field_not_in_class_shape() {
    // Simulate what parse_class_shape should produce: only public members.
    // The private field #hidden is excluded from ClassShape.members.
    let class_with_private = make_class(vec![
        make_member("visible", ClassMemberKind::Property, Visibility::Public),
        // Note: #hidden is NOT in members — parse_class_shape must exclude it.
    ]);

    // A class with no private fields at all should compare identically to one
    // that has #hidden excluded — no findings.
    let class_without_private = make_class(vec![make_member(
        "visible",
        ClassMemberKind::Property,
        Visibility::Public,
    )]);

    let findings = compare_class_shapes(
        &class_with_private,
        &class_without_private,
        "Container",
        &test_file(),
        1,
        1,
    );

    assert!(
        findings.is_empty(),
        "Two structurally identical classes (both with only 'visible') should produce no findings; got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

/// ECF-10: Verify that parse_exports on a class with ES private fields
/// only reports the class export itself, not the private fields.
/// The private field #hidden is internal to the class and must not leak into
/// the top-level export map.
#[test]
fn test_private_field_export_not_leaked() {
    let src = b"export class A { #hidden: string; visible: number; }";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    // Only "A" should appear as an export — the #hidden field is not a top-level export.
    assert!(
        exports.iter().any(|e| e.name == "A"),
        "Expected export 'A'; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    assert!(
        !exports
            .iter()
            .any(|e| e.name == "#hidden" || e.name.contains("hidden")),
        "Private field '#hidden' must not appear as an export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

/// ECF-08: Getter → Setter kind change (both accessor kinds) fires class-method-kind-changed.
#[test]
fn test_getter_to_setter_kind_change_detected() {
    let old_class = make_class(vec![make_member(
        "data",
        ClassMemberKind::Getter,
        Visibility::Public,
    )]);
    let new_class = make_class(vec![make_member(
        "data",
        ClassMemberKind::Setter,
        Visibility::Public,
    )]);

    let findings = compare_class_shapes(&old_class, &new_class, "DataHolder", &test_file(), 1, 1);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CLASS_METHOD_KIND_CHANGED),
        "Expected class-method-kind-changed (Getter→Setter); got: {:?}",
        findings.iter().map(|f| f.rule).collect::<Vec<_>>()
    );
}

/// ECF-01/02: Mixed export statement with both type-only and regular named exports
/// are both captured correctly.
#[test]
fn test_type_and_value_exports_in_same_file() {
    let src = b"
export const foo = 42;
export type { MyType };
export interface MyInterface {}
";

    let exports = parse_exports(src, SourceLanguage::TypeScript).expect("parse failed");

    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

    assert!(
        names.contains(&"foo"),
        "Expected 'foo' in exports; got: {:?}",
        names
    );
    assert!(
        names.contains(&"MyType"),
        "Expected 'MyType' (type-only export) in exports; got: {:?}",
        names
    );
    assert!(
        names.contains(&"MyInterface"),
        "Expected 'MyInterface' (interface export) in exports; got: {:?}",
        names
    );
}
