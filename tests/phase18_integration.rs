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
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
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

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1,
        "removal at original (root) subpath must fire"
    );
    assert_eq!(
        findings.iter().filter(|f| f.rule == "export-added").count(),
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

    let findings = compare_exports(&old, &new, noop_root(), true);
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

    let findings = compare_exports(&old, &new, noop_root(), true);
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
        make_sym("Foo", test_file(), 1, vec![DeclarationKind::Interface]),
    );
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
        make_sym("Foo", test_file(), 5, vec![DeclarationKind::Variable]),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
        make_sym("Foo", test_file(), 5, vec![DeclarationKind::Variable]),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1
    );
    assert_eq!(
        findings.iter().filter(|f| f.rule == "export-added").count(),
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
        make_sym("User", test_file(), 1, vec![DeclarationKind::Namespace]),
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

    let findings = compare_exports(&old, &new, noop_root(), true);
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

// ─── SC-8 / INF-02: rollup-plugin-dts split declaration ─────────────────────
//
// Real-package inspiration: @radix-ui/react-dropdown-menu 2.1.2 ships a
// rollup-plugin-dts bundled declaration where ambient `declare const X`
// and the `export { X }` specifier live in separate statements. Phase
// 18 INF-02's two-pass flow builds a file-scope ambient sidecar in pass
// 1 and enriches the export's type_signature via the link-pass.

#[test]
fn inf_02_sc8_rollup_plugin_dts_split_decl_populates_type_signature() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("index.d.ts"),
        r#"
declare const DropdownMenu: number;
declare const Root: string;
export { DropdownMenu, Root };
"#,
    )
    .unwrap();
    write_pkg(
        dir.path(),
        serde_json::json!({
            "name": "split-dts",
            "version": "1.0.0",
            "types": "./index.d.ts"
        }),
    );

    let (exports, _) = discover_exports(dir.path()).unwrap();
    let dd_key = exports
        .keys()
        .find(|k| k.name == "DropdownMenu")
        .expect("DropdownMenu should be discovered");
    let dd_sym = &exports[dd_key];
    assert!(
        dd_sym.type_signature.is_some(),
        "type_signature must be populated from declare site per INF-02; got {:?}",
        dd_sym.type_signature
    );
    assert!(
        dd_sym.type_signature.as_ref().unwrap().contains("number"),
        "type_signature should contain 'number'; got {:?}",
        dd_sym.type_signature
    );
    assert!(dd_sym.decl_kinds.contains(&DeclarationKind::Variable));

    let root_key = exports.keys().find(|k| k.name == "Root").unwrap();
    let root_sym = &exports[root_key];
    assert!(
        root_sym
            .type_signature
            .as_ref()
            .map(|s| s.contains("string"))
            .unwrap_or(false),
        "Root type_signature should contain 'string'; got {:?}",
        root_sym.type_signature
    );
}

// ─── SC-9 / INF-03: tsup `foo as default` trailer ──────────────────────────
//
// Real-package inspiration: tsup / esbuild ESM output commonly trails
// with `export { foo as default, foo, Bar }` so consumers can do both
// default and named imports. Phase 18 INF-03 flips the emitted
// RawExportKind to Default when the alias_name is literally "default".

#[test]
fn inf_03_sc9_tsup_default_trailer_captured() {
    let dir = tempfile::tempdir().unwrap();
    // NOTE: We use `.mts` (ESM TypeScript) rather than `.mjs`. The
    // tree-sitter-javascript 0.25.0 grammar drops the `as default`
    // alias clause from the parse tree entirely (only one
    // `export_specifier name: (identifier)` is captured without the
    // `alias:` field) — a grammar gap that prevents INF-03 detection
    // from firing on raw JavaScript sources. The TypeScript grammar
    // handles `as default` correctly. Real-world tsup/esbuild output
    // also commonly ships `.d.ts` / `.mts` alongside `.mjs` where the
    // type layer carries the alias information. See SUMMARY.md for
    // the deferred follow-up item tracking the JS-grammar gap.
    std::fs::write(
        dir.path().join("index.mts"),
        r#"
const z = 1;
const ZodError = 2;
export { z as default, z, ZodError };
"#,
    )
    .unwrap();
    write_pkg(
        dir.path(),
        serde_json::json!({
            "name": "zod-shape",
            "version": "1.0.0",
            "main": "./index.mts"
        }),
    );

    let (exports, _) = discover_exports(dir.path()).unwrap();
    // ExportKind doesn't implement Hash; use a Vec for presence checks.
    let pairs: Vec<(String, ExportKind)> = exports
        .values()
        .map(|s| (s.name.clone(), s.kind.clone()))
        .collect();

    assert!(
        pairs
            .iter()
            .any(|(n, k)| n == "default" && matches!(k, ExportKind::Default)),
        "Default export should be captured from `z as default` trailer; got {:?}",
        pairs
    );
    assert!(
        pairs
            .iter()
            .any(|(n, k)| n == "z" && matches!(k, ExportKind::Named)),
        "Named export `z` should be present; got {:?}",
        pairs
    );
    assert!(
        pairs
            .iter()
            .any(|(n, k)| n == "ZodError" && matches!(k, ExportKind::Named)),
        "Named export `ZodError` should be present; got {:?}",
        pairs
    );
}

