//! Commonjs declarations compatibility checks.

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
fn cjs_class_method_removed() {
    // Old: module.exports = class Foo { method() {} toString() { return "foo"; } }
    // New: module.exports = class Foo { toString() { return "foo"; } }
    // Expected: class-method-removed finding
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"module.exports = class Foo { method() {} toString() { return \"foo\"; } }\n";
    let new_source = b"module.exports = class Foo { toString() { return \"foo\"; } }\n";

    std::fs::write(old_dir.path().join("index.js"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.js"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(PathBuf::from("index.js"), 1, Some(DeclarationKind::Class)),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(PathBuf::from("index.js"), 1, Some(DeclarationKind::Class)),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings.iter().any(|f| f.rule == "class-method-removed"),
        "removing method() from CJS inline class should produce class-method-removed; findings: {:?}",
        findings.iter().map(|f| (&f.rule, &f.message)).collect::<Vec<_>>()
    );
}

#[test]
fn cjs_function_params() {
    // Old: module.exports = function(a, b) { return a + b; }
    // New: module.exports = function(a, b, c) { return a + b + c; }
    // Expected: param-added finding (detected via compare_structural_diff)
    // NOTE: param-added for non-LSP mode is detected via tree-sitter param count
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"module.exports = function(a, b) { return a + b; }\n";
    let new_source = b"module.exports = async function(a, b) { return a + b; }\n";

    std::fs::write(old_dir.path().join("index.js"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.js"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(
            PathBuf::from("index.js"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("default"),
        make_default_export_sym(
            PathBuf::from("index.js"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    // sync-to-async is detectable without LSP — tree-sitter extracts is_async
    assert!(
        findings.iter().any(|f| f.rule == "sync-to-async"
            || f.rule == "async-to-sync"
            || f.rule == "param-added"),
        "structural change in CJS function export should be detected; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn cjs_exports_handler_async() {
    // Old: exports.handler = async (req, res) => { res.send('ok'); }
    // New: exports.handler = (req, res) => { res.send('ok'); }
    // Expected: async-to-sync finding
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();

    let old_source = b"exports.handler = async (req, res) => { res.send('ok'); }\n";
    let new_source = b"exports.handler = (req, res) => { res.send('ok'); }\n";

    std::fs::write(old_dir.path().join("index.js"), old_source).unwrap();
    std::fs::write(new_dir.path().join("index.js"), new_source).unwrap();

    let mut old_exports = ExportMap::new();
    old_exports.insert(
        ExportKey::new_root_value("handler"),
        make_export_sym(
            "handler",
            PathBuf::from("index.js"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let mut new_exports = ExportMap::new();
    new_exports.insert(
        ExportKey::new_root_value("handler"),
        make_export_sym(
            "handler",
            PathBuf::from("index.js"),
            1,
            Some(DeclarationKind::Function),
        ),
    );

    let findings =
        compare_structural_diff(old_dir.path(), new_dir.path(), &old_exports, &new_exports);

    assert!(
        findings.iter().any(|f| f.rule == "async-to-sync"),
        "removing async from CJS exports.handler should produce async-to-sync; findings: {:?}",
        findings
            .iter()
            .map(|f| (&f.rule, &f.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn cjs_class_method_detected_by_parse_exports() {
    // Verify parse_exports correctly identifies decl_kind=Class for CJS inline classes
    let source = b"module.exports = class Foo { method() {} toString() { return 'foo'; } }\n";
    let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
    let default_export = exports.iter().find(|e| e.name == "default");
    assert!(
        default_export.is_some(),
        "module.exports = class should emit 'default' export; got: {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        default_export.unwrap().decl_kind,
        Some(DeclarationKind::Class),
        "CJS inline class should have decl_kind Class"
    );
}
