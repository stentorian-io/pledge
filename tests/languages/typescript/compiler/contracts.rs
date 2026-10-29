//! Compare semantic type findings with independently compiled consumers.
#![cfg(feature = "lsp-tests")]

use interlint::ecosystems::node::discovery::discover_exports;
use interlint::languages::javascript::compare;
use interlint::languages::typescript::compiler;
use serde_json::Value;
use std::process::Command;

// Each topic is independently selectable while sharing the same consumer oracle.
macro_rules! consumer_contracts {
    ($name:ident, $fixture:literal) => {
        #[tokio::test]
        async fn $name() {
            assert_consumer_contracts(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/typescript/compiler/",
                $fixture,
                ".json"
            )))
            .await;
        }
    };
}

consumer_contracts!(accessors, "accessors");
consumer_contracts!(classes, "classes");
consumer_contracts!(callables, "callables");
consumer_contracts!(exact_optional, "exact-optional");
consumer_contracts!(generics, "generics");
consumer_contracts!(inference, "inference");
consumer_contracts!(iterator_inputs, "iterator-inputs");
consumer_contracts!(javascript_interop, "javascript-interop");
consumer_contracts!(public_types, "public-types");
consumer_contracts!(readonly, "readonly");
consumer_contracts!(satisfies, "satisfies");

async fn assert_consumer_contracts(specifications: &str) {
    let cases: Vec<Value> = serde_json::from_str(specifications).unwrap();
    assert!(!cases.is_empty(), "consumer topic must contain cases");
    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let breaking = case["breaking"].as_bool().unwrap();
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        let extension = case["extension"].as_str().unwrap();
        let mut exports = Vec::new();
        for (version, directory, succeeds) in [("old", &old, true), ("new", &new, !breaking)] {
            let root = directory.path();
            std::fs::write(
                root.join("package.json"),
                serde_json::json!({"name":"compiler-types","type":"module","exports":format!("./index.{extension}")}).to_string(),
            ).unwrap();
            std::fs::write(
                root.join(format!("index.{extension}")),
                case[version].as_str().unwrap(),
            )
            .unwrap();
            std::fs::write(
                root.join("consumer.ts"),
                format!(
                    "import {{api}} from './index.{}';\n{}",
                    if extension == "cjs" { "cjs" } else { "js" },
                    case["consumer"].as_str().unwrap()
                ),
            )
            .unwrap();
            let output = Command::new("tsc")
                .current_dir(root)
                .args([
                    "--noEmit",
                    "--strict",
                    "--skipLibCheck",
                    "--allowJs",
                    "--checkJs",
                    "--target",
                    "ES2022",
                    "--module",
                    "NodeNext",
                    "--moduleResolution",
                    "NodeNext",
                    "consumer.ts",
                ])
                .args(
                    case["exact_optional"]
                        .as_bool()
                        .unwrap_or(false)
                        .then_some("--exactOptionalPropertyTypes"),
                )
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{name}/{version}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            let (mut discovered, warnings) = discover_exports(root).unwrap();
            assert!(warnings.is_empty(), "{name}/{version}: {warnings:?}");
            let result = compiler::enrich_exports(&mut discovered, root)
                .await
                .unwrap();
            assert!(
                matches!(result, compiler::EnrichResult::Enriched),
                "{name}/{version}: {result:?}"
            );
            exports.push(discovered);
        }
        let mut findings = compare::compare_exports(&exports[0], &exports[1], new.path(), false);
        findings.extend(
            interlint::languages::javascript::compare::structure::compare_structural_diff(
                old.path(),
                new.path(),
                &exports[0],
                &exports[1],
            ),
        );
        if interlint::core::findings::has_breaking_changes(&findings) != breaking {
            failures.push(format!(
                "{name}: expected breaking={breaking}, got {findings:?}"
            ));
        }
        if let Some(rule) = case["forbidden_rule"].as_str() {
            if findings.iter().any(|finding| finding.rule == rule) {
                failures.push(format!("{name}: unexpected {rule}: {findings:?}"));
            }
        }
        if let Some(rule) = case["required_rule"].as_str() {
            if !findings.iter().any(|finding| finding.rule == rule) {
                failures.push(format!("{name}: missing {rule}: {findings:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn unresolved_public_types_cannot_report_complete_analysis() {
    for source in [
        "export declare function api(): MissingType;",
        "export type Open<T> = { [K in keyof T]: MissingType };",
        "export const api = { value: missing } satisfies { value: unknown };",
    ] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"unresolved-types","types":"index.d.ts"}"#,
        )
        .unwrap();
        std::fs::write(directory.path().join("index.d.ts"), source).unwrap();
        let (mut exports, _) = discover_exports(directory.path()).unwrap();
        let result = compiler::enrich_exports(&mut exports, directory.path())
            .await
            .unwrap();
        assert!(
            matches!(result, compiler::EnrichResult::Degraded(_)),
            "{source}: {result:?}"
        );
        if source.contains("satisfies") {
            assert!(exports.values().any(|symbol| matches!(
                symbol.satisfies_type,
                Some(
                    interlint::languages::typescript::types::shape::TypeShape::Satisfies {
                        narrowed: None,
                        ..
                    }
                )
            )));
        }
    }
}

#[tokio::test]
async fn completed_compiler_evaluation_retires_only_source_type_limits() {
    for missing_source in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"type-limits","exports":"./index.d.ts"}"#,
        )
        .unwrap();
        let mut source = String::from("type T0 = {value:string};\n");
        for index in 1..50 {
            source.push_str(&format!("type T{index} = T{};\n", index - 1));
        }
        source.push_str("export declare function api(): T49;\n");
        if missing_source {
            source.push_str("export * from './missing.js';\n");
        }
        std::fs::write(directory.path().join("index.d.ts"), source).unwrap();
        let (mut exports, mut warnings) = discover_exports(directory.path()).unwrap();
        assert!(
            warnings.iter().any(|warning| warning.type_expansion_only),
            "{warnings:?}"
        );
        let result = compiler::enrich_exports(&mut exports, directory.path())
            .await
            .unwrap();
        assert!(
            matches!(result, compiler::EnrichResult::Enriched),
            "{result:?}"
        );
        result.reconcile_discovery_warnings(&mut warnings);
        assert_eq!(
            warnings.iter().any(|warning| warning.incomplete),
            missing_source,
            "{warnings:?}"
        );
        assert!(
            warnings.iter().all(|warning| !warning.type_expansion_only),
            "{warnings:?}"
        );
    }
}
