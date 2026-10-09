use interlint::discover::discover_exports;
use serde_json::{json, Value};

fn package(manifest: Value, files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), manifest.to_string()).unwrap();
    for (path, contents) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    dir
}

#[test]
fn public_subpaths_remain_distinct_when_they_share_sources() {
    for (manifest, files) in [
        (
            json!({"exports":{".":"./index.js","./alias":"./index.js"}}),
            vec![("index.js", "export const api = 1;")],
        ),
        (
            json!({"exports":{".":"./a.js","./alias":"./b.js"}}),
            vec![
                ("a.js", "export * from './shared.js';"),
                ("b.js", "export * from './shared.js';"),
                ("shared.js", "export const api = 1;"),
            ],
        ),
    ] {
        let dir = package(manifest, &files);
        let (exports, warnings) = discover_exports(dir.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        for subpath in [".", "./alias"] {
            assert!(
                exports
                    .keys()
                    .any(|key| key.subpath == subpath && key.name == "api"),
                "{exports:?}"
            );
        }
    }
}

#[test]
fn nested_conditions_keep_their_full_identity() {
    let dir = package(
        json!({"exports":{"browser":{"import":"./browser.js"},"node":{"import":"./node.js"}}}),
        &[
            ("browser.js", "export const api = 1;"),
            ("node.js", "export const api = 2;"),
        ],
    );
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty());
    for condition in ["browser/import", "node/import"] {
        assert!(
            exports
                .keys()
                .any(|key| key.condition.as_deref() == Some(condition) && key.name == "api"),
            "{exports:?}"
        );
    }
}

#[test]
fn default_condition_hides_later_branches() {
    let dir = package(
        serde_json::from_str(r#"{"exports":{"default":"./empty.js","node":"./node.js"}}"#).unwrap(),
        &[
            ("empty.js", "export {};"),
            ("node.js", "export const api = 1;"),
        ],
    );
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(exports.is_empty(), "{exports:?}");
}

#[test]
fn wildcard_expansion_includes_nested_paths_and_respects_exclusions() {
    let dir = package(
        json!({"exports":{"./*":{"node":"./dist/*.js"},"./private/*":null,"./special":"./special.js"}}),
        &[
            ("dist/nested/tool.js", "export const api = 1;"),
            ("dist/private/hidden.js", "export const hidden = 1;"),
            ("dist/special.js", "export const wrong = 1;"),
            ("special.js", "export const correct = 1;"),
        ],
    );
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|k| k.subpath == "./nested/tool"
        && k.name == "api"
        && k.condition.as_deref() == Some("node")));
    assert!(exports
        .keys()
        .any(|k| k.subpath == "./special" && k.name == "correct"));
    assert!(!exports
        .keys()
        .any(|k| k.name == "hidden" || k.name == "wrong"));
}

#[test]
fn javascript_entry_points_include_declaration_companions() {
    for (runtime, declaration) in [
        ("index.js", "index.d.ts"),
        ("index.mjs", "index.d.mts"),
        ("index.cjs", "index.d.cts"),
    ] {
        let dir = package(
            json!({"exports":format!("./{runtime}")}),
            &[
                (runtime, "export const api = x => x;"),
                (
                    declaration,
                    "export declare function api(x: string): string;",
                ),
            ],
        );
        let (exports, warnings) = discover_exports(dir.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(
            exports.iter().any(|(k, s)| k.name == "api"
                && k.condition.as_deref() == Some("types")
                && s.file.ends_with(declaration)),
            "{exports:?}"
        );
    }
}

#[test]
fn packages_without_exports_include_deep_imports() {
    let dir = package(
        json!({"main":"index.js"}),
        &[
            ("index.js", "export const root = 1;"),
            ("lib/extra.js", "export const api = 1;"),
        ],
    );
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(exports
        .keys()
        .any(|k| k.subpath == "./lib/extra.js" && k.name == "api"));
}

#[test]
fn chunk_paths_remain_readable_and_parent_specifiers_resolve() {
    let dir = package(
        json!({"exports":"./dist/index.js"}),
        &[
            (
                "dist/index.js",
                "export * from './chunks/chunk-ABC12345.js';",
            ),
            (
                "dist/chunks/chunk-ABC12345.js",
                "export { api } from '../../impl.js';",
            ),
            ("impl.js", "export const api = 1;"),
        ],
    );
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.values().any(|s| s.name == "api"));
    assert!(
        exports.values().all(|s| dir.path().join(&s.file).is_file()),
        "{exports:?}"
    );
}

#[test]
fn missing_public_entry_points_report_incomplete_analysis() {
    let dir = package(json!({"exports":"./missing.js"}), &[]);
    let (_, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings
        .iter()
        .any(|w| w.incomplete && w.message.contains("missing.js")));
}

#[test]
fn declaration_barrels_prefer_declarations_over_existing_runtime_files() {
    for (entry, runtime, declaration) in [
        ("index.d.ts", "impl.js", "impl.d.ts"),
        ("index.d.mts", "impl.mjs", "impl.d.mts"),
        ("index.d.cts", "impl.cjs", "impl.d.cts"),
    ] {
        let directory = package(
            json!({"exports":{"types":format!("./{entry}"),"default":format!("./{runtime}")}}),
            &[
                (entry, &format!("export {{ api }} from './{runtime}';")),
                (runtime, "export const api = x => x;"),
                (declaration, "export declare function api(x: string): void;"),
            ],
        );
        let (exports, warnings) = discover_exports(directory.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let types = exports
            .iter()
            .find(|(key, _)| key.name == "api" && key.condition.as_deref() == Some("types"))
            .unwrap()
            .1;
        assert!(types.file.ends_with(declaration), "{types:?}");
        let runtime_symbol = exports
            .iter()
            .find(|(key, _)| key.name == "api" && key.condition.as_deref() == Some("default"))
            .unwrap()
            .1;
        assert!(runtime_symbol.file.ends_with(runtime), "{runtime_symbol:?}");
    }
}
