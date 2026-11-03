//! Interfaces compatibility checks.

use pledge::core::findings::Severity;
use pledge::languages::javascript::compare::structure::compare_structural_diff;
use pledge::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol,
};
use std::path::PathBuf;

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
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    }
}

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
    // Two interface declarations union their members
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
    // interface-member-removed must be Major severity
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
