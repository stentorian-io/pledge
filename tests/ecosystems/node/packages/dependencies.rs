#![cfg(feature = "lsp-tests")]

use interlint::{compare, discover::discover_exports, lsp};
use serde_json::json;

fn package(symbol: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for name in ["package", "dependency"] {
        std::fs::create_dir(root.join(name)).unwrap();
    }
    std::fs::write(
        root.join("dependency/package.json"),
        json!({"name":"local-contract","version":"1.0.0","types":"index.d.ts"}).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("dependency/index.d.ts"),
        format!("export declare function {symbol}(): string;"),
    )
    .unwrap();
    std::fs::write(root.join("package/package.json"), json!({"name":"dependency-barrel","version":"1.0.0","exports":"./index.d.ts","dependencies":{"local-contract":"file:../dependency"}}).to_string()).unwrap();
    std::fs::write(
        root.join("package/index.d.ts"),
        "export * from 'local-contract';",
    )
    .unwrap();
    directory
}

#[tokio::test]
async fn enrichment_refreshes_exports_after_installing_dependencies() {
    let old = package("api");
    let new = package("replacement");
    let mut maps = Vec::new();
    for directory in [&old, &new] {
        let root = directory.path().join("package");
        let (mut exports, _) = discover_exports(&root).unwrap();
        let result = lsp::enrich_exports(&mut exports, &root).await.unwrap();
        assert!(matches!(result, lsp::EnrichResult::Enriched), "{result:?}");
        assert!(
            !exports.keys().any(|key| key.name.starts_with("*<")),
            "{exports:?}"
        );
        maps.push(exports);
    }
    assert!(maps[0].keys().any(|key| key.name == "api"), "{:?}", maps[0]);
    let findings = compare::compare_exports(&maps[0], &maps[1], &new.path().join("package"), true);
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "export-removed" && finding.message.contains("api")),
        "{findings:?}"
    );
    for (directory, succeeds) in [(&old, true), (&new, false)] {
        let root = directory.path().join("package");
        std::fs::write(
            root.join("consumer.ts"),
            "import {api} from './index.js'; api();",
        )
        .unwrap();
        let output = std::process::Command::new("tsc")
            .current_dir(&root)
            .args([
                "--noEmit",
                "--strict",
                "--skipLibCheck",
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
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
async fn prepared_discovery_resolves_dependency_exports_before_enrichment() {
    let directory = package("api");
    let root = directory.path().join("package");
    assert!(lsp::client::prepare_dependencies(&root).await.unwrap());
    let (mut exports, warnings) = discover_exports(&root).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|key| key.name == "api"));
    assert!(!exports.keys().any(|key| key.name.starts_with("*<")));
    let result = lsp::enrich_prepared_exports(&mut exports, &root)
        .await
        .unwrap();
    assert!(matches!(result, lsp::EnrichResult::Enriched), "{result:?}");
}

#[tokio::test]
async fn missing_dependencies_degrade_instead_of_enriching_placeholders() {
    let directory = package("api");
    std::fs::remove_dir_all(directory.path().join("dependency")).unwrap();
    let root = directory.path().join("package");
    let (mut exports, _) = discover_exports(&root).unwrap();
    let result = lsp::enrich_exports(&mut exports, &root).await.unwrap();
    assert!(
        matches!(result, lsp::EnrichResult::Degraded(_)),
        "{result:?}"
    );
}

#[tokio::test]
async fn compiler_resolves_dependencies_that_publish_only_an_exports_map() {
    let directory = package("api");
    let root = directory.path().join("package");
    let dependency = directory.path().join("dependency");
    std::fs::remove_file(dependency.join("index.d.ts")).unwrap();
    std::fs::create_dir(dependency.join("dist")).unwrap();
    std::fs::write(dependency.join("package.json"), json!({"name":"local-contract","version":"1.0.0","exports":{"types":"./dist/contracts.d.ts"}}).to_string()).unwrap();
    std::fs::write(
        dependency.join("dist/contracts.d.ts"),
        "export interface Options {value:string}",
    )
    .unwrap();
    std::fs::write(root.join("index.d.ts"), "import type {Options} from 'local-contract'; export declare function api(input: Options): void;").unwrap();
    let (mut exports, _) = discover_exports(&root).unwrap();
    let result = lsp::enrich_exports(&mut exports, &root).await.unwrap();
    assert!(matches!(result, lsp::EnrichResult::Enriched), "{result:?}");
    assert!(exports.values().any(|symbol| symbol.name == "api" && symbol.overloads.iter().any(|shape| {
        matches!(shape, interlint::types::shape::TypeShape::Function { params, .. } if matches!(&params[0].shape, interlint::types::shape::TypeShape::Object(fields) if fields.contains_key("value")))
    })), "{exports:?}");
}
