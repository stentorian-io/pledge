//! Public contracts retain strict consumer evidence under loose author settings.
#![cfg(feature = "lsp-tests")]

use pledge::core::findings::has_breaking_changes;
use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare;
use pledge::languages::typescript::compiler::{self, EnrichResult};
use std::process::Command;

struct StrictContract {
    option: &'static str,
    permissive: &'static str,
    restrictive: &'static str,
    broken_consumer: &'static str,
    compatible_consumer: &'static str,
}

async fn strict_contract_survives_author_setting(contract: StrictContract) {
    for author_strict in [false, true] {
        for breaking in [true, false] {
            let (old, new, consumer) = if breaking {
                (
                    contract.permissive,
                    contract.restrictive,
                    contract.broken_consumer,
                )
            } else {
                (
                    contract.restrictive,
                    contract.permissive,
                    contract.compatible_consumer,
                )
            };
            let mut exports = Vec::new();
            let mut directories = Vec::new();
            for (version, source) in [("old", old), ("new", new)] {
                let directory = tempfile::tempdir().unwrap();
                let root = directory.path();
                std::fs::write(
                    root.join("package.json"),
                    r#"{"name":"strict-contract","type":"module","exports":"./index.d.ts"}"#,
                )
                .unwrap();
                std::fs::write(root.join("index.d.ts"), source).unwrap();
                std::fs::write(
                    root.join("consumer.ts"),
                    format!("import {{api}} from './index.js';\n{consumer}"),
                )
                .unwrap();
                let mut config = serde_json::json!({
                    "compilerOptions": {
                        "strict": true, "target": "ES2022", "module": "NodeNext",
                        "moduleResolution": "NodeNext", "noEmit": true, "skipLibCheck": true
                    },
                    "files": ["consumer.ts"]
                });
                config["compilerOptions"][contract.option] = author_strict.into();
                std::fs::write(root.join("tsconfig.json"), config.to_string()).unwrap();

                let output = Command::new("tsc")
                    .current_dir(root)
                    .args(["--project", "tsconfig.json"])
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.success(),
                    !(author_strict && breaking && version == "new"),
                    "{}/{author_strict}/{breaking}/{version}: {}{}",
                    contract.option,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );

                let (mut discovered, warnings) = discover_exports(root).unwrap();
                assert!(warnings.is_empty(), "{warnings:?}");
                let enriched = compiler::enrich_exports(&mut discovered, root)
                    .await
                    .unwrap();
                assert!(matches!(enriched, EnrichResult::Enriched), "{enriched:?}");
                exports.push(discovered);
                directories.push(directory);
            }

            let findings =
                compare::compare_exports(&exports[0], &exports[1], directories[1].path(), false);
            assert_eq!(
                has_breaking_changes(&findings),
                breaking,
                "{}/{author_strict}/{breaking}: {findings:?}",
                contract.option
            );
            assert!(
                !findings
                    .iter()
                    .any(|finding| finding.rule == "analysis-incomplete"),
                "{findings:?}"
            );
            if breaking {
                assert!(
                    findings
                        .iter()
                        .any(|finding| finding.rule == "param-type-narrowed"),
                    "{findings:?}"
                );
            }
        }
    }
}

#[tokio::test]
async fn strict_null_consumers_remain_protected_under_loose_author_settings() {
    strict_contract_survives_author_setting(StrictContract {
        option: "strictNullChecks",
        permissive: "export declare function api(value: string | null): void;",
        restrictive: "export declare function api(value: string): void;",
        broken_consumer: "api(null);",
        compatible_consumer: "api('value');",
    })
    .await;
}

#[tokio::test]
async fn strict_callback_consumers_remain_protected_under_loose_author_settings() {
    strict_contract_survives_author_setting(StrictContract {
        option: "strictFunctionTypes",
        permissive: "export declare function api(handler: (value: string) => void): void;",
        restrictive:
            "export declare function api(handler: (value: string | number) => void): void;",
        broken_consumer: "api((value: string) => {});",
        compatible_consumer: "api((value: string | number) => {});",
    })
    .await;
}

#[tokio::test]
async fn exact_optional_consumers_remain_protected_under_loose_author_settings() {
    strict_contract_survives_author_setting(StrictContract {
        option: "exactOptionalPropertyTypes",
        permissive: "export declare function api(value: { x?: string | undefined }): void;",
        restrictive: "export declare function api(value: { x?: string }): void;",
        broken_consumer: "api({ x: undefined });",
        compatible_consumer: "api({ x: 'value' }); api({});",
    })
    .await;
}
