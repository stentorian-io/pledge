use pledge::core::findings::Severity;
use pledge::ecosystems::node::{compare::compare_exports_map_diff, discovery};
use pledge::languages::javascript::compare::{compare_exports, structure::compare_structural_diff};
use serde_json::{json, Value};

fn package(manifest: Value, files: &[(&str, &str)]) -> tempfile::TempDir {
    let package = tempfile::tempdir().unwrap();
    std::fs::write(package.path().join("package.json"), manifest.to_string()).unwrap();
    for (name, source) in files {
        let path = package.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    package
}

#[test]
fn type_version_ranges_and_subpaths_keep_independent_declarations() {
    let manifest = json!({"types": "index.d.ts", "typesVersions": {
        ">=5.0": {"*": ["v5.d.ts"], "feature": ["feature.d.ts"]},
        ">=4.0": {"*": ["v4.d.ts"]}
    }});
    let old = package(
        manifest.clone(),
        &[
            (
                "index.d.ts",
                "export declare function parse(x: string): string;",
            ),
            (
                "v5.d.ts",
                "export declare function parse(x: string): string;",
            ),
            (
                "v4.d.ts",
                "export declare function parse(x: number): string;",
            ),
            (
                "feature.d.ts",
                "export declare function parse(x: boolean): string;",
            ),
        ],
    );
    let new = package(
        manifest,
        &[
            (
                "index.d.ts",
                "export declare function parse(x: string): string;",
            ),
            (
                "v5.d.ts",
                "export declare function parse(x: number): string;",
            ),
            (
                "v4.d.ts",
                "export declare function parse(x: number): string;",
            ),
            (
                "feature.d.ts",
                "export declare function parse(x: boolean): string;",
            ),
        ],
    );
    let (before, warnings) = discovery::discover_exports(old.path()).unwrap();
    let (after, new_warnings) = discovery::discover_exports(new.path()).unwrap();
    assert!(warnings.is_empty() && new_warnings.is_empty());
    for (subpath, range, file) in [
        (".", ">=5.0", "v5.d.ts"),
        (".", ">=4.0", "v4.d.ts"),
        ("./feature", ">=5.0", "feature.d.ts"),
    ] {
        assert!(before.iter().any(|(key, symbol)| key.name == "parse"
            && key.subpath == subpath
            && key.condition.as_deref() == Some(&format!("typesVersions/{range}"))
            && symbol.file.ends_with(file)));
    }
    assert!(compare_structural_diff(old.path(), old.path(), &before, &before).is_empty());
    assert_eq!(before.len(), after.len());
    #[cfg(feature = "lsp-tests")]
    {
        use pledge::languages::typescript::compiler::{enrich_prepared_exports, EnrichResult};
        let (mut before, mut after) = (before, after);
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            assert!(matches!(
                enrich_prepared_exports(&mut before, old.path())
                    .await
                    .unwrap(),
                EnrichResult::Enriched
            ));
            assert!(matches!(
                enrich_prepared_exports(&mut after, new.path())
                    .await
                    .unwrap(),
                EnrichResult::Enriched
            ));
        });
        let changes = compare_exports(&before, &after, new.path(), true);
        assert!(
            changes
                .iter()
                .any(|finding| finding.rule == "param-type-narrowed"
                    && finding.severity == Severity::Major),
            "{changes:?}"
        );
    }
}

#[test]
fn reordering_overlapping_type_version_mappings_changes_the_selected_api() {
    use pledge::ecosystems::node::compare::metadata::compare_metadata;
    let old = package(
        json!({"typesVersions":{">=5":{"*":["v5.d.ts"]}, ">=4":{"*":["v4.d.ts"]}}}),
        &[],
    );
    let new = package(
        json!({"typesVersions":{">=4":{"*":["v4.d.ts"]}, ">=5":{"*":["v5.d.ts"]}}}),
        &[],
    );
    assert!(compare_metadata(old.path(), new.path())
        .iter()
        .any(|finding| finding.rule == "types-versions-changed"));
    let old = package(
        json!({"typesVersions":{">=5":{"*":["v5.d.ts"]}, "<5":{"*":["v4.d.ts"]}}}),
        &[],
    );
    let new = package(
        json!({"typesVersions":{"<5":{"*":["v4.d.ts"]}, ">=5":{"*":["v5.d.ts"]}}}),
        &[],
    );
    assert!(compare_metadata(old.path(), new.path()).is_empty());
}