// ─── SC-10 / IDN-01: Alias-trailer rename ───────────────────────────────────
//
// Real-package inspiration: libraries commonly rename an export while
// preserving back-compat via `export { NewName as OldName, NewName }` —
// the alias-trailer pattern. Phase 18 IDN-01 collapses the add/remove
// pair into a single Minor `export-renamed` finding, strictly guarded
// by the D-05 two-condition rule (aliased specifier AND plain specifier
// both present in the new source).

#[test]
fn idn_01_sc10_alias_trailer_rename_produces_minor_renamed_finding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("old.ts"), "export const OldApi = 1;\n").unwrap();
    std::fs::write(
        dir.path().join("new.ts"),
        r#"
const NewApi = 1;
export { NewApi as OldApi, NewApi };
"#,
    )
    .unwrap();

    let mut old_map = ExportMap::new();
    let mut new_map = ExportMap::new();
    old_map.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "OldApi"),
        make_sym(
            "OldApi",
            PathBuf::from("old.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    new_map.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "NewApi"),
        make_sym(
            "NewApi",
            PathBuf::from("new.ts"),
            3,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old_map, &new_map, dir.path(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_EXPORT_RENAMED)
            .count(),
        1,
        "exactly one export-renamed expected; got {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        0,
        "IDN-01 should suppress the base export-removed"
    );
    assert_eq!(
        findings.iter().filter(|f| f.rule == "export-added").count(),
        0,
        "IDN-01 should suppress the base export-added"
    );
    let renamed = findings
        .iter()
        .find(|f| f.rule == RULE_EXPORT_RENAMED)
        .unwrap();
    assert!(matches!(renamed.severity, Severity::Minor));
    assert!(
        renamed.message.contains("OldApi") && renamed.message.contains("NewApi"),
        "rename message must mention both names; got: {}",
        renamed.message
    );
}

#[test]
fn idn_01_sc10_no_alias_trailer_stays_as_major_removed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("new.ts"), "export const NewApi = 1;\n").unwrap();

    let mut old_map = ExportMap::new();
    let mut new_map = ExportMap::new();
    old_map.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "OldApi"),
        make_sym(
            "OldApi",
            PathBuf::from("old.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    new_map.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "NewApi"),
        make_sym(
            "NewApi",
            PathBuf::from("new.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old_map, &new_map, dir.path(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_EXPORT_RENAMED)
            .count(),
        0,
        "without alias trailer, IDN-01 must NOT coalesce"
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        1,
        "plain removal stays as Major export-removed"
    );
}

// ─── SC-11 / IDN-02: Namespace-move cascade (direct insertion) ──────────────
//
// Real-package inspiration: tRPC 10.45.0 / deprecation-style refactors where
// a namespace like `A.x / A.y` is relocated to `D.x / D.y`. Phase 18 IDN-02
// exact-subtree depth-1 consolidation collapses the add/remove cascade into
// one Major namespace-member-moved per member.

#[test]
fn idn_02_sc11_namespace_move_collapses_into_member_moved_findings() {
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "A.x"),
        make_sym(
            "A.x",
            PathBuf::from("ns.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "A.y"),
        make_sym(
            "A.y",
            PathBuf::from("ns.ts"),
            2,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "D.x"),
        make_sym(
            "D.x",
            PathBuf::from("ns.ts"),
            10,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "D.y"),
        make_sym(
            "D.y",
            PathBuf::from("ns.ts"),
            11,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
            .count(),
        2,
        "IDN-02 should emit 2 namespace-member-moved; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "export-removed")
            .count(),
        0,
        "IDN-02 should suppress the base export-removed"
    );
    assert_eq!(
        findings.iter().filter(|f| f.rule == "export-added").count(),
        0,
        "IDN-02 should suppress the base export-added"
    );
    for f in findings
        .iter()
        .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
    {
        assert!(matches!(f.severity, Severity::Major));
        assert!(
            f.message.contains("A.") && f.message.contains("D."),
            "ns-move message must mention both paths; got {}",
            f.message
        );
    }
}

#[test]
fn idn_02_sc11_partial_overlap_does_not_collapse() {
    // D-09 exact-subtree match — partial overlap must NOT collapse.
    let mut old = ExportMap::new();
    let mut new = ExportMap::new();
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "A.x"),
        make_sym(
            "A.x",
            PathBuf::from("ns.ts"),
            1,
            vec![DeclarationKind::Variable],
        ),
    );
    old.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "A.y"),
        make_sym(
            "A.y",
            PathBuf::from("ns.ts"),
            2,
            vec![DeclarationKind::Variable],
        ),
    );
    new.insert(
        ExportKey::new(".", None, SymbolSpace::Value, "D.x"),
        make_sym(
            "D.x",
            PathBuf::from("ns.ts"),
            10,
            vec![DeclarationKind::Variable],
        ),
    );

    let findings = compare_exports(&old, &new, noop_root(), true);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
            .count(),
        0,
        "partial overlap must NOT collapse; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

