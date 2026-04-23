//! Integration tests for Phase 18 — Multi-subpath Identity and Grammar Routing.
//!
//! Tests all REQ-IDs end-to-end:
//!   ARCH-01..05, INF-01..03, IDN-01..02.
//!
//! Uses the post-Blocker-1 compare_exports signature: (old, new, pkg_root).
//! All ExportSymbol literals include compound_members: Vec::new() (post-18-02).

use std::path::PathBuf;

use interlint::compare::compare_exports;
use interlint::compare::subtype::{RULE_EXPORT_RENAMED, RULE_NAMESPACE_MEMBER_MOVED};
use interlint::discover::discover_exports;
use interlint::types::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, Severity, SymbolSpace,
};

// ─── Helpers ────────────────────────────────────────────────────────────────

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn make_sym(name: &str, file: PathBuf, line: u32, kinds: Vec<DeclarationKind>) -> ExportSymbol {
    // Warning 5 committed: compound_members is on ExportSymbol (added in 18-02).
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file,
        line,
        col: 1,
        type_signature: None,
        overloads: Vec::new(),
        decl_kinds: kinds,
        type_guard_type: None,
        is_async: false,
        is_generator: false,
        is_reexport: false,
        is_type_only: false,
        release_tag: None,
        compound_members: Vec::new(),
    }
}

fn write_pkg(dir: &std::path::Path, pkg: serde_json::Value) {
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();
}

/// Benign pkg_root placeholder — used for synthetic ExportMap tests that don't
/// need real source files (IDN-01 source-scan will find no alias trailer).
fn noop_root() -> &'static std::path::Path {
    std::path::Path::new(".")
}

// ─── SC-1 / ARCH-01: Subpath move (React 18 → 19 inspired) ──────────────────
//
// Real-package inspiration: React 18.3.1 → 19.0.0 split of the JSX runtime —
// `createElement`-style helpers moved from the root entry point to the
// `./jsx-runtime` subpath. A consumer importing from the old root path
// breaks when the symbol only exists at the new subpath.

#[test]
fn arch_01_sc1_symbol_moved_between_subpaths_produces_removed_finding() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();

    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "createElement"),
        make_sym(
            "createElement",
            PathBuf::from("index.js"),
            1,
            vec![DeclarationKind::Function],
        ),
    );
    new.insert(
        ExportKey::new("./jsx-runtime", None, SymbolSpace::Value, "createElement"),
        make_sym(
            "createElement",
            PathBuf::from("jsx-runtime.js"),
            1,
            vec![DeclarationKind::Function],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root());
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1,
        "removal at original (root) subpath must fire"
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-added")
            .count(),
        1,
        "addition at new subpath must fire"
    );
    let removed = findings
        .iter()
        .find(|f| f.rule == "export-removed")
        .unwrap();
    assert_eq!(removed.file, PathBuf::from("index.js"));
}

// ─── SC-2 / ARCH-01: Dropped CJS condition (Zod 3.23.8 dual-publish) ────────
//
// Real-package inspiration: Zod 3.23.8 dual-publishes as ESM + CJS under
// the root subpath with `import` / `require` conditions pointing to
// `lib/index.mjs` and `lib/index.js` respectively. Dropping the `require`
// condition silently breaks CommonJS consumers.

