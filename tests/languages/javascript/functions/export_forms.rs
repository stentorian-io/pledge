//! Export forms compatibility checks.

use pledge::languages::javascript::compare::structure::compare_structural_diff;
use pledge::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol,
};
use pledge::languages::javascript::syntax::exports::parse_exports;
use pledge::languages::javascript::syntax::grammar::SourceLanguage;
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
        satisfies_type: None,
        compound_members: Vec::new(),
        typeof_import_refs: Vec::new(),
        is_proxy_dispatched: false,
        local_name: None,
    }
}

#[test]
fn test_arrow_function_identified_as_function() {
    // Arrow function exports should be identified as functions by extract_declaration,
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
    // function expression exports should be identified as functions.
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
    // parse_exports should produce entries for arrow function exports
    let source = b"export const add = (x: number, y: number): number => x + y;\n";
    let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();

    assert!(
        exports.iter().any(|e| e.name == "add"),
        "arrow function export 'add' should appear; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_anonymous_default_function_async_to_sync() {
    // anonymous default function structural diff — async-to-sync is detectable
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
fn replacing_a_function_expression_with_an_arrow_removes_construction() {
    use pledge::ecosystems::node::discovery::discover_exports;
    use pledge::languages::javascript::compare::structure::compare_structural_diff;

    for (replacement, breaking) in [
        ("()=>{}", true),
        ("async function(){}", true),
        ("function*(){}", true),
        ("function renamed(){}", false),
    ] {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        for (root, expression) in [(&old, "function(){}"), (&new, replacement)] {
            std::fs::write(
                root.path().join("package.json"),
                r#"{"name":"construction","type":"module","exports":"./index.js"}"#,
            )
            .unwrap();
            std::fs::write(
                root.path().join("index.js"),
                format!("export const Api={expression};"),
            )
            .unwrap();
        }
        let (before, _) = discover_exports(old.path()).unwrap();
        let (after, _) = discover_exports(new.path()).unwrap();
        let findings = compare_structural_diff(old.path(), new.path(), &before, &after);
        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.rule == "function-constructor-removed"),
            breaking,
            "{replacement}: {findings:?}"
        );
    }
}
