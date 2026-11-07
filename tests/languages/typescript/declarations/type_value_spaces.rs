//! Type value spaces compatibility checks.

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare::compare_exports;
use pledge::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, SymbolSpace,
};
use pledge::languages::javascript::syntax::exports::parse_exports;
use pledge::languages::javascript::syntax::grammar::SourceLanguage;
use pledge::languages::typescript::types::shape::{PrimitiveKind, TypeShape};
use std::path::PathBuf;

fn test_file() -> PathBuf {
    PathBuf::from("src/index.ts")
}

fn make_sym(name: &str, file: PathBuf, line: u32, kinds: Vec<DeclarationKind>) -> ExportSymbol {
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
        is_proxy_dispatched: false,
        local_name: None,
    }
}

/// Benign pkg_root placeholder — used for synthetic ExportMap tests that don't
/// need real source files (IDN-01 source-scan will find no alias trailer).
fn noop_root() -> &'static std::path::Path {
    std::path::Path::new(".")
}

#[test]
fn type_and_value_space_coexist_and_diff_independently() {
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

#[test]
fn direct_type_aliases_and_values_keep_separate_declaration_identities() {
    for source in [
        "export declare const Brand: unique symbol;\nexport type Brand = string;",
        "export type Brand = string;\nexport declare const Brand: unique symbol;",
        "export declare const Brand: unique symbol;\nexport type/*alias*/Brand = string;",
    ] {
        let raw = parse_exports(source.as_bytes(), SourceLanguage::TypeScript).unwrap();
        assert_eq!(raw.len(), 2, "{source}: {raw:?}");
        assert!(raw
            .iter()
            .any(|export| export.is_type_only
                && export.decl_kind == Some(DeclarationKind::TypeAlias)));
        assert!(raw
            .iter()
            .any(|export| !export.is_type_only
                && export.decl_kind == Some(DeclarationKind::Variable)));

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("package.json"), r#"{"exports":"./index.d.ts"}"#).unwrap();
        std::fs::write(root.join("index.d.ts"), source).unwrap();
        let (before, warnings) = discover_exports(root).unwrap();
        assert!(warnings.is_empty(), "{source}: {warnings:?}");
        let type_key = ExportKey::new(".", None, SymbolSpace::Type, "Brand");
        let value_key = ExportKey::new_root_value("Brand");
        assert!(before.contains_key(&type_key), "{source}: {before:?}");
        assert!(before.contains_key(&value_key), "{source}: {before:?}");
        assert_ne!(before[&type_key].line, before[&value_key].line);
        assert_eq!(
            before[&type_key].overloads,
            vec![TypeShape::Primitive(PrimitiveKind::String)]
        );
        assert_eq!(
            before[&value_key].overloads,
            vec![TypeShape::Opaque("unique symbol".into())]
        );

        std::fs::write(
            root.join("index.d.ts"),
            "export type Brand = string;\nexport declare const Brand: unique symbol;",
        )
        .unwrap();
        let (reordered, warnings) = discover_exports(root).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let reordered_findings = compare_exports(&before, &reordered, root, true);
        assert!(
            reordered_findings.is_empty(),
            "{source}: {reordered_findings:?}"
        );

        std::fs::write(
            root.join("index.d.ts"),
            "export declare const Brand: unique symbol;",
        )
        .unwrap();
        let (after, warnings) = discover_exports(root).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let findings = compare_exports(&before, &after, root, true);
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.rule == "export-removed")
                .count(),
            1,
            "{source}: {findings:?}"
        );
        assert!(after.contains_key(&value_key));
        assert_eq!(after[&value_key].overloads, before[&value_key].overloads);
    }
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn compiler_evidence_keeps_type_alias_and_unique_symbol_consumers_separate() {
    use pledge::languages::typescript::compiler::{self, EnrichResult};

    let mut packages = Vec::new();
    let mut exports = Vec::new();
    for (source, type_available) in [
        (
            "export declare const Brand: unique symbol; export type Brand = string;",
            true,
        ),
        (
            "export type Brand = string; export declare const Brand: unique symbol;",
            true,
        ),
        ("export declare const Brand: unique symbol;", false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(
            root.join("package.json"),
            r#"{"type":"module","exports":"./index.d.ts"}"#,
        )
        .unwrap();
        std::fs::write(root.join("index.d.ts"), source).unwrap();
        for (consumer, succeeds) in [
            ("const value:typeof Brand = Brand;", true),
            ("const text:Brand = 'text';", type_available),
        ] {
            std::fs::write(
                root.join("consumer.ts"),
                format!("import {{Brand}} from './index.js'; {consumer}"),
            )
            .unwrap();
            assert_typescript_compiles(root, succeeds, &format!("{source} / {consumer}"));
        }
        let (mut discovered, warnings) = discover_exports(root).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(matches!(
            compiler::enrich_exports(&mut discovered, root)
                .await
                .unwrap(),
            EnrichResult::Enriched
        ));
        let value_key = ExportKey::new_root_value("Brand");
        assert!(matches!(
            discovered[&value_key].overloads.as_slice(),
            [TypeShape::Generic { name, args }]
                if name == "<nominal>"
                    && matches!(args.last(), Some(TypeShape::Primitive(PrimitiveKind::Symbol)))
        ));
        if type_available {
            let type_key = ExportKey::new(".", None, SymbolSpace::Type, "Brand");
            assert_eq!(
                discovered[&type_key].overloads,
                vec![TypeShape::Primitive(PrimitiveKind::String)]
            );
        }
        exports.push(discovered);
        packages.push(directory);
    }
    assert!(compare_exports(&exports[0], &exports[1], packages[1].path(), true).is_empty());
    let removed = compare_exports(&exports[0], &exports[2], packages[2].path(), true);
    assert_eq!(removed.len(), 1, "{removed:?}");
    assert_eq!(removed[0].rule, "export-removed");
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn imported_declarations_keep_their_contracts_through_local_exports() {
    use pledge::core::findings::has_breaking_changes;
    use pledge::languages::javascript::compare::structure::compare_structural_diff;
    use pledge::languages::typescript::compiler::{self, EnrichResult};

    for (name, before_entry, after_entry, declaration, breaking) in [
        (
            "named imported interface changes",
            "import {Model} from './model.js'; export {Model as Api};",
            "import {Model} from './model.js'; export {Model as Api};",
            DeclarationKind::Interface,
            true,
        ),
        (
            "explicitly type-only exported interface changes",
            "import {Model} from './model.js'; export type {Model as Api};",
            "import {Model} from './model.js'; export type {Model as Api};",
            DeclarationKind::Interface,
            true,
        ),
        (
            "type-only imported interface changes",
            "import type {Model} from './model.js'; export {Model as Api};",
            "import type {Model} from './model.js'; export {Model as Api};",
            DeclarationKind::Interface,
            true,
        ),
        (
            "direct interface forwarding becomes local forwarding",
            "export {Model as Api} from './model.js';",
            "import {Model} from './model.js'; export {Model as Api};",
            DeclarationKind::Interface,
            false,
        ),
        (
            "local interface forwarding changes declaration order",
            "import {Model} from './model.js'; export {Model as Api};",
            "export {Model as Api}; import {Model} from './model.js';",
            DeclarationKind::Interface,
            false,
        ),
        (
            "default imported class changes",
            "import Model from './model.js'; export {Model as Api};",
            "import Model from './model.js'; export {Model as Api};",
            DeclarationKind::Class,
            true,
        ),
        (
            "direct default forwarding becomes local forwarding",
            "export {default as Api} from './model.js';",
            "import Model from './model.js'; export {Model as Api};",
            DeclarationKind::Class,
            false,
        ),
    ] {
        let (model, consumer, space) = match declaration {
            DeclarationKind::Interface => (
                "export interface Model",
                "const result:Api = {value:'text'};",
                SymbolSpace::Type,
            ),
            DeclarationKind::Class => (
                "export default class Model",
                "const text:string = new Api().value;",
                SymbolSpace::Value,
            ),
            _ => unreachable!(),
        };
        let key = ExportKey::new(".", None, space, "Api");
        let mut packages = Vec::new();
        let mut source_exports = Vec::new();
        let mut enriched_exports = Vec::new();
        for (entry, changed) in [(before_entry, false), (after_entry, breaking)] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            let field_type = if changed { "number" } else { "string" };
            let expected = if changed {
                PrimitiveKind::Number
            } else {
                PrimitiveKind::String
            };
            std::fs::write(
                root.join("package.json"),
                r#"{"type":"module","exports":"./index.d.ts"}"#,
            )
            .unwrap();
            std::fs::write(root.join("index.d.ts"), entry).unwrap();
            std::fs::write(
                root.join("model.d.ts"),
                format!("{model} {{ value: {field_type} }}"),
            )
            .unwrap();
            std::fs::write(
                root.join("consumer.ts"),
                format!("import {{Api}} from './index.js'; {consumer}"),
            )
            .unwrap();
            assert_typescript_compiles(root, !changed, name);

            let (mut discovered, warnings) = discover_exports(root).unwrap();
            assert!(warnings.is_empty(), "{name}: {warnings:?}");
            let assert_field = |exports: &ExportMap| {
                assert_eq!(exports.len(), 1, "{name}: {exports:?}");
                let symbol = exports
                    .get(&key)
                    .unwrap_or_else(|| panic!("{name}: {exports:?}"));
                assert!(
                    symbol.decl_kinds.contains(&declaration),
                    "{name}: {symbol:?}"
                );
                let fields = match symbol.overloads.as_slice() {
                    [TypeShape::Object(fields)] => fields,
                    [TypeShape::Intersection(members)] => members
                        .iter()
                        .find_map(|member| match member {
                            TypeShape::Object(fields) => Some(fields),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{name}: {symbol:?}")),
                    _ => panic!("{name}: missing declaration shape: {symbol:?}"),
                };
                assert_eq!(
                    fields["value"].shape,
                    TypeShape::Primitive(expected.clone())
                );
            };
            assert_field(&discovered);
            source_exports.push(discovered.clone());
            assert!(matches!(
                compiler::enrich_exports(&mut discovered, root)
                    .await
                    .unwrap(),
                EnrichResult::Enriched
            ));
            assert_field(&discovered);
            enriched_exports.push(discovered);
            packages.push(directory);
        }
        for exports in [source_exports, enriched_exports] {
            let mut findings = compare_exports(&exports[0], &exports[1], packages[1].path(), false);
            findings.extend(compare_structural_diff(
                packages[0].path(),
                packages[1].path(),
                &exports[0],
                &exports[1],
            ));
            assert_eq!(
                has_breaking_changes(&findings),
                breaking,
                "{name}: {findings:?}"
            );
            assert!(
                !findings
                    .iter()
                    .any(|finding| finding.rule == "analysis-incomplete"),
                "{name}: {findings:?}"
            );
        }
    }
    assert_type_only_forwarding_facets().await;
}

#[cfg(feature = "lsp-tests")]
fn assert_typescript_compiles(root: &std::path::Path, succeeds: bool, context: &str) {
    let output = std::process::Command::new("tsc")
        .current_dir(root)
        .args([
            "--noEmit",
            "--strict",
            "--skipLibCheck",
            "--target",
            "ES2022",
            "--module",
            "NodeNext",
            "--moduleResolution",
            "NodeNext",
            "consumer.ts",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        succeeds,
        "{context}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(feature = "lsp-tests")]
async fn assert_type_only_forwarding_facets() {
    use pledge::core::findings::has_breaking_changes;
    use pledge::languages::javascript::compare::structure::compare_structural_diff;
    use pledge::languages::typescript::compiler::{self, EnrichResult};

    for (name, new_declarations, alias_type, value_type, breaking) in [
        (
            "type-only value changes",
            "export declare const Brand:string; export type Brand=string;",
            Some(PrimitiveKind::String),
            PrimitiveKind::String,
            true,
        ),
        (
            "type-only alias changes",
            "export declare const Brand:number; export type Brand=number;",
            Some(PrimitiveKind::Number),
            PrimitiveKind::Number,
            true,
        ),
        (
            "type-only facets reorder",
            "export type Brand=string; export declare const Brand:number;",
            Some(PrimitiveKind::String),
            PrimitiveKind::Number,
            false,
        ),
        (
            "sole type-only value remains referencable",
            "export declare const Brand:number;",
            None,
            PrimitiveKind::Number,
            false,
        ),
    ] {
        for chained in [false, true] {
            let mut packages = Vec::new();
            let mut source_exports = Vec::new();
            let mut enriched_exports = Vec::new();
            for new in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let root = directory.path();
                let declarations = if new {
                    new_declarations
                } else if alias_type.is_some() {
                    "export declare const Brand:number; export type Brand=string;"
                } else {
                    "export declare const Brand:number;"
                };
                std::fs::write(
                    root.join("package.json"),
                    r#"{"type":"module","exports":"./index.d.ts"}"#,
                )
                .unwrap();
                std::fs::write(root.join("model.d.ts"), declarations).unwrap();
                std::fs::write(
                    root.join("bridge.d.ts"),
                    "export type {Brand as Reference} from './model.js';",
                )
                .unwrap();
                let entry = match (chained, new) {
                    (false, false) => "export type {Brand as Reference} from './model.js';",
                    (false, true) => {
                        "import {Brand} from './model.js'; export type {Brand as Reference};"
                    }
                    (true, false) => "export type {Reference} from './bridge.js';",
                    (true, true) => {
                        "import type {Reference} from './bridge.js'; export {Reference};"
                    }
                };
                std::fs::write(root.join("index.d.ts"), entry).unwrap();
                let alias_consumer = if alias_type.is_some() {
                    "const alias:Reference='ok';"
                } else {
                    ""
                };
                std::fs::write(root.join("consumer.ts"), format!("import type {{Reference}} from './index.js'; const value:typeof Reference=1; {alias_consumer}")).unwrap();
                assert_typescript_compiles(root, !new || !breaking, name);
                std::fs::write(
                    root.join("consumer.ts"),
                    "import {Reference} from './index.js'; const forbidden=Reference;",
                )
                .unwrap();
                assert_typescript_compiles(root, false, "type-only exports forbid runtime access");

                let (mut discovered, warnings) = discover_exports(root).unwrap();
                assert!(warnings.is_empty(), "{name}: {warnings:?}");
                let assert_facets = |exports: &ExportMap| {
                    assert_eq!(
                        exports.len(),
                        if alias_type.is_some() { 2 } else { 1 },
                        "{name}: {exports:?}"
                    );
                    if let Some(alias) = &alias_type {
                        let key = ExportKey::new(".", None, SymbolSpace::Type, "Reference");
                        let symbol = &exports[&key];
                        assert!(symbol.is_type_only);
                        assert_eq!(symbol.decl_kinds, vec![DeclarationKind::TypeAlias]);
                        let expected = if new {
                            alias.clone()
                        } else {
                            PrimitiveKind::String
                        };
                        assert_eq!(
                            symbol.overloads,
                            vec![TypeShape::Primitive(expected)],
                            "{name}: {symbol:?}"
                        );
                    }
                    let space = if alias_type.is_some() {
                        SymbolSpace::Value
                    } else {
                        SymbolSpace::Type
                    };
                    let key = ExportKey::new(".", None, space, "Reference");
                    let symbol = &exports[&key];
                    assert!(symbol.is_type_only);
                    assert_eq!(symbol.decl_kinds, vec![DeclarationKind::Variable]);
                    let expected = if new {
                        value_type.clone()
                    } else {
                        PrimitiveKind::Number
                    };
                    assert_eq!(
                        symbol.overloads,
                        vec![TypeShape::Primitive(expected)],
                        "{name}: {symbol:?}"
                    );
                };
                assert_facets(&discovered);
                source_exports.push(discovered.clone());
                assert!(matches!(
                    compiler::enrich_exports(&mut discovered, root)
                        .await
                        .unwrap(),
                    EnrichResult::Enriched
                ));
                assert_facets(&discovered);
                enriched_exports.push(discovered);
                packages.push(directory);
            }
            for exports in [source_exports, enriched_exports] {
                let mut findings =
                    compare_exports(&exports[0], &exports[1], packages[1].path(), false);
                findings.extend(compare_structural_diff(
                    packages[0].path(),
                    packages[1].path(),
                    &exports[0],
                    &exports[1],
                ));
                assert_eq!(
                    has_breaking_changes(&findings),
                    breaking,
                    "{name}: {findings:?}"
                );
                assert!(
                    !findings.iter().any(|finding| matches!(
                        finding.rule,
                        "analysis-incomplete" | "declaration-kind-changed"
                    )),
                    "{name}: {findings:?}"
                );
            }
        }
    }
}
