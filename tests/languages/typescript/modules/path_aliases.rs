//! Path aliases compatibility checks.

use pledge::languages::javascript::model::ExportKey;
use serde_json::{json, Value};

fn alias_cases() -> Vec<(&'static str, &'static str, Value, &'static str)> {
    vec![
        (
            "suffix",
            "@app/thing.ts",
            json!({"@app/*.ts":["./src/*.ts"]}),
            "src/thing.ts",
        ),
        (
            "exact extensionless",
            "@app",
            json!({"@app":["./src/thing"]}),
            "src/thing.ts",
        ),
        (
            "declaration extension",
            "@app/thing",
            json!({"@app/*":["./src/*"]}),
            "src/thing.d.ts",
        ),
        (
            "exact precedence",
            "@app/thing",
            json!({"@app/*":["./decoy.ts"],"@app/thing":["./src/thing.ts"]}),
            "src/thing.ts",
        ),
        (
            "longest prefix",
            "@app/thing",
            json!({"*":["./decoy.ts"],"@app/*":["./src/*.ts"]}),
            "src/thing.ts",
        ),
        (
            "equal prefix first suffix",
            "@app/thing.ts",
            json!({"@app/*.ts":["./src/*.ts"],"@app/*":["./decoy.ts"]}),
            "src/thing.ts",
        ),
        (
            "equal prefix first wildcard",
            "@app/thing.ts",
            json!({"@app/*":["./src/thing.ts"],"@app/*.ts":["./decoy.ts"]}),
            "src/thing.ts",
        ),
        (
            "replacement fallbacks",
            "@app",
            json!({"@app":["./missing.ts","./src/thing"]}),
            "src/thing.d.ts",
        ),
        (
            "directory declarations",
            "@app",
            json!({"@app":["./src"]}),
            "src/index.d.ts",
        ),
        (
            "javascript substitution",
            "@app",
            json!({"@app":["./src/thing.js"]}),
            "src/thing.d.ts",
        ),
    ]
}

fn alias_package(specifier: &str, paths: Value, target: &str) -> tempfile::TempDir {
    let package = tempfile::tempdir().unwrap();
    let root = package.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join(target), "export declare const selected: number;").unwrap();
    std::fs::write(root.join("decoy.ts"), "export declare const decoy: string;").unwrap();
    std::fs::write(
        root.join("index.ts"),
        format!("export * from {specifier:?};"),
    )
    .unwrap();
    std::fs::write(
        root.join("consumer.ts"),
        "import { selected } from './index'; const required: number = selected;",
    )
    .unwrap();
    std::fs::write(
        root.join("package.json"),
        json!({"name":"aliases","exports":{".":{"types":"./index.ts"}}}).to_string(),
    )
    .unwrap();
    std::fs::write(root.join("tsconfig.json"), json!({"compilerOptions":{"strict":true,"noEmit":true,"paths":paths},"files":["consumer.ts"]}).to_string()).unwrap();
    package
}

#[test]
fn aliases_select_one_pattern_and_probe_its_replacements() {
    for (name, specifier, paths, target) in alias_cases() {
        let package = alias_package(specifier, paths, target);

        let (exports, warnings) =
            pledge::ecosystems::node::discovery::discover_exports(package.path()).unwrap();

        assert!(warnings.is_empty(), "{name}: {warnings:?}");
        assert!(
            exports
                .keys()
                .any(|key| key.subpath == "." && key.name == "selected"),
            "{name}: {exports:?}"
        );
        assert!(
            !exports
                .keys()
                .any(|key| key.subpath == "." && key.name == "decoy"),
            "{name}: {exports:?}"
        );
    }
}