#[test]
fn malformed_exports_objects_are_incomplete_instead_of_public_surfaces() {
    for exports in [
        json!({".":"./index.js", "import":"./index.js"}),
        json!({"0":"./index.js", "default":"./index.js"}),
        json!({".":{"1":"./index.js"}}),
    ] {
        let invalid = package(
            json!({"exports": exports}),
            &[("index.js", "exports.ok=1;")],
        );
        let valid = package(
            json!({"exports":"./index.js"}),
            &[("index.js", "exports.ok=1;")],
        );
        let (surface, warnings) = discovery::discover_exports(invalid.path()).unwrap();
        assert!(surface.is_empty());
        assert!(warnings
            .iter()
            .any(|warning| warning.incomplete && warning.message.contains("configuration")));
        assert!(compare_exports_map_diff(valid.path(), invalid.path())
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"));
    }
}

#[test]
fn deleting_a_wildcard_side_effect_module_removes_a_public_subpath() {
    let manifest = json!({"exports":{".":"./index.js", "./*":"./*.js"}});
    let old = package(
        manifest.clone(),
        &[
            ("index.js", "exports.ok=1;"),
            ("polyfill.js", "globalThis.registered=true;"),
        ],
    );
    let new = package(manifest, &[("index.js", "exports.ok=1;")]);
    let changes = compare_exports_map_diff(old.path(), new.path());
    assert!(
        changes
            .iter()
            .any(|finding| finding.rule == "subpath-removed"
                && finding.message.contains("./polyfill")
                && finding.severity == Severity::Major),
        "{changes:?}"
    );
    assert!(compare_exports_map_diff(old.path(), old.path()).is_empty());
}

#[test]
fn removing_require_access_is_breaking_even_without_exported_symbols() {
    let old = package(
        json!({"exports":"./index.js"}),
        &[("index.js", "globalThis.registered=true;")],
    );
    let restricted = package(
        json!({"exports":{"import":"./index.js"}}),
        &[("index.js", "globalThis.registered=true;")],
    );
    let equivalent = package(
        json!({"exports":{"default":"./index.js"}}),
        &[("index.js", "globalThis.registered=true;")],
    );
    assert!(compare_exports_map_diff(old.path(), restricted.path())
        .iter()
        .any(|finding| finding.severity == Severity::Major));
    assert!(!compare_exports_map_diff(old.path(), equivalent.path())
        .iter()
        .any(|finding| finding.severity == Severity::Major));
}

#[test]
fn deleting_a_null_blocker_does_not_remove_an_existing_api() {
    let old = package(
        json!({"exports":{".":"./index.js", "./blocked":null}}),
        &[("index.js", "exports.ok=1;")],
    );
    let new = package(
        json!({"exports":{".":"./index.js"}}),
        &[("index.js", "exports.ok=1;")],
    );
    assert!(!compare_exports_map_diff(old.path(), new.path())
        .iter()
        .any(|finding| finding.severity == Severity::Major));
}

#[test]
fn removing_exports_retains_legacy_main_and_reports_missing_aliases() {
    let old = package(
        json!({"main":"index.js", "exports":{".":"./index.js", "./alias":"./index.js"}}),
        &[("index.js", "exports.ok=1;")],
    );
    let new = package(json!({"main":"index.js"}), &[("index.js", "exports.ok=1;")]);
    let changes = compare_exports_map_diff(old.path(), new.path());
    assert_eq!(
        changes
            .iter()
            .filter(|finding| finding.severity == Severity::Major)
            .count(),
        1,
        "{changes:?}"
    );
    assert!(changes[0].message.contains("./alias"));
}

#[test]
fn declaration_fallbacks_do_not_preserve_removed_runtime_access() {
    for subpath in [".", "./feature"] {
        let old = package(
            json!({"types":"feature.d.ts", "exports":{subpath:"./feature.js"}}),
            &[
                ("feature.js", "globalThis.registered=true;"),
                ("feature.d.ts", "export {};"),
            ],
        );
        let new = package(
            json!({"types":"feature.d.ts"}),
            &[("feature.d.ts", "export {};")],
        );
        assert!(compare_exports_map_diff(old.path(), new.path())
            .iter()
            .any(|finding| {
                finding.rule == "subpath-removed" && finding.message.contains(subpath)
            }));
    }
    let declaration = package(
        json!({"types":"feature.d.ts", "exports":{"types":"./feature.d.ts"}}),
        &[("feature.d.ts", "export {};")],
    );
    let legacy = package(
        json!({"types":"feature.d.ts"}),
        &[("feature.d.ts", "export {};")],
    );
    assert!(compare_exports_map_diff(declaration.path(), legacy.path()).is_empty());
}

#[test]
fn removing_exports_preserves_each_loader_access_mode() {
    let files = [("feature.js", "globalThis.registered=true;")];
    let legacy = package(json!({}), &files);
    let bare = package(json!({"exports":{"./feature":"./feature.js"}}), &files);
    assert!(compare_exports_map_diff(bare.path(), legacy.path())
        .iter()
        .any(|finding| {
            finding.rule == "subpath-removed" && finding.message.contains("./feature")
        }));
    for exports in [
        json!({"./feature":{"require":"./feature.js"}}),
        json!({"./feature.js":"./feature.js"}),
    ] {
        let old = package(json!({"exports":exports}), &files);
        assert!(compare_exports_map_diff(old.path(), legacy.path()).is_empty());
    }
}

#[test]
fn export_urls_decode_names_without_accepting_encoded_traversal() {
    for target in ["./a%20b.js", "./a%20b.js?cache=1#module"] {
        let valid = package(json!({"exports":target}), &[("a b.js", "exports.ok=1;")]);
        let (surface, warnings) = discovery::discover_exports(valid.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(surface.keys().any(|key| key.name == "ok"));
    }
    let invalid = package(
        json!({"exports":"./%2e%2e/outside.js"}),
        &[("outside.js", "exports.ok=1;")],
    );
    assert!(discovery::discover_exports(invalid.path())
        .unwrap()
        .1
        .iter()
        .any(|warning| warning.incomplete));
}

#[test]
fn top_level_exports_null_uses_legacy_main_and_deep_imports() {
    let manifest = json!({"main":"index.js", "exports":null});
    let old = package(
        manifest.clone(),
        &[
            ("index.js", "exports.ok=1;"),
            ("deep.js", "exports.deep=1;"),
        ],
    );
    let new = package(
        manifest,
        &[
            ("index.js", "exports.other=1;"),
            ("deep.js", "exports.deep=1;"),
        ],
    );
    let (before, warnings) = discovery::discover_exports(old.path()).unwrap();
    let (after, new_warnings) = discovery::discover_exports(new.path()).unwrap();
    assert!(warnings.is_empty() && new_warnings.is_empty());
    assert!(before
        .keys()
        .any(|key| key.subpath == "./deep.js" && key.name == "deep"));
    assert!(compare_exports(&before, &after, new.path(), false)
        .iter()
        .any(|finding| finding.rule == "export-removed"));
    assert!(!compare_exports_map_diff(old.path(), old.path())
        .iter()
        .any(|finding| finding.severity == Severity::Major));
}

#[test]
fn legacy_main_resolves_extensions_directories_and_index_fallbacks() {
    for (manifest, runtime) in [
        (json!({"main":"./entry"}), "entry.js"),
        (json!({"main":"./dist"}), "dist/index.js"),
        (json!({"main":"./missing"}), "index.js"),
        (json!({"types":"types.d.ts"}), "index.js"),
    ] {
        let old = package(
            manifest.clone(),
            &[
                (runtime, "exports.answer=42;"),
                ("types.d.ts", "export {};"),
            ],
        );
        let new = package(
            manifest,
            &[
                (runtime, "exports.changed=42;"),
                ("types.d.ts", "export {};"),
            ],
        );

        let (before, old_warnings) = discovery::discover_exports(old.path()).unwrap();
        let (after, new_warnings) = discovery::discover_exports(new.path()).unwrap();
        assert!(
            old_warnings.is_empty() && new_warnings.is_empty(),
            "{runtime}: {old_warnings:?} {new_warnings:?}"
        );
        assert!(before
            .keys()
            .any(|key| key.subpath == "." && key.name == "answer"));
        let changes = compare_exports(&before, &after, new.path(), false);
        assert!(
            changes
                .iter()
                .any(|finding| finding.rule == "export-removed"
                    && finding.severity == Severity::Major)
        );
        assert!(compare_exports(&before, &before, old.path(), false).is_empty());

        #[cfg(feature = "lsp-tests")]
        for (root, succeeds) in [(old.path(), true), (new.path(), false)] {
            let output = std::process::Command::new("node")
                .args([
                    "--eval",
                    "require('node:assert/strict').equal(require('./').answer,42)",
                ])
                .current_dir(root)
                .output()
                .unwrap();
            assert_eq!(output.status.success(), succeeds, "{runtime}: {output:?}");
        }
    }
}
