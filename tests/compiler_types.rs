//! Compare semantic type findings with independently compiled consumers.
#![cfg(feature = "lsp-tests")]

use interlint::{compare, discover::discover_exports, lsp};
use serde_json::Value;
use std::process::Command;

#[tokio::test]
async fn evaluated_public_types_agree_with_strict_consumers() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/compiler-types.json")).unwrap();
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
                    "import {{api}} from './index.js';\n{}",
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
            let result = lsp::enrich_exports(&mut discovered, root).await.unwrap();
            assert!(
                matches!(result, lsp::EnrichResult::Enriched),
                "{name}/{version}: {result:?}"
            );
            exports.push(discovered);
        }
        let mut findings = compare::compare_exports(&exports[0], &exports[1], new.path(), false);
        findings.extend(compare::compare_structural_diff(
            old.path(),
            new.path(),
            &exports[0],
            &exports[1],
        ));
        if compare::has_breaking_changes(&findings) != breaking {
            failures.push(format!(
                "{name}: expected breaking={breaking}, got {findings:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn unresolved_public_types_cannot_report_complete_analysis() {
    for source in [
        "export declare function api(): MissingType;",
        "export type Open<T> = { [K in keyof T as `get${Capitalize<K & string>}`]: T[K] };",
    ] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"unresolved-types","types":"index.d.ts"}"#,
        )
        .unwrap();
        std::fs::write(directory.path().join("index.d.ts"), source).unwrap();
        let (mut exports, _) = discover_exports(directory.path()).unwrap();
        let result = lsp::enrich_exports(&mut exports, directory.path())
            .await
            .unwrap();
        assert!(
            matches!(result, lsp::EnrichResult::Degraded(_)),
            "{source}: {result:?}"
        );
    }
}