#[cfg(feature = "lsp-tests")]
#[test]
fn alias_selection_matches_the_typescript_consumer() {
    for (name, specifier, paths, target) in alias_cases() {
        let package = alias_package(specifier, paths, target);

        let result = std::process::Command::new("tsc")
            .current_dir(package.path())
            .args(["--project", "tsconfig.json"])
            .output()
            .unwrap();

        assert!(
            result.status.success(),
            "{name}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn missing_selected_alias_does_not_fall_back_to_a_less_specific_pattern() {
    let package = alias_package(
        "@app/thing",
        json!({"*":["./decoy.ts"],"@app/*":["./missing/*.ts"]}),
        "src/thing.ts",
    );

    let (exports, _) =
        pledge::ecosystems::node::discovery::discover_exports(package.path()).unwrap();

    assert!(
        !exports
            .keys()
            .any(|key| key.subpath == "." && key.name == "decoy"),
        "{exports:?}"
    );
}

#[test]
fn aliases_take_precedence_over_unrelated_sibling_files() {
    let package = alias_package("alias", json!({"alias":["./src/thing.ts"]}), "src/thing.ts");
    std::fs::write(package.path().join("alias.ts"), "export const decoy = 1;").unwrap();

    let (exports, warnings) =
        pledge::ecosystems::node::discovery::discover_exports(package.path()).unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        exports.keys().any(|key| key.name == "selected"),
        "{exports:?}"
    );
    assert!(
        !exports.keys().any(|key| key.name == "decoy"),
        "{exports:?}"
    );
}

#[test]
fn path_aliases_do_not_resolve_missing_relative_imports() {
    let package = alias_package("./missing", json!({"*":["./decoy.ts"]}), "src/thing.ts");

    let (exports, warnings) =
        pledge::ecosystems::node::discovery::discover_exports(package.path()).unwrap();

    assert!(
        !exports.keys().any(|key| key.name == "decoy"),
        "{exports:?}"
    );
    assert!(
        !warnings.is_empty(),
        "a missing relative import must remain unresolved"
    );
}

#[test]
fn typescript_hash_aliases_override_imports_but_runtime_javascript_keeps_imports() {
    let package = alias_package(
        "#alias",
        json!({"#alias":["./src/thing.ts"]}),
        "src/thing.ts",
    );
    std::fs::write(package.path().join("package.json"), json!({
        "name":"aliases", "type":"module", "exports":{".":{"types":"./index.ts","default":"./index.js"}},
        "imports":{"#alias":"./decoy.js"}
    }).to_string()).unwrap();
    std::fs::write(package.path().join("index.js"), "export * from '#alias';").unwrap();
    std::fs::write(package.path().join("decoy.js"), "export const runtime = 1;").unwrap();

    let (exports, warnings) =
        pledge::ecosystems::node::discovery::discover_exports(package.path()).unwrap();

    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        exports
            .keys()
            .any(|key| key.name == "selected" && key.condition.as_deref() == Some("types")),
        "{exports:?}"
    );
    assert!(
        exports
            .keys()
            .any(|key| key.name == "runtime" && key.condition.as_deref() == Some("default")),
        "{exports:?}"
    );
    assert_eq!(exports.len(), 2, "{exports:?}");
}

#[test]
fn tsconfig_alias_resolution() {
    // Create a package with tsconfig.json having path aliases and use discover_exports.
    // The @/utils alias should resolve to src/utils.ts.
    let pkg_dir = tempfile::tempdir().unwrap();

    std::fs::create_dir_all(pkg_dir.path().join("src")).unwrap();

    // Create src/utils.ts with an exported function
    std::fs::write(
        pkg_dir.path().join("src/utils.ts"),
        b"export function helper() { return 42; }\n",
    )
    .unwrap();

    // Create index.ts that re-exports via alias
    std::fs::write(
        pkg_dir.path().join("index.ts"),
        b"export * from '@/utils';\n",
    )
    .unwrap();

    // Create tsconfig.json with path aliases
    let tsconfig = serde_json::json!({
        "compilerOptions": {
            "baseUrl": ".",
            "paths": {
                "@/*": ["src/*"]
            }
        }
    });
    std::fs::write(
        pkg_dir.path().join("tsconfig.json"),
        serde_json::to_string_pretty(&tsconfig).unwrap(),
    )
    .unwrap();

    // Create package.json pointing to index.ts
    let pkg = serde_json::json!({
        "name": "pkg",
        "version": "1.0.0",
        "main": "./index.ts"
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let result = pledge::ecosystems::node::discovery::discover_exports(pkg_dir.path());
    assert!(
        result.is_ok(),
        "discover_exports should not error with tsconfig path aliases; got: {:?}",
        result.err()
    );

    // If tsconfig alias resolved, 'helper' should appear in the export map
    let (export_map, _warnings) = result.unwrap();
    assert!(
        export_map.contains_key(&ExportKey::new_root_value("helper")),
        "tsconfig alias @/utils should resolve to src/utils.ts and export 'helper'; got: {:?}",
        export_map.keys().collect::<Vec<_>>()
    );
}
