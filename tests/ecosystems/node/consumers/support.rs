//! Consumer compilation/execution provides an independent compatibility oracle.
#![cfg(feature = "lsp-tests")]

use interlint::{compare, discover::discover_exports};
use serde_json::Value;
use std::process::Command;

fn package(specification: &Value) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        specification["manifest"].to_string(),
    )
    .unwrap();
    for (name, contents) in specification["files"].as_object().unwrap() {
        let file = directory.path().join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents.as_str().unwrap()).unwrap();
    }
    directory
}

pub(crate) fn assert_consumer_contracts(specifications: &str) {
    let cases: Vec<Value> = serde_json::from_str(specifications).unwrap();
    assert!(!cases.is_empty(), "consumer topic must contain cases");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let old = package(&case["old"]);
        let new = package(&case["new"]);
        let (before, old_warnings) = discover_exports(old.path()).unwrap();
        let (after, new_warnings) = discover_exports(new.path()).unwrap();
        assert!(
            old_warnings.is_empty() && new_warnings.is_empty(),
            "{name}: {old_warnings:?} {new_warnings:?}"
        );
        let mut findings = compare::compare_exports(&before, &after, new.path(), false);
        findings.extend(compare::compare_structural_diff(
            old.path(),
            new.path(),
            &before,
            &after,
        ));
        findings.extend(compare::compare_exports_map_diff(old.path(), new.path()));
        let breaking = case["breaking"].as_bool().unwrap();
        assert_eq!(
            compare::has_breaking_changes(&findings),
            breaking,
            "{name}: {findings:?}"
        );
        for (version, root, succeeds) in [("old", old.path(), true), ("new", new.path(), !breaking)]
        {
            let consumer = case["consumer"].as_str().unwrap();
            let mut command = if case["runtime"] == "typescript" {
                std::fs::write(root.join("consumer.ts"), consumer).unwrap();
                let mut command = Command::new("tsc");
                command.args([
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
                ]);
                command
            } else {
                let mut command = Command::new("node");
                command.args(["--input-type=module", "--eval", consumer]);
                command
            };
            let output = command.current_dir(root).output().unwrap();
            assert_eq!(
                output.status.success(),
                succeeds,
                "{name}/{version}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
