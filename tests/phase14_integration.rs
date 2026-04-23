//! Integration tests for Phase 14 — Structural Diff Coverage for Non-Declaration Exports.
//!
//! These tests verify end-to-end behavior of all Phase 14 requirements:
//!   REQ-14-01: Arrow function and function expression handling in structural diff
//!   REQ-14-02: Anonymous default export handling (default function, default class)
//!   REQ-14-03: Namespace member tracking as dotted names (Foo.bar)
//!   REQ-14-04: Declaration merging for interfaces (collect from all declarations)
//!   REQ-14-05: Interface member structural diff (member-removed, member-added, param changes)
//!
//! All tests exercise the public API: parse_exports, compare_exports, compare_structural_diff.

use interlint::compare::compare_exports;
use interlint::compare::compare_structural_diff;
use interlint::discover::tree_query::{parse_exports, SourceLanguage};
use interlint::types::{DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, Severity};
use std::path::PathBuf;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn make_export_sym(
    name: &str,
    file: PathBuf,
    line: u32,
    decl_kind: Option<DeclarationKind>,
) -> ExportSymbol {
    ExportSymbol {
        name: name.to_string(),
        kind: ExportKind::Named,
        file,
        line,
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
        compound_members: Vec::new(),
    }
}

fn make_default_export_sym(
    file: PathBuf,
    line: u32,
    decl_kind: Option<DeclarationKind>,
) -> ExportSymbol {
    ExportSymbol {
        name: "default".to_string(),
        kind: ExportKind::Default,
        file,
        line,
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
        compound_members: Vec::new(),
    }
}

// ─── REQ-14-01: Arrow function and function expression structural diff ────────

