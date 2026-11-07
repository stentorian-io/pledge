//! Run with the selected PATH compiler, including older supported API implementations.
#![cfg(feature = "lsp-tests")]

use pledge::core::findings::has_breaking_changes;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::{compare, model::ExportMap};
use pledge::languages::typescript::compiler::{self, EnrichResult};
use std::path::Path;
use std::process::{Command, Output};

pub(super) fn compiler_version() -> (u32, u32) {
    let output = Command::new("tsc").arg("--version").output().unwrap();
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap();
    let mut components = version.trim().strip_prefix("Version ").unwrap().split('.');
    (
        components.next().unwrap().parse().unwrap(),
        components.next().unwrap().parse().unwrap(),
    )
}

fn package(source: &str, consumer: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        r#"{"name":"compiler-version-contract","type":"module","exports":"./index.ts"}"#,
    )
    .unwrap();
    std::fs::write(directory.path().join("index.ts"), source).unwrap();
    std::fs::write(directory.path().join("consumer.ts"), consumer).unwrap();
    directory
}

fn compile(root: &Path) -> Output {
    Command::new("tsc")
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
            "index.ts",
            "consumer.ts",
        ])
        .output()
        .unwrap()
}

async fn complete_exports(root: &Path) -> ExportMap {
    let (mut exports, warnings) = discover_exports(root).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let result = compiler::enrich_exports(&mut exports, root).await.unwrap();
    assert!(matches!(result, EnrichResult::Enriched), "{result:?}");
    exports
}

#[tokio::test]
async fn language_and_library_features_follow_the_selected_compiler_version() {
    let version = compiler_version();
    for (name, source, minimum) in [
        (
            "const type parameters",
            "export declare function api<const T extends string>(x:T):T;",
            Some((5, 0)),
        ),
        (
            "NoInfer",
            "export declare function api<T>(x:T,y:NoInfer<T>):T;",
            Some((5, 4)),
        ),
        (
            "generic typed arrays",
            "export declare function api():Uint8Array<ArrayBuffer>;",
            Some((5, 7)),
        ),
        (
            "malformed declaration",
            "export declare function api(x:string:):string; export declare const valid:number;",
            None,
        ),
    ] {
        let directory = package(source, "");
        let output = compile(directory.path());
        let supported = minimum.is_some_and(|minimum| version >= minimum);
        assert_eq!(
            output.status.success(),
            supported,
            "{name} on {version:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let (mut exports, mut warnings) = discover_exports(directory.path()).unwrap();
        let enrichment = compiler::enrich_exports(&mut exports, directory.path())
            .await
            .unwrap();
        enrichment.reconcile_discovery_warnings(&mut warnings);
        let complete = matches!(enrichment, EnrichResult::Enriched)
            && !warnings.iter().any(|warning| warning.incomplete);

        assert_eq!(
            complete, supported,
            "{name} on {version:?}: {enrichment:?}, {warnings:?}"
        );
    }
}

#[tokio::test]
async fn inferred_predicate_changes_follow_the_compilers_consumer_behavior() {
    let consumer = "import {api} from './index.js'; const input:unknown='text'; if(api(input)) input.toUpperCase();";
    let old = package(
        "export function api(x:unknown) { return typeof x === 'string'; }",
        consumer,
    );
    let new = package(
        "export function api(x:unknown):boolean { return typeof x === 'string'; }",
        consumer,
    );
    let old_compiles = compile(old.path()).status.success();
    assert_eq!(old_compiles, compiler_version() >= (5, 5));
    assert!(!compile(new.path()).status.success());

    let old_exports = complete_exports(old.path()).await;
    let new_exports = complete_exports(new.path()).await;
    let findings = compare::compare_exports(&old_exports, &new_exports, new.path(), false);

    assert_eq!(
        has_breaking_changes(&findings),
        old_compiles,
        "{findings:?}"
    );
    assert_eq!(
        findings
            .iter()
            .any(|finding| finding.rule == "type-guard-widened"),
        old_compiles,
        "{findings:?}"
    );
}

#[tokio::test]
async fn parameter_narrowing_breaks_consumers_across_compiler_versions() {
    let consumer = "import {api} from './index.js'; api(1);";
    let old = package(
        "export declare function api(x:string|number):string;",
        consumer,
    );
    let new = package("export declare function api(x:string):string;", consumer);
    assert!(compile(old.path()).status.success());
    assert!(!compile(new.path()).status.success());

    let old_exports = complete_exports(old.path()).await;
    let new_exports = complete_exports(new.path()).await;
    let findings = compare::compare_exports(&old_exports, &new_exports, new.path(), false);

    assert!(has_breaking_changes(&findings), "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "param-type-narrowed"),
        "{findings:?}"
    );
}