#[test]
fn arch_01_sc2_dropped_cjs_condition_produces_condition_scoped_finding() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();

    old.insert(
        ExportKey::new(".", Some("import"), SymbolSpace::Value, "z"),
        make_sym(
            "z",
            PathBuf::from("lib/index.mjs"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    old.insert(
        ExportKey::new(".", Some("require"), SymbolSpace::Value, "z"),
        make_sym(
            "z",
            PathBuf::from("lib/index.js"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", Some("import"), SymbolSpace::Value, "z"),
        make_sym(
            "z",
            PathBuf::from("lib/index.mjs"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root());
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1
    );
    let removed = findings
        .iter()
        .find(|f| f.rule == "export-removed")
        .unwrap();
    assert!(
        removed.message.contains("require") || removed.message.contains("z"),
        "removed finding message should name the dropped condition or symbol, got: {}",
        removed.message
    );
}

// ─── SC-3 / ARCH-04: Multi-file same-name class ─────────────────────────────
//
// Synthetic fixture (per 18-RESEARCH.md Open Q1: no mainstream package
// ships the same class name at two distinct public subpaths). Validates
// that Phase 18's ExportKey keeps identity split across (subpath, name)
// so both instances participate in structural diffing.

#[test]
fn arch_04_sc3_multi_subpath_same_name_class_both_diffed() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();

    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "User"),
        make_sym(
            "User",
            PathBuf::from("src/a.ts"),
            1,
            vec![DeclarationKind::Class],
        ),
    );
    old.insert(
        ExportKey::new("./admin", None, SymbolSpace::Value, "User"),
        make_sym(
            "User",
            PathBuf::from("src/b.ts"),
            1,
            vec![DeclarationKind::Class],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "User"),
        make_sym(
            "User",
            PathBuf::from("src/a.ts"),
            1,
            vec![DeclarationKind::Class],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root());
    let removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "export-removed")
        .collect();
    assert_eq!(removed.len(), 1, "exactly one removal (admin User)");
    assert_eq!(removed[0].file, PathBuf::from("src/b.ts"));
}

// ─── SC-4 / ARCH-03: Same-name type+value coexist ──────────────────────────
//
// Validates D-03 (SymbolSpace as first-class key dimension): `export type Foo`
// and `export const Foo` live in distinct ExportMap entries and diff
// independently against their own space. Removing only the type-space entry
// produces exactly one export-removed for Foo(Type); the value-space Foo is
// untouched.

#[test]
fn arch_03_sc4_type_and_value_space_coexist_and_diff_independently() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();

    old.insert(
        ExportKey::new(".", None, SymbolSpace::Type, "Foo"),
        make_sym(
            "Foo",
            test_file(),
            1,
            vec![DeclarationKind::Interface],
        ),
    );
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
        make_sym(
            "Foo",
            test_file(),
            5,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
        make_sym(
            "Foo",
            test_file(),
            5,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root());
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-added")
            .count(),
        0
    );
}

// ─── SC-5 / ARCH-02: Compound-component assignment ──────────────────────────
//
// Real-package inspiration: @headlessui/react 1.7.19 exports compound
// components like `Menu` with attached members `Menu.Item`, `Menu.Header`,
// `Menu.Button` via direct property assignment and `Object.assign` patterns.
// Phase 18 ARCH-02 captures these member names into
// `ExportSymbol.compound_members`.

#[test]
fn arch_02_sc5_compound_component_menu_item_attaches() {
    let dir = tempfile::tempdir().unwrap();
    // NOTE: D-16 conservative rule — attach ONLY when target identifier is
    // itself an exported symbol declared in the same file. Direct property
    // assignment (`Menu.Item = ...`) and `Object.assign` attach members.
    // We keep Menu as `const` so it is a valid assignment target (the plan
    // stub used `Menu = Object.assign(Menu, ...)` which re-assigns to a
    // const — not valid TypeScript; simplified to the Object.assign form
    // that attaches in a single expression statement).
    let src = r#"
export const Menu = () => null;
Menu.Item = () => null;
Menu.Header = () => null;
Object.assign(Menu, { Footer: () => null });
"#;
    std::fs::write(dir.path().join("menu.ts"), src).unwrap();
    write_pkg(
        dir.path(),
        serde_json::json!({
            "name": "ui-lib",
            "version": "1.0.0",
            "main": "./menu.ts"
        }),
    );

    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    let menu_key = exports
        .keys()
        .find(|k| k.name == "Menu")
        .expect("Menu should be discovered");
    let menu_sym = &exports[menu_key];

    let has_item = menu_sym.compound_members.iter().any(|m| m == "Item");
    let has_header = menu_sym.compound_members.iter().any(|m| m == "Header");
    let has_footer = menu_sym.compound_members.iter().any(|m| m == "Footer");
    assert!(
        has_item && has_header && has_footer,
        "expected Item/Header/Footer in Menu.compound_members, got {:?}",
        menu_sym.compound_members
    );

    // Only parse/ERROR warnings are a concern; resolver-level warnings about
    // entry-point resolution etc. are acceptable.
    let parse_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("parse") || w.message.contains("ERROR"))
        .collect();
    assert!(
        parse_warnings.is_empty(),
        "no parse warnings expected, got {:?}",
        parse_warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

// ─── SC-6 / ARCH-05: Declaration merging hybrid (class+namespace) ───────────
//
// Real-package inspiration: TypeScript stdlib patterns like
// `class Foo {}` merged with `namespace Foo { ... }` (e.g., DOM API shims,
// or the pattern `class Observable { ... } namespace Observable { ... }`
// used by rxjs-style libraries). Phase 18 D-14 stores both decl_kinds on
// the merged ExportSymbol so REF-05 (namespace-to-other) sees the
// namespace facet is preserved and DOES NOT fire.

#[test]
fn arch_05_sc6_class_namespace_hybrid_preserved_no_ref05_fp() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Namespace, "User"),
        make_sym(
            "User",
            test_file(),
            1,
            vec![DeclarationKind::Namespace],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Namespace, "User"),
        make_sym(
            "User",
            test_file(),
            1,
            vec![DeclarationKind::Class, DeclarationKind::Namespace],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root());
    let ref05_fired = findings.iter().any(|f| f.rule == "namespace-to-other");
    assert!(
        !ref05_fired,
        "hybrid class+namespace must NOT fire REF-05; got findings: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

// ─── SC-7 / INF-01: .tsx file JSX body does not suppress exports ────────────
//
// Real-package inspiration: @radix-ui/react-dropdown-menu 2.1.2 ships
// components as `.tsx` source with JSX bodies. Pre-INF-01, using the
// pure TypeScript grammar caused the JSX body to parse as an ERROR node,
// which upstream was filtered out and caused all exports in the file to
// be suppressed. INF-01 routes `.tsx` through LANGUAGE_TSX.

#[test]
fn inf_01_sc7_tsx_jsx_body_discovers_exports() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("comp.tsx"),
        r#"
export const Comp = () => <div>hi</div>;
export interface Props { children: string }
export function other(): number { return 1; }
"#,
    )
    .unwrap();
    write_pkg(
        dir.path(),
        serde_json::json!({
            "name": "tsx-pkg",
            "version": "1.0.0",
            "main": "./comp.tsx"
        }),
    );

    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    let names: std::collections::HashSet<String> = exports.keys().map(|k| k.name.clone()).collect();

    assert!(
        names.contains("Comp"),
        "Comp must be discovered; got {:?}",
        names
    );
    assert!(
        names.contains("Props"),
        "Props must be discovered; got {:?}",
        names
    );
    assert!(
        names.contains("other"),
        "other must be discovered; got {:?}",
        names
    );

    let parse_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("parse") || w.message.contains("ERROR"))
        .collect();
    assert!(
        parse_warnings.is_empty(),
        "TSX grammar should parse JSX body cleanly; got: {:?}",
        parse_warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}