#[test]
fn test_arrow_function_identified_as_function() {
    // REQ-14-01: Arrow function exports should be identified as functions by extract_declaration,
    // enabling async/generator and overload-based structural diff.
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    // Old version is async arrow, new version is non-async
    // async-to-sync is detected because is_async is set from tree-sitter (no LSP needed)
    let old_source = b"export const add = async (x: number): Promise<number> => x;\n";
    let new_source = b"export const add = (x: number): number => x;\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("add"),
        make_export_sym(
            "add",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Variable),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("add"),
        make_export_sym(
            "add",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Variable),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    // async-to-sync is detectable without LSP because is_async is set from tree-sitter extraction
    assert!(
        findings.iter().any(|f| f.rule == "async-to-sync"),
        "arrow function async-to-sync change should be detected; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_arrow_function_async_to_sync() {
    // Old: export const fetch = async (url: string): Promise<string> => '';
    // New: export const fetch = (url: string): string => '';
    // Expected: async-to-sync behavioral finding
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export const fetch = async (url: string): Promise<string> => '';\n";
    let new_source = b"export const fetch = (url: string): string => '';\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("fetch"),
        make_export_sym(
            "fetch",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Variable),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("fetch"),
        make_export_sym(
            "fetch",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Variable),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    // async-to-sync is a behavioral change (Warning or Major severity)
    assert!(
        findings.iter().any(|f| f.rule == "async-to-sync"),
        "changing async arrow to sync should produce async-to-sync finding; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_function_expression_identified_as_function() {
    // REQ-14-01: function expression exports should be identified as functions.
    // async-to-sync is detectable without LSP (from tree-sitter is_async extraction).
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source =
        b"export const handler = async function(req: string): Promise<string> { return req; };\n";
    let new_source = b"export const handler = function(req: string): string { return req; };\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("handler"),
        make_export_sym(
            "handler",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Variable),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("handler"),
        make_export_sym(
            "handler",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Variable),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings.iter().any(|f| f.rule == "async-to-sync"),
        "function expression async-to-sync change should be detected; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_arrow_function_detected_in_parse_exports() {
    // REQ-14-01: parse_exports should produce entries for arrow function exports
    let source = b"export const add = (x: number, y: number): number => x + y;\n";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    assert!(
        exports.iter().any(|e| e.name == "add"),
        "arrow function export 'add' should appear; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

// ─── REQ-14-02: Anonymous default export structural diff ─────────────────────

#[test]
fn test_anonymous_default_function_async_to_sync() {
    // REQ-14-02: anonymous default function structural diff — async-to-sync is detectable
    // without LSP because is_async is set from tree-sitter extraction.
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export default async function(x: number): Promise<string> { return ''; }\n";
    let new_source = b"export default function(x: number): string { return ''; }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings.iter().any(|f| f.rule == "async-to-sync"),
        "async anonymous default function becoming sync should produce async-to-sync; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn test_anonymous_default_class_method_removed() {
    // Old: export default class { foo(): void {} }
    // New: export default class { }
    // Expected: class-method-removed finding
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export default class { foo(): void {} }\n";
    let new_source = b"export default class { }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(PathBuf::from("index.ts"), 1, Some(DeclarationKind::Class)),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(PathBuf::from("index.ts"), 1, Some(DeclarationKind::Class)),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-method-removed" && f.severity == Severity::Major),
        "removing foo() from anonymous default class should produce class-method-removed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

// ─── REQ-14-03: Namespace member tracking ────────────────────────────────────

#[test]
fn test_namespace_member_removed() {
    // Old: export namespace Foo { export function bar(): void {} }
    // New: export namespace Foo { }
    // Expected: export-removed for "Foo.bar"
    let old_source = b"export namespace Foo { export function bar(): void {} }\n";
    let new_source = b"export namespace Foo { }\n";

    let old_exports_raw = parse_exports(old_source, SourceLanguage::TypeScript).unwrap();
    let new_exports_raw = parse_exports(new_source, SourceLanguage::TypeScript).unwrap();

    let mut old_exports = ExportMap::new();
    for raw in &old_exports_raw {
        old_exports.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: raw.decl_kind.clone().map(|k| vec![k]).unwrap_or_default(),
                type_guard_type: None,
                is_async: false,
                is_generator: false,
                is_reexport: false,
                is_type_only: false,
                release_tag: None,
                compound_members: Vec::new(),
            },
        );
    }

    let mut new_exports = ExportMap::new();
    for raw in &new_exports_raw {
        new_exports.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: raw.decl_kind.clone().map(|k| vec![k]).unwrap_or_default(),
                type_guard_type: None,
                is_async: false,
                is_generator: false,
                is_reexport: false,
                is_type_only: false,
                release_tag: None,
                compound_members: Vec::new(),
            },
        );
    }

    // Verify that parse_exports produced "Foo.bar" for old source
    assert!(
        old_exports.contains_key(&ExportKey::new_root_value("Foo.bar")),
        "old exports should contain 'Foo.bar'; got: {:?}",
        old_exports.keys().collect::<Vec<_>>()
    );

    let findings = compare_exports(&old_exports, &new_exports, std::path::Path::new("."));

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-removed" && f.message.contains("Foo.bar")),
        "removing namespace member Foo.bar should produce export-removed finding; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_namespace_member_added() {
    // Old: export namespace Foo { }
    // New: export namespace Foo { export function bar(): void {} }
    // Expected: export-added for "Foo.bar"
    let old_source = b"export namespace Foo { }\n";
    let new_source = b"export namespace Foo { export function bar(): void {} }\n";

    let old_exports_raw = parse_exports(old_source, SourceLanguage::TypeScript).unwrap();
    let new_exports_raw = parse_exports(new_source, SourceLanguage::TypeScript).unwrap();

    let mut old_exports = ExportMap::new();
    for raw in &old_exports_raw {
        old_exports.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: raw.decl_kind.clone().map(|k| vec![k]).unwrap_or_default(),
                type_guard_type: None,
                is_async: false,
                is_generator: false,
                is_reexport: false,
                is_type_only: false,
                release_tag: None,
                compound_members: Vec::new(),
            },
        );
    }

    let mut new_exports = ExportMap::new();
    for raw in &new_exports_raw {
        new_exports.insert(
            ExportKey::new_root_value(&raw.name),
            ExportSymbol {
                name: raw.name.clone(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.ts"),
                line: raw.line,
                col: raw.col,
                type_signature: None,
                overloads: Vec::new(),
                decl_kinds: raw.decl_kind.clone().map(|k| vec![k]).unwrap_or_default(),
                type_guard_type: None,
                is_async: false,
                is_generator: false,
                is_reexport: false,
                is_type_only: false,
                release_tag: None,
                compound_members: Vec::new(),
            },
        );
    }

    // Verify parse_exports produced "Foo.bar" for new source
    assert!(
        new_exports.contains_key(&ExportKey::new_root_value("Foo.bar")),
        "new exports should contain 'Foo.bar'; got: {:?}",
        new_exports.keys().collect::<Vec<_>>()
    );

    let findings = compare_exports(&old_exports, &new_exports, std::path::Path::new("."));

    assert!(
        findings
            .iter()
            .any(|f| f.rule == "export-added" && f.message.contains("Foo.bar")),
        "adding namespace member Foo.bar should produce export-added finding; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_namespace_itself_still_tracked() {
    // REQ-14-03: The namespace itself (e.g. "Foo") should still appear, not just members
    let source = b"export namespace Foo { export function bar(): void {} }\n";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    assert!(
        exports.iter().any(|e| e.name == "Foo"),
        "namespace 'Foo' itself should still be in exports; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        exports.iter().any(|e| e.name == "Foo.bar"),
        "namespace member 'Foo.bar' should also be in exports; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

// ─── REQ-14-04: Declaration merging for interfaces ────────────────────────────

#[test]
fn test_declaration_merging_interface_member_removed() {
    // Old: two interface Foo declarations with different members
    // New: only one — effectively dropping b()
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export interface Foo { a(): void; }\nexport interface Foo { b(): void; }\n";
    let new_source = b"export interface Foo { a(): void; }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("Foo"),
        make_export_sym(
            "Foo",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("Foo"),
        make_export_sym(
            "Foo",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    // Removing b() from the merged interface should be detected
    assert!(
        findings.iter().any(|f| {
            f.rule == "interface-member-removed" && f.message.contains('b')
        }),
        "removing 'b' from merged interface declarations should produce interface-member-removed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn test_declaration_merging_collects_all_members() {
    // REQ-14-04: Two interface declarations union their members
    // Old: a() + b() (from two declarations)
    // New: a() only
    // The diff should know old had both a() and b()
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source =
        b"export interface Config { debug: boolean; }\nexport interface Config { verbose: boolean; }\n";
    let new_source = b"export interface Config { debug: boolean; }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("Config"),
        make_export_sym(
            "Config",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("Config"),
        make_export_sym(
            "Config",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    // verbose was in the second declaration — should be detected as removed
    assert!(
        findings.iter().any(|f| {
            f.rule == "interface-member-removed" && f.message.contains("verbose")
        }),
        "removing 'verbose' from merged interface should produce interface-member-removed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

// ─── REQ-14-05: Interface member structural diff ──────────────────────────────

#[test]
fn test_interface_member_removed_finding() {
    // Old: export interface Config { debug: boolean; verbose: boolean; }
    // New: export interface Config { debug: boolean; }
    // Expected: interface-member-removed Major finding for "verbose"
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export interface Config { debug: boolean; verbose: boolean; }\n";
    let new_source = b"export interface Config { debug: boolean; }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("Config"),
        make_export_sym(
            "Config",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("Config"),
        make_export_sym(
            "Config",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings.iter().any(|f| {
            f.rule == "interface-member-removed"
                && f.severity == Severity::Major
                && f.message.contains("verbose")
        }),
        "removing 'verbose' from interface should produce interface-member-removed Major; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.severity, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn test_interface_member_added_required() {
    // Old: export interface Config { debug: boolean; }
    // New: export interface Config { debug: boolean; port: number; }
    // Expected: interface-member-added Major finding for "port"
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export interface Config { debug: boolean; }\n";
    let new_source = b"export interface Config { debug: boolean; port: number; }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("Config"),
        make_export_sym(
            "Config",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("Config"),
        make_export_sym(
            "Config",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings
            .iter()
            .any(|f| { f.rule == "interface-member-added" && f.message.contains("port") }),
        "adding 'port' to interface should produce interface-member-added finding; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_interface_method_param_count_changed() {
    // Old: export interface Handler { process(x: string): void; }
    // New: export interface Handler { process(x: string, y: number): void; }
    // Expected: interface-param-count-changed Major finding
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export interface Handler { process(x: string): void; }\n";
    let new_source = b"export interface Handler { process(x: string, y: number): void; }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("Handler"),
        make_export_sym(
            "Handler",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("Handler"),
        make_export_sym(
            "Handler",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings.iter().any(|f| {
            f.rule == "interface-param-count-changed"
                && f.message.contains("process")
        }),
        "adding param to interface method should produce interface-param-count-changed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn test_interface_member_removed_is_major() {
    // REQ-14-05: interface-member-removed must be Major severity
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"export interface Api { send(): void; receive(): string; }\n";
    let new_source = b"export interface Api { send(): void; }\n";

    std::fs::write(old_dir.path().join("index.ts"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.ts"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("Api"),
        make_export_sym(
            "Api",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("Api"),
        make_export_sym(
            "Api",
            PathBuf::from("index.ts"),
            1,
            Some(DeclarationKind::Interface),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    let removed_finding = findings
        .iter()
        .find(|f| f.rule == "interface-member-removed" && f.message.contains("receive"));
    assert!(
        removed_finding.is_some(),
        "should find interface-member-removed for 'receive'; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        removed_finding.unwrap().severity,
        Severity::Major,
        "interface-member-removed should be Major"
    );
}