// ─── Warning 6: SC-11 through FULL discover→compare pipeline ────────────────
//
// Warning 6 fix: exercise the SAME namespace-move scenario via the full
// discover_exports + compare_exports pipeline, NOT just direct ExportKey
// insertion. This validates that Phase 14's namespace-member emission
// (producing dotted-name ExportKeys like `A.x`) composes with Phase 18
// IDN-02 consolidation end-to-end. If Phase 14 ever regresses dotted-
// name emission or the resolver breaks the Value/Namespace space assignment
// for namespace members, this test fires immediately.

#[test]
fn idn_02_sc11_warning6_through_full_pipeline() {
    let old_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        old_dir.path().join("ns.ts"),
        r#"
export namespace A {
  export const x = 1;
  export const y = 2;
}
"#,
    )
    .unwrap();
    write_pkg(
        old_dir.path(),
        serde_json::json!({
            "name": "ns-pkg", "version": "1.0.0", "main": "./ns.ts"
        }),
    );

    let new_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        new_dir.path().join("ns.ts"),
        r#"
export namespace D {
  export const x = 1;
  export const y = 2;
}
"#,
    )
    .unwrap();
    write_pkg(
        new_dir.path(),
        serde_json::json!({
            "name": "ns-pkg", "version": "1.0.0", "main": "./ns.ts"
        }),
    );

    let (old_exports, _) = discover_exports(old_dir.path()).unwrap();
    let (new_exports, _) = discover_exports(new_dir.path()).unwrap();

    // Sanity: Phase 14 produced dotted-name ExportKeys.
    assert!(
        old_exports.keys().any(|k| k.name == "A.x"),
        "Phase 14 should emit dotted A.x; got keys: {:?}",
        old_exports.keys().map(|k| &k.name).collect::<Vec<_>>()
    );
    assert!(
        new_exports.keys().any(|k| k.name == "D.x"),
        "Phase 14 should emit dotted D.x; got keys: {:?}",
        new_exports.keys().map(|k| &k.name).collect::<Vec<_>>()
    );

    let findings = compare_exports(&old_exports, &new_exports, new_dir.path(), true);
    // Through the full pipeline, IDN-02 should consolidate into exactly 2
    // namespace-member-moved findings. Note: the namespace declarations
    // themselves (A/D in SymbolSpace::Namespace) remain as plain add/remove
    // outside the scope of IDN-02's depth-1 member consolidation.
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == RULE_NAMESPACE_MEMBER_MOVED)
            .count(),
        2,
        "Warning 6: pipeline test must yield 2 namespace-member-moved findings via discover→compare; got: {:?}",
        findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

// ─── Blocker 3: api-extractor $N suffix composes with ExportKey ─────────────
//
// Blocker 3 fix: Phase 17's `$N` rollup-suffix handling must interact
// cleanly with Phase 18's ExportKey keying. When api-extractor's TSDoc
// rollup emits a disambiguation-suffixed local name like `Foo$1` and
// re-exports it via `export { Foo$1 as Foo }`, the public-facing
// ExportKey name must be the base `Foo` (derived from the alias),
// keyed under its declared subpath (`./types`). Identical old/new
// shape produces no surface findings.

