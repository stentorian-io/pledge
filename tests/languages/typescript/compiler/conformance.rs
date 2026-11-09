//! Adapt pinned upstream language cases to old/new package consumer contracts.
#![cfg(feature = "lsp-tests")]

use crate::typescript_consumers::TscBatch;
use pledge::core::findings::has_breaking_changes;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare;
use pledge::languages::typescript::compiler::{self, EnrichResult};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumerCase {
    name: String,
    profile: String,
    old: String,
    new: String,
    consumer: String,
    breaking: bool,
    upstream: String,
}

async fn compare_consumers(
    upstream: &str,
    profile: &str,
    module: &str,
    resolution: &str,
    extension: &str,
) {
    let legacy_resolution_deprecation =
        resolution == "Node" && super::version_support::compiler_version().0 >= 6;
    let cases: Vec<ConsumerCase> = serde_json::from_str(include_str!(
        "../../../fixtures/typescript/compiler/upstream-contracts.json"
    ))
    .unwrap();
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|case| case.profile == profile && case.upstream == upstream)
        .collect();
    assert!(cases.iter().any(|case| case.breaking));
    assert!(cases.iter().any(|case| !case.breaking));

    let mut compiler = compiler::CompilerSession::default();
    let mut consumers = TscBatch::default();
    for case in cases {
        assert!(case.upstream.starts_with("typescript:conformance/"));
        let mut exports = Vec::new();
        let mut directories = Vec::new();
        for (version, source) in [("old", &case.old), ("new", &case.new)] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            let manifest = serde_json::json!({
                "name": "upstream-consumer",
                "type": if module == "CommonJS" { "commonjs" } else { "module" },
                "exports": format!("./index.{extension}"),
            });
            std::fs::write(root.join("package.json"), manifest.to_string()).unwrap();
            std::fs::write(root.join(format!("index.{extension}")), source).unwrap();
            let imported = match extension {
                "d.mts" => "mjs",
                "d.cts" => "cjs",
                _ => "js",
            };
            std::fs::write(
                root.join("consumer.ts"),
                format!(
                    "import {{api}} from './index.{imported}';\n{}",
                    case.consumer
                ),
            )
            .unwrap();
            let mut config = serde_json::json!({
                "compilerOptions": { "strict": true, "exactOptionalPropertyTypes": true,
                    "target": "ES2022", "module": module, "moduleResolution": resolution,
                    "noEmit": true, "skipLibCheck": true },
                "files": ["consumer.ts"],
            });
            if legacy_resolution_deprecation {
                config["compilerOptions"]["ignoreDeprecations"] = "6.0".into();
            }
            std::fs::write(root.join("tsconfig.json"), config.to_string()).unwrap();

            let (mut discovered, warnings) = discover_exports(root).unwrap();
            assert!(warnings.is_empty(), "{}/{version}: {warnings:?}", case.name);
            let enriched = compiler
                .enrich_exports(&mut discovered, root)
                .await
                .unwrap();
            assert!(
                matches!(enriched, EnrichResult::Enriched),
                "{}/{version}: {enriched:?}",
                case.name
            );
            exports.push(discovered);
            directories.push(directory);
        }
        let mut findings =
            compare::compare_exports(&exports[0], &exports[1], directories[1].path(), false);
        findings.extend(compare::structure::compare_structural_diff(
            directories[0].path(),
            directories[1].path(),
            &exports[0],
            &exports[1],
        ));
        assert_eq!(
            has_breaking_changes(&findings),
            case.breaking,
            "{}: {findings:?}",
            case.name
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{}: {findings:?}",
            case.name
        );
        for (directory, (version, succeeds)) in directories
            .into_iter()
            .zip([("old", true), ("new", !case.breaking)])
        {
            consumers.push(
                directory,
                &["--project", "tsconfig.json"],
                succeeds,
                format!("{}/{version}", case.name),
            );
        }
    }
    drop(compiler);
    consumers.assert_outcomes();
}

#[tokio::test]
async fn mapped_modifier_consumers_agree_under_nodenext() {
    compare_consumers(
        "typescript:conformance/types/mapped/mappedTypes1.ts",
        "compiler-nodenext",
        "NodeNext",
        "NodeNext",
        "d.ts",
    )
    .await;
}

#[tokio::test]
async fn tuple_rest_consumers_agree_under_node16() {
    compare_consumers(
        "typescript:conformance/types/rest/genericRestParameters1.ts",
        "compiler-node16",
        "Node16",
        "Node16",
        "d.mts",
    )
    .await;
}

#[tokio::test]
async fn template_union_consumers_agree_under_commonjs() {
    compare_consumers(
        "typescript:conformance/types/literal/templateLiteralTypes1.ts",
        "compiler-commonjs",
        "CommonJS",
        "Node",
        "d.cts",
    )
    .await;
}

#[tokio::test]
async fn indexed_results_agree_with_typescript_consumers() {
    compare_consumers(
        "typescript:conformance/types/keyof/keyofAndIndexedAccess.ts",
        "compiler-nodenext",
        "NodeNext",
        "NodeNext",
        "d.ts",
    )
    .await;
}

#[tokio::test]
async fn intersection_inputs_agree_with_typescript_consumers() {
    compare_consumers(
        "typescript:conformance/types/intersection/intersectionAndUnionTypes.ts",
        "compiler-nodenext",
        "NodeNext",
        "NodeNext",
        "d.ts",
    )
    .await;
}

#[tokio::test]
async fn discriminated_results_agree_with_exhaustive_consumers() {
    compare_consumers(
        "typescript:conformance/types/union/discriminatedUnionTypes1.ts",
        "compiler-nodenext",
        "NodeNext",
        "NodeNext",
        "d.ts",
    )
    .await;
}

#[tokio::test]
async fn merged_interface_results_agree_with_typescript_consumers() {
    compare_consumers(
        "typescript:conformance/interfaces/declarationMerging/mergeTwoInterfaces.ts",
        "compiler-nodenext",
        "NodeNext",
        "NodeNext",
        "d.ts",
    )
    .await;
}

#[tokio::test]
async fn call_and_construct_signatures_agree_with_typescript_consumers() {
    compare_consumers(
        "typescript:conformance/interfaces/interfaceDeclarations/interfaceWithCallAndConstructSignature.ts",
        "compiler-nodenext",
        "NodeNext",
        "NodeNext",
        "d.ts",
    )
    .await;
}
