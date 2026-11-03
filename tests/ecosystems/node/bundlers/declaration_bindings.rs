//! Declaration bindings compatibility checks.

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::compare::rules::RULE_EXPORT_RENAMED;
use pledge::languages::javascript::model::{DeclarationKind, ExportKind};

fn write_pkg(dir: &std::path::Path, pkg: serde_json::Value) {
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();
}

#[test]
fn rollup_plugin_dts_split_decl_populates_type_signature() {
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

#[test]
fn tsup_default_trailer_captured() {
    let dir = tempfile::tempdir().unwrap();
    // Declaration bundlers commonly retain the default alias in `.mts`
    // output alongside a named export of the same binding.
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

#[test]
fn api_extractor_n_suffix_composes_with_subpath_keying() {
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
    // Assertion 2: the subpath identity is preserved —  keying placed
    // the symbol at "./types" (NOT at root "."), composing with $N handling.
    assert_eq!(
        foo_key.subpath, "./types",
        " ExportKey must carry the './types' subpath; got {:?}",
        foo_key
    );
    // Assertion 3: no residual "Foo$1" public key — the internal binding
    // is NOT exported.
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
