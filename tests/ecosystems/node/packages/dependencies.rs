#![cfg(feature = "lsp-tests")]

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare;
use pledge::languages::typescript::compiler;
use serde_json::json;

fn dependency_archive(path: &std::path::Path, manifest: &serde_json::Value) {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    let manifest = manifest.to_string();
    let mut header = tar::Header::new_gnu();
    header.set_size(manifest.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append_data(&mut header, "package/package.json", manifest.as_bytes())
        .unwrap();
    std::fs::write(path, archive.into_inner().unwrap().finish().unwrap()).unwrap();
}

#[tokio::test]
async fn development_peer_conflicts_do_not_block_runtime_dependency_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("package");
    std::fs::create_dir(&root).unwrap();
    let runtime = directory.path().join("runtime.tgz");
    let development = directory.path().join("development.tgz");
    dependency_archive(
        &runtime,
        &json!({"name":"runtime-contract","version":"1.0.0"}),
    );
    dependency_archive(
        &development,
        &json!({
            "name":"development-tool", "version":"1.0.0",
            "peerDependencies":{"runtime-contract":"^2.0.0"}
        }),
    );
    let manifest = json!({
        "name":"prepared-package", "version":"1.0.0",
        "dependencies":{"runtime-contract":format!("file:{}", runtime.display())},
        "devDependencies":{"development-tool":format!("file:{}", development.display())}
    })
    .to_string();
    std::fs::write(root.join("package.json"), &manifest).unwrap();
    std::fs::write(
        root.join(".npmrc"),
        format!(
            "offline=true\ncache={}\n",
            directory.path().join("cache").display()
        ),
    )
    .unwrap();
    let lockfile = r#"{"name":"prepared-package","lockfileVersion":3,"packages":{}}"#;
    std::fs::write(root.join("package-lock.json"), lockfile).unwrap();

    let prepared = pledge::ecosystems::node::dependencies::prepare_dependencies(&root).await;

    assert!(prepared.unwrap());
    assert!(root
        .join("node_modules/runtime-contract/package.json")
        .is_file());
    assert!(!root.join("node_modules/development-tool").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("package.json")).unwrap(),
        manifest
    );
    assert_eq!(
        std::fs::read_to_string(root.join("package-lock.json")).unwrap(),
        lockfile
    );
}

#[tokio::test]
async fn relative_tarball_requirements_resolve_from_the_original_package() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("package");
    std::fs::create_dir(&root).unwrap();
    dependency_archive(
        &root.join("runtime.tgz"),
        &json!({"name":"runtime-contract","version":"1.0.0"}),
    );
    std::fs::write(
        root.join("package.json"),
        json!({
            "name":"prepared-package", "version":"1.0.0",
            "dependencies":{"runtime-contract":"./runtime.tgz"}
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join(".npmrc"),
        format!(
            "offline=true\ncache={}\n",
            directory.path().join("cache").display()
        ),
    )
    .unwrap();

    pledge::ecosystems::node::dependencies::prepare_dependencies(&root)
        .await
        .unwrap();

    assert!(root
        .join("node_modules/runtime-contract/package.json")
        .is_file());
}

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
        let result = compiler::enrich_exports(&mut exports, &root).await.unwrap();
        assert!(
            matches!(result, compiler::EnrichResult::Enriched),
            "{result:?}"
        );
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
    assert!(
        pledge::ecosystems::node::dependencies::prepare_dependencies(&root)
            .await
            .unwrap()
    );
    let (mut exports, warnings) = discover_exports(&root).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|key| key.name == "api"));
    assert!(!exports.keys().any(|key| key.name.starts_with("*<")));
    let result = compiler::enrich_prepared_exports(&mut exports, &root)
        .await
        .unwrap();
    assert!(
        matches!(result, compiler::EnrichResult::Enriched),
        "{result:?}"
    );
}

#[tokio::test]
async fn missing_dependencies_degrade_instead_of_enriching_placeholders() {
    let directory = package("api");
    std::fs::remove_dir_all(directory.path().join("dependency")).unwrap();
    let root = directory.path().join("package");
    let (mut exports, _) = discover_exports(&root).unwrap();
    let result = compiler::enrich_exports(&mut exports, &root).await.unwrap();
    assert!(
        matches!(result, compiler::EnrichResult::Degraded(_)),
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
    let result = compiler::enrich_exports(&mut exports, &root).await.unwrap();
    assert!(
        matches!(result, compiler::EnrichResult::Enriched),
        "{result:?}"
    );
    assert!(exports.values().any(|symbol| symbol.name == "api" && symbol.overloads.iter().any(|shape| {
        matches!(shape, pledge::languages::typescript::types::shape::TypeShape::Function { params, .. } if matches!(&params[0].shape, pledge::languages::typescript::types::shape::TypeShape::Object(fields) if fields.contains_key("value")))
    })), "{exports:?}");
}
