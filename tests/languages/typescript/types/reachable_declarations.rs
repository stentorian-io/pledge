use interlint::compare::{compare_exports, compare_structural_diff, has_breaking_changes};
use interlint::discover::discover_exports;

fn package(source: &str, dependency: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"exports":"./index.ts"}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("index.ts"), source).unwrap();
    std::fs::write(dir.path().join("types.ts"), dependency).unwrap();
    dir
}

fn compare(
    old: &str,
    new: &str,
    old_dependency: &str,
    new_dependency: &str,
) -> Vec<interlint::types::Finding> {
    let old = package(old, old_dependency);
    let new = package(new, new_dependency);
    let (old_exports, old_warnings) = discover_exports(old.path()).unwrap();
    let (new_exports, new_warnings) = discover_exports(new.path()).unwrap();
    assert!(
        old_warnings.is_empty() && new_warnings.is_empty(),
        "{old_warnings:?} {new_warnings:?}"
    );
    let mut findings = compare_exports(&old_exports, &new_exports, new.path(), false);
    findings.extend(compare_structural_diff(
        old.path(),
        new.path(),
        &old_exports,
        &new_exports,
    ));
    findings
}

#[test]
fn private_parameter_and_return_types_are_part_of_the_public_contract() {
    for (old, new) in [
        ("interface Options { x: string } export function api(x: Options): void {}", "interface Options { x: string; y: number } export function api(x: Options): void {}"),
        ("interface Result { x: string } export declare function api(): Result;", "interface Result { x: number } export declare function api(): Result;"),
        ("type Options = { x: string }; export declare function api(x: Options): void;", "type Options = { x: string; y: number }; export declare function api(x: Options): void;"),
    ] {
        let findings = compare(old, new, "", "");
        assert!(has_breaking_changes(&findings), "{old} -> {new}: {findings:?}");
        assert!(findings.iter().any(|f| f.message.contains("api")));
    }
}

#[test]
fn inherited_and_merged_members_are_compared() {
    for (old, new) in [
        ("interface Base { x: string } export interface Public extends Base {}", "interface Base { x: number } export interface Public extends Base {}"),
        ("class Base { method(): void {} } export class Public extends Base {}", "class Base {} export class Public extends Base {}"),
        ("interface Options { x: string } interface Options { y?: number } export declare function api(x: Options): void;", "interface Options { x: string } interface Options { y: number } export declare function api(x: Options): void;"),
    ] {
        let findings = compare(old, new, "", "");
        assert!(has_breaking_changes(&findings), "{old} -> {new}: {findings:?}");
    }
}

#[test]
fn imported_aliases_and_generic_arguments_are_resolved() {
    for source in [
        "import type { Options as Local } from './types.js'; export declare function api(x: Local): void;",
        "import type * as Types from './types.js'; export declare function api(x: Types.Options): void;",
        "import type { Options } from './types.js'; type Local<T> = { input: T }; export declare function api(x: Local<Options>): void;",
    ] {
        let findings = compare(source, source, "export interface Options { x: string }", "export interface Options { x: string; y: number }");
        assert!(has_breaking_changes(&findings), "{source}: {findings:?}");
    }
}

#[test]
fn compatible_private_type_renames_and_widening_stay_compatible() {
    for (old, new) in [
        ("interface Options { x: string } export declare function api(x: Options): void;", "interface Settings { x: string } export declare function api(x: Settings): void;"),
        ("interface Options { x: string } export declare function api(x: Options): void;", "interface Options { x: string; y?: number } export declare function api(x: Options): void;"),
        ("class Base { private hidden: string; method(): void {} } export class Public extends Base {}", "class Base { private hidden: number; method(): void {} } export class Public extends Base {}"),
    ] {
        let findings = compare(old, new, "", "");
        assert!(!has_breaking_changes(&findings), "{old} -> {new}: {findings:?}");
    }
}

#[test]
fn recursive_type_dependencies_terminate_and_keep_visible_fields() {
    let old = "interface Link { next?: Link; value: string } export declare function api(): Link;";
    let new = "interface Link { next?: Link; value: number } export declare function api(): Link;";
    let findings = compare(old, new, "", "");
    assert!(has_breaking_changes(&findings), "{findings:?}");
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn real_hover_enrichment_preserves_reachable_type_changes() {
    let old = package(
        "interface Result { x: string } export declare function api(): Result;",
        "",
    );
    let new = package(
        "interface Result { x: number } export declare function api(): Result;",
        "",
    );
    let (mut old_exports, _) = discover_exports(old.path()).unwrap();
    let (mut new_exports, _) = discover_exports(new.path()).unwrap();
    for (exports, root) in [
        (&mut old_exports, old.path()),
        (&mut new_exports, new.path()),
    ] {
        let result = interlint::lsp::enrich_exports(exports, root).await.unwrap();
        assert!(
            matches!(result, interlint::lsp::EnrichResult::Enriched),
            "{result:?}"
        );
    }
    let findings = compare_exports(&old_exports, &new_exports, new.path(), true);
    assert!(has_breaking_changes(&findings), "{findings:?}");
}

#[test]
fn generic_parameters_shadow_local_declarations() {
    let findings = compare(
        "type T = string; export declare function api<T>(x: T): T;",
        "type T = number; export declare function api<T>(x: T): T;",
        "",
        "",
    );
    assert!(!has_breaking_changes(&findings), "{findings:?}");
}

#[test]
fn package_type_expansion_is_bounded_and_deterministic() {
    let mut source = String::from("type Leaf = { x: string; y: number };\n");
    for depth in 0..6 {
        let previous = if depth == 0 {
            "Leaf".into()
        } else {
            format!("Level{}", depth - 1)
        };
        source.push_str(&format!(
            "type Level{depth} = {{ a: {previous}; b: {previous}; c: {previous} }};\n"
        ));
    }
    for index in 0..100 {
        source.push_str(&format!(
            "export declare function api{index:03}(): Level5;\n"
        ));
    }
    let directory = package(&source, "");
    let (_, first) = discover_exports(directory.path()).unwrap();
    let (_, second) = discover_exports(directory.path()).unwrap();
    assert!(first
        .iter()
        .any(|warning| warning.incomplete && warning.message.contains("budget exhausted")));
    assert_eq!(
        first
            .iter()
            .map(|warning| &warning.message)
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(|warning| &warning.message)
            .collect::<Vec<_>>()
    );
}