#[test]
fn blocker3_api_extractor_n_suffix_composes_with_subpath_keying() {
    let old_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(old_dir.path().join("dist")).unwrap();
    std::fs::write(
        old_dir.path().join("dist/types.d.ts"),
        // api-extractor rollup shape: internal `Foo$1` binding + re-export
        // via alias to base name `Foo`. Only the alias_name is exported;
        // `Foo$1` is NOT a public key.
        r#"
interface Foo$1 {
  a: string;
}
export { Foo$1 as Foo };
"#,
    )
    .unwrap();
    write_pkg(
        old_dir.path(),
        serde_json::json!({
            "name": "api-extractor-pkg",
            "version": "1.0.0",
            "exports": {
                "./types": "./dist/types.d.ts"
            }
        }),
    );

    let new_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(new_dir.path().join("dist")).unwrap();
    std::fs::write(
        new_dir.path().join("dist/types.d.ts"),
        r#"
interface Foo$1 {
  a: string;
}
export { Foo$1 as Foo };
"#,
    )
    .unwrap();
    write_pkg(
        new_dir.path(),
        serde_json::json!({
            "name": "api-extractor-pkg",
            "version": "1.0.0",
            "exports": {
                "./types": "./dist/types.d.ts"
            }
        }),
    );

    let (old_exports, _) = discover_exports(old_dir.path()).unwrap();
    let (new_exports, _) = discover_exports(new_dir.path()).unwrap();

    // Assertion 1: the public symbol is keyed under base name "Foo"
    //   (the alias_name), NOT "Foo$1" (the internal local binding).
    let foo_key = old_exports
        .keys()
        .find(|k| k.name == "Foo")
        .unwrap_or_else(|| {
            panic!(
                "expected ExportKey with name='Foo' (derived from `as Foo` alias); got keys: {:?}",
                old_exports.keys().map(|k| &k.name).collect::<Vec<_>>()
            )
        });
    // Assertion 2: the subpath identity is preserved — Phase 18 keying placed
    //   the symbol at "./types" (NOT at root "."), composing with $N handling.
    assert_eq!(
        foo_key.subpath, "./types",
        "Phase 18 ExportKey must carry the './types' subpath; got {:?}",
        foo_key
    );
    // Assertion 3: no residual "Foo$1" public key — the internal binding
    //   is NOT exported.
    assert!(
        !old_exports.keys().any(|k| k.name == "Foo$1"),
        "internal `Foo$1` binding must NOT appear as a public ExportKey; got keys: {:?}",
        old_exports.keys().map(|k| &k.name).collect::<Vec<_>>()
    );

    // Assertion 4: identical old/new shape → no surface add/remove/renamed.
    let findings = compare_exports(&old_exports, &new_exports, new_dir.path(), true);
    let surface_findings: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == "export-added" || f.rule == "export-removed" || f.rule == RULE_EXPORT_RENAMED
        })
        .collect();
    assert!(
        surface_findings.is_empty(),
        "identical $1-suffixed api-extractor shape should produce no surface findings; got: {:?}",
        surface_findings
            .iter()
            .map(|f| (f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

// ─── SC-12: umbrella self-check ─────────────────────────────────────────────
//
// Validates (via source-text inspection) that every required SC + regression
// fixture function is present in this file. If a future refactor renames or
// accidentally deletes a fixture, sc_12 fires before the individual test
// coverage is silently lost.

#[test]
fn sc_12_all_success_criteria_have_fixtures() {
    let src = include_str!("phase18_integration.rs");
    let required_markers = [
        "arch_01_sc1_",
        "arch_01_sc2_",
        "arch_04_sc3_",
        "arch_03_sc4_",
        "arch_02_sc5_",
        "arch_05_sc6_",
        "inf_01_sc7_",
        "inf_02_sc8_",
        "inf_03_sc9_",
        "idn_01_sc10_",
        "idn_02_sc11_",
        // Blocker 3 and Warning 6 regressions:
        "blocker3_api_extractor_n_suffix",
        "idn_02_sc11_warning6_through_full_pipeline",
    ];
    for marker in required_markers {
        assert!(
            src.contains(marker),
            "Phase 18 integration file missing fixture for {}",
            marker
        );
    }
}
