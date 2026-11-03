use pledge::ecosystems::node::discovery::discover_exports;
use pledge::languages::javascript::compare;
use serde_json::json;

fn package(wrapper: &str, production: &str, development: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (name, source) in [
        ("index.cjs", wrapper),
        ("production.cjs", production),
        ("development.cjs", development),
    ] {
        std::fs::write(directory.path().join(name), source).unwrap();
    }
    std::fs::write(
        directory.path().join("package.json"),
        json!({"name":"forwarding-test","exports":"./index.cjs"}).to_string(),
    )
    .unwrap();
    directory
}

const WRAPPER: &str = "if (process.env.NODE_ENV === 'production') { module.exports = require('./production.cjs'); } else { module.exports = require('./development.cjs'); }";

#[test]
fn conditional_commonjs_exports_preserve_each_environment() {
    let old = package(
        WRAPPER,
        "exports.api = function() {};",
        "exports.api = function() {};",
    );
    let new = package(
        WRAPPER,
        "exports.other = function() {};",
        "exports.api = function() {};",
    );
    let (before, warnings) = discover_exports(old.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let (after, warnings) = discover_exports(new.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(before.keys().filter(|key| key.name == "api").count(), 2);
    let findings = compare::compare_exports(&before, &after, new.path(), false);
    let removals: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule == "export-removed")
        .collect();
    assert_eq!(removals.len(), 1, "{findings:?}");
    assert!(
        removals[0].message.contains("NODE_ENV=production"),
        "{findings:?}"
    );
}

#[test]
fn direct_bound_and_spread_requires_forward_the_public_surface() {
    for wrapper in [
        "module.exports = require('./production.cjs');",
        "const implementation = require('./production.cjs'); module.exports = implementation;",
        "module.exports = { ...require('./production.cjs') };",
    ] {
        let directory = package(wrapper, "exports.api = 1; exports.default = 2;", "");
        let (exports, warnings) = discover_exports(directory.path()).unwrap();
        assert!(warnings.is_empty(), "{wrapper}: {warnings:?}");
        assert!(
            exports.keys().any(|key| key.name == "api"),
            "{wrapper}: {exports:?}"
        );
        assert!(exports
            .values()
            .filter(|symbol| symbol.name == "api")
            .all(|symbol| symbol.file.ends_with("production.cjs")));
        assert!(
            exports
                .values()
                .any(|symbol| symbol.name == "default" && symbol.file.ends_with("production.cjs")),
            "{wrapper}: {exports:?}"
        );
    }
}

#[cfg(feature = "lsp-tests")]
#[test]
fn node_consumers_confirm_branch_specific_removal() {
    let old = package(WRAPPER, "exports.api = () => 1;", "exports.api = () => 1;");
    let new = package(
        WRAPPER,
        "exports.other = () => 1;",
        "exports.api = () => 1;",
    );
    for (environment, succeeds) in [("production", false), ("development", true)] {
        for (directory, expected) in [(&old, true), (&new, succeeds)] {
            let output = std::process::Command::new("node")
                .env("NODE_ENV", environment)
                .args(["-e", "require(process.argv[1]).api()"])
                .arg(directory.path().join("index.cjs"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                expected,
                "{environment}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn installed_generated_packages_and_external_barrels_are_resolved() {
    let directory = package(
        "module.exports = { ...require('.generated/client/default') };",
        "",
        "",
    );
    let generated = directory.path().join("node_modules/.generated/client");
    std::fs::create_dir_all(&generated).unwrap();
    std::fs::write(
        generated.join("default.js"),
        "exports.Client = class Client {};",
    )
    .unwrap();
    std::fs::write(
        generated.join("default.d.ts"),
        "export declare class WrongRuntime {}",
    )
    .unwrap();
    let (exports, warnings) = discover_exports(directory.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|key| key.name == "Client"));
    assert!(!exports.keys().any(|key| key.name == "WrongRuntime"));
    std::fs::write(
        directory.path().join("index.cjs"),
        "export * from '.generated/client/default';",
    )
    .unwrap();
    let (exports, warnings) = discover_exports(directory.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|key| key.name == "Client"));
}

#[test]
fn equivalent_condition_rewrites_and_target_relocations_stay_compatible() {
    let old = package(WRAPPER, "exports.api = 1;", "exports.api = 1;");
    let new = package("if (process.env.NODE_ENV !== 'production') { module.exports = require('./development.cjs'); } else { module.exports = require('./renamed.cjs'); }", "", "exports.api = 1;");
    std::fs::write(new.path().join("renamed.cjs"), "exports.api = 1;").unwrap();
    let (before, _) = discover_exports(old.path()).unwrap();
    let (after, _) = discover_exports(new.path()).unwrap();
    assert!(!pledge::core::findings::has_breaking_changes(
        &compare::compare_exports(&before, &after, new.path(), false)
    ));
}

#[test]
fn explicit_object_properties_override_earlier_spreads() {
    let directory = package(
        "module.exports = { ...require('./production.cjs'), api: 2 };",
        "exports.api = 1;",
        "",
    );
    let (exports, warnings) = discover_exports(directory.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports
        .values()
        .any(|symbol| symbol.name == "api" && symbol.file.ends_with("index.cjs")));
}

#[cfg(feature = "lsp-tests")]
#[test]
fn published_react_exports_match_node_in_both_environments() {
    let tarball = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/node/reference/react/to.tgz"),
    )
    .unwrap();
    let (_directory, root) =
        pledge::ecosystems::node::acquire::extract_tgz_from_bytes(&tarball).unwrap();
    let (exports, _) = discover_exports(&root).unwrap();
    for (environment, branch) in [
        ("production", "NODE_ENV=production"),
        ("development", "NODE_ENV!=production"),
    ] {
        let output = std::process::Command::new("node")
            .env("NODE_ENV", environment)
            .args([
                "-e",
                "console.log(JSON.stringify(Object.keys(require(process.argv[1]))))",
            ])
            .arg(root.join("index.js"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: std::collections::BTreeSet<String> =
            serde_json::from_slice(&output.stdout).unwrap();
        let context = format!("default/cjs:{branch}");
        let discovered: std::collections::BTreeSet<_> = exports
            .keys()
            .filter(|key| key.subpath == "." && key.condition.as_deref() == Some(&context))
            .map(|key| key.name.clone())
            .collect();
        assert_eq!(discovered, actual, "{environment}");
    }
}

#[test]
fn later_assignments_replace_the_previous_commonjs_surface() {
    for wrapper in [
        "module.exports = require('./production.cjs'); module.exports = {};",
        "exports.api = () => 1; module.exports = {};",
        "let api = require('./production.cjs'); api = {}; module.exports = api;",
        "let api = require('./production.cjs'); api = require('./development.cjs'); module.exports = api;",
    ] {
        let directory = package(wrapper, "exports.api = () => 1;", "exports.other = 1;");

        let (exports, warnings) = discover_exports(directory.path()).unwrap();

        assert!(warnings.is_empty(), "{wrapper}: {warnings:?}");
        assert!(
            !exports.keys().any(|key| key.name == "api"),
            "{wrapper}: {exports:?}"
        );
    }
}

#[test]
fn rebinding_an_alias_after_forwarding_preserves_the_exported_object() {
    for wrapper in [
        "let api = require('./production.cjs'); module.exports = api; api = {};",
        "let api = require('./production.cjs'); const saved = api; api = {}; module.exports = saved;",
    ] {
        let directory = package(wrapper, "exports.api = () => 1;", "");

        let (exports, warnings) = discover_exports(directory.path()).unwrap();

        assert!(warnings.is_empty(), "{wrapper}: {warnings:?}");
        assert!(exports.keys().any(|key| key.name == "api"), "{wrapper}");
    }
}

#[test]
fn deleting_export_properties_reports_incomplete_evidence() {
    for wrapper in [
        "exports.api = () => 1; delete exports.api;",
        "module.exports = require('./production.cjs'); delete module.exports.api;",
        "const api = require('./production.cjs'); delete api.api; module.exports = api;",
    ] {
        let directory = package(wrapper, "exports.api = () => 1;", "");

        let (_, warnings) = discover_exports(directory.path()).unwrap();

        assert!(
            warnings.iter().any(|warning| warning.incomplete
                && !warning.type_expansion_only
                && warning.message.contains("CommonJS export mutation")),
            "{wrapper}: {warnings:?}"
        );
    }
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn node_consumers_confirm_commonjs_replacement_removes_the_api() {
    let old = package(
        "module.exports = require('./production.cjs');",
        "exports.api = () => 1;",
        "",
    );
    let new = package(
        "module.exports = require('./production.cjs'); module.exports = {};",
        "exports.api = () => 1;",
        "",
    );
    for (directory, succeeds) in [(&old, true), (&new, false)] {
        let output = std::process::Command::new("node")
            .args(["-e", "require(process.argv[1]).api()"])
            .arg(directory.path().join("index.cjs"))
            .output()
            .unwrap();
        assert_eq!(output.status.success(), succeeds);
    }
    let (mut before, _) = discover_exports(old.path()).unwrap();
    let (mut after, _) = discover_exports(new.path()).unwrap();
    for (exports, directory) in [(&mut before, old.path()), (&mut after, new.path())] {
        let result =
            pledge::languages::typescript::compiler::enrich_prepared_exports(exports, directory)
                .await
                .unwrap();
        assert!(
            matches!(
                result,
                pledge::languages::typescript::compiler::EnrichResult::Enriched
            ),
            "{result:?}"
        );
    }
    let findings = compare::compare_exports(&before, &after, new.path(), true);
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "export-removed" && finding.message.contains("api")),
        "{findings:?}"
    );
}

#[test]
fn unresolved_mutations_do_not_erase_the_known_surface() {
    for wrapper in [
        "module.exports = require('./production.cjs'); module.exports = factory();",
        "module.exports = require('./production.cjs'); while (condition) { module.exports = {}; }",
        "let api = require('./production.cjs'); if (condition) { api = {}; } module.exports = api;",
    ] {
        let directory = package(wrapper, "exports.api = () => 1;", "");
        let (exports, warnings) = discover_exports(directory.path()).unwrap();
        assert!(exports.keys().any(|key| key.name == "api"), "{wrapper}");
        assert!(
            warnings.iter().any(|warning| warning.incomplete),
            "{wrapper}: {warnings:?}"
        );
    }
    let directory = package(
        "module.exports = require('./production.cjs'); module.exports = module.exports;",
        "exports.api = () => 1;",
        "",
    );
    let (exports, warnings) = discover_exports(directory.path()).unwrap();
    assert!(exports.keys().any(|key| key.name == "api"));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn bundler_export_annotations_and_default_unwrapping_preserve_evidence() {
    let directory = package(
        "exports.own = 1; 0 && (module.exports = { ...require('./production.cjs') });",
        "exports.api = 1;",
        "",
    );
    let (exports, warnings) = discover_exports(directory.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|key| key.name == "own"));
    assert!(exports.keys().any(|key| key.name == "api"));

    let directory = package(
        "exports.default = function api() {}; module.exports = exports.default;",
        "",
        "",
    );
    let (exports, warnings) = discover_exports(directory.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(exports.keys().any(|key| key.name == "default"));
}

#[test]
fn uncertain_alias_rebinding_keeps_later_forwarding_incomplete() {
    for wrapper in [
        "let api = require('./production.cjs'); while (false) { api = require('./development.cjs'); } module.exports = api;",
        "let api = require('./production.cjs'); while (false) { api = {}; } module.exports = api;",
        "let api = require('./production.cjs'); while (false) { api = {}; } const saved = api; module.exports = saved;",
        "let api = {}; if (false) { api = require('./production.cjs'); } module.exports = api;",
        "let api = require('./production.cjs'); api ||= {}; module.exports = api;",
        "var api = require('./production.cjs'); while (false) { var api = require('./development.cjs'); } module.exports = api;",
    ] {
        let directory = package(wrapper, "exports.api = 1;", "exports.other = 1;");
        let (_, warnings) = discover_exports(directory.path()).unwrap();
        assert!(warnings.iter().any(|warning| warning.incomplete), "{wrapper}: {warnings:?}");
    }
}

#[test]
fn writes_through_detached_exports_keep_analysis_incomplete() {
    for wrapper in [
        "module.exports = require('./production.cjs'); exports.extra = 1;",
        "module.exports = require('./production.cjs'); exports['extra'] = 1;",
        "module.exports = require('./production.cjs'); if (false) { exports = module.exports; } exports.extra = 1;",
        "module.exports = require('./production.cjs'); exports ||= module.exports; exports.extra = 1;",
    ] {
        let directory = package(wrapper, "exports.api = 1;", "");
        let (_, warnings) = discover_exports(directory.path()).unwrap();
        assert!(warnings.iter().any(|warning| warning.incomplete), "{wrapper}: {warnings:?}");
    }
}

#[test]
fn later_alias_rebinding_and_explicit_exports_reattachment_preserve_known_identity() {
    for wrapper in [
        "let api = require('./production.cjs'); module.exports = api; while (false) { api = {}; }",
        "let api = require('./production.cjs'); module.exports = api; if (false) { api = require('./development.cjs'); }",
        "module.exports = require('./production.cjs'); exports = module.exports; exports.extra = 1;",
        "let api = require('./production.cjs'); 0 && (api = {}); module.exports = api;",
    ] {
        let directory = package(wrapper, "exports.api = 1;", "exports.other = 1;");
        let (exports, warnings) = discover_exports(directory.path()).unwrap();
        assert!(warnings.is_empty(), "{wrapper}: {warnings:?}");
        assert!(exports.keys().any(|key| key.name == "api"), "{wrapper}: {exports:?}");
    }
}

#[cfg(feature = "lsp-tests")]
#[tokio::test]
async fn native_commonjs_alias_identity_matches_analysis_or_retains_incomplete_evidence() {
    use pledge::languages::typescript::compiler;
    use std::process::Command;

    for (wrapper, names, incomplete) in [
        ("module.exports = require('./production.cjs'); exports.extra = 1;", vec!["api"], true),
        ("let api = require('./production.cjs'); while (false) { api = require('./development.cjs'); } module.exports = api;", vec!["api"], true),
        ("let api = require('./production.cjs'); while (false) { api = {}; } const saved = api; module.exports = saved;", vec!["api"], true),
        ("let api = {}; if (false) { api = require('./production.cjs'); } module.exports = api;", vec![], true),
        ("let api = require('./production.cjs'); api ||= {}; module.exports = api;", vec!["api"], true),
        ("var api = require('./production.cjs'); while (false) { var api = require('./development.cjs'); } module.exports = api;", vec!["api"], true),
        ("let api = require('./production.cjs'); module.exports = api; while (false) { api = {}; }", vec!["api"], false),
        ("module.exports = require('./production.cjs'); exports = module.exports; exports.extra = 1;", vec!["api", "extra"], false),
        ("let api = require('./production.cjs'); 0 && (api = {}); module.exports = api;", vec!["api"], false),
    ] {
        let directory = package(wrapper, "exports.api = 1;", "exports.other = 1;");
        let output = Command::new("node").current_dir(directory.path())
            .args(["-e", "process.stdout.write(JSON.stringify(Object.keys(require('./index.cjs')).sort()))"])
            .output().unwrap();
        assert!(output.status.success(), "{wrapper}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(serde_json::from_slice::<Vec<String>>(&output.stdout).unwrap(), names, "{wrapper}");

        let (mut exports, mut warnings) = discover_exports(directory.path()).unwrap();
        let result = compiler::enrich_exports(&mut exports, directory.path()).await.unwrap();
        assert!(matches!(result, compiler::EnrichResult::Enriched), "{wrapper}: {result:?}");
        result.reconcile_discovery_warnings(&mut warnings);
        assert_eq!(warnings.iter().any(|warning| warning.incomplete), incomplete, "{wrapper}: {warnings:?}");
        if !incomplete {
            let mut discovered: Vec<_> = exports.keys().map(|key| key.name.as_str()).collect();
            discovered.sort_unstable();
            assert_eq!(discovered, names, "{wrapper}");
        }
    }
}

#[test]
fn known_environment_alias_rebinding_preserves_branch_identity() {
    let directory = package(
        "let api = require('./production.cjs'); if (process.env.NODE_ENV === 'development') { api = require('./development.cjs'); module.exports = api; } else { module.exports = api; }",
        "exports.api = 1;", "exports.other = 1;",
    );
    let (exports, warnings) = discover_exports(directory.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        exports.keys().any(|key| key.name == "api"
            && key.condition.as_deref() == Some("cjs:NODE_ENV!=development")),
        "{exports:?}"
    );
    assert!(
        exports.keys().any(|key| key.name == "other"
            && key.condition.as_deref() == Some("cjs:NODE_ENV=development")),
        "{exports:?}"
    );
}
