//! Unknown runtime keys must not look like an empty, fully analyzed interface.
use interlint::ecosystems::node::discovery::discover_exports;
use interlint::languages::javascript::model::SymbolSpace;

fn package(source: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"name":"keys","exports":"./index.js"}"#,
    )
    .unwrap();
    std::fs::write(root.path().join("index.js"), source).unwrap();
    root
}

#[test]
fn runtime_computed_export_keys_are_incomplete() {
    for source in [
        "exports[process.env.KEY] = 1;",
        "let key='x'; key='y'; exports[key] = 1;",
        "const Symbol={for:()=>Math.random()}; exports[Symbol.for('x')]=1;",
        "import {Symbol} from './custom.js'; exports[Symbol.for('x')]=1;",
    ] {
        let root = package(source);
        let (_, warnings) = discover_exports(root.path()).unwrap();
        assert!(
            warnings.iter().any(|warning| warning.incomplete
                && warning.message.contains("computed CommonJS export key")),
            "{source}: {warnings:?}"
        );
    }
}

#[test]
fn symbol_properties_do_not_collide_with_similarly_spelled_string_keys() {
    let root = package("exports[Symbol.iterator]=1; exports['@@iterator']=2;");
    let (exports, warnings) = discover_exports(root.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports
        .keys()
        .any(|key| key.name == "@@iterator" && key.space == SymbolSpace::Symbol));
    assert!(exports
        .keys()
        .any(|key| key.name == "@@iterator" && key.space == SymbolSpace::Value));
}

#[test]
fn unknown_class_keys_are_incomplete_but_unrelated_writes_are_not() {
    for (statement, incomplete) in [
        ("this[process.env.KEY]=1", true),
        ("other[process.env.KEY]=1", false),
    ] {
        let old = package(&format!("export class Api {{ static {{ {statement}; }} }}"));
        let new = package("export class Api {}");
        let (before, _) = discover_exports(old.path()).unwrap();
        let (after, _) = discover_exports(new.path()).unwrap();
        let findings =
            interlint::languages::javascript::compare::structure::compare_structural_diff(
                old.path(),
                new.path(),
                &before,
                &after,
            );
        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            incomplete,
            "{findings:?}"
        );
    }
}

#[test]
fn local_export_objects_and_deferred_writes_are_not_module_symbols() {
    for source in [
        "const exports={}; exports[Symbol.iterator]=1;",
        "function later(){exports[Symbol.iterator]=1;}",
    ] {
        let root = package(source);
        let (exports, _) = discover_exports(root.path()).unwrap();
        assert!(
            !exports.keys().any(|key| key.space == SymbolSpace::Symbol),
            "{exports:?}"
        );
    }
}

#[test]
fn declared_unique_symbol_members_have_static_identity() {
    let root = package(
        "export declare const Key: unique symbol; export declare class Api { [Key]: string; }",
    );
    // The package helper uses a JS entry point; this case is a declaration input.
    std::fs::rename(root.path().join("index.js"), root.path().join("index.d.ts")).unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"name":"keys","exports":"./index.d.ts"}"#,
    )
    .unwrap();
    let (exports, warnings) = discover_exports(root.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let findings = interlint::languages::javascript::compare::structure::compare_structural_diff(
        root.path(),
        root.path(),
        &exports,
        &exports,
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"),
        "{findings:?}"
    );
}
