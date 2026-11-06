//! Integration tests for: LSP Type Enrichment
//!
//! These tests require a real typescript-language-server to be available.
//! Run with: cargo test --features lsp-tests --test typescript compiler::enrichment
//! Standard `cargo test` skips these tests entirely.

#![cfg(feature = "lsp-tests")]

use crate::fixtures::fixture_path;
use pledge::core::findings::{Finding, Severity};
use pledge::ecosystems::node::discovery as discover;
use pledge::languages::javascript::compare;
use pledge::languages::typescript::compiler;
use std::process::Command;

#[tokio::test]
async fn unresolved_commonjs_symbol_keys_retain_missing_runtime_evidence() {
    for source in [
        "const key=String('key'); function create(){return {[Symbol.for(key)]:1}} module.exports=create();",
        "/** @returns {{[key:symbol]:number}} */ function create(){return {[Symbol.for('key')]:1}} module.exports=create();",
        "const source={[Symbol.for('key')]:1}; function create(){return {...source}} module.exports=create();",
        "function create(){return {get [Symbol.for('key')](){return 1}}} module.exports=create();",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"symbol-key-evidence","exports":"./index.cjs"}"#,
        )
        .unwrap();
        std::fs::write(root.join("index.cjs"), source).unwrap();
        let consumer = Command::new("node")
            .current_dir(root)
            .args([
                "-e",
                "if(require('./index.cjs')[Symbol.for('key')]!==1)throw Error('missing symbol');",
            ])
            .output()
            .unwrap();
        assert!(
            consumer.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&consumer.stderr)
        );

        let (mut exports, warnings) = discover::discover_exports(root).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let enrichment = compiler::enrich_exports(&mut exports, root).await.unwrap();

        assert!(
            matches!(enrichment, compiler::EnrichResult::Degraded(ref reason) if reason.contains("symbol")),
            "{source}: {enrichment:?}"
        );
        assert!(exports.keys().any(|key| key.name == "default"));
    }
}

#[tokio::test]
async fn unresolved_commonjs_factory_results_retain_missing_runtime_evidence() {
    for annotation in ["any", "unknown"] {
        for has_api in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path();
            std::fs::write(
                root.join("package.json"),
                r#"{"name":"factory-types","exports":"./index.cjs"}"#,
            )
            .unwrap();
            let result = if has_api { "{api:1}" } else { "{}" };
            std::fs::write(root.join("index.cjs"), format!("/** @returns {{{annotation}}} */ function create() {{ return {result}; }} module.exports = create();")).unwrap();
            let consumer = Command::new("node")
                .current_dir(root)
                .args([
                    "-e",
                    "if (require('./index.cjs').api !== 1) throw Error('missing api');",
                ])
                .output()
                .unwrap();
            assert_eq!(
                consumer.status.success(),
                has_api,
                "{}",
                String::from_utf8_lossy(&consumer.stderr)
            );
            let (mut exports, warnings) = discover::discover_exports(root).unwrap();
            assert!(warnings.is_empty(), "{warnings:?}");
            assert!(!exports.is_empty());
            let enrichment = compiler::enrich_exports(&mut exports, root).await.unwrap();
            assert!(
                matches!(enrichment, compiler::EnrichResult::Degraded(ref reason) if reason.contains("export result requires runtime evidence")),
                "{annotation}/{has_api}: {enrichment:?}"
            );
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"explicit-any","exports":"./index.cjs"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("index.cjs"),
        "/** @type {any} */ const result = 1; module.exports = result;",
    )
    .unwrap();
    let (mut exports, warnings) = discover::discover_exports(root).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!exports.is_empty());
    let enrichment = compiler::enrich_exports(&mut exports, root).await.unwrap();
    assert!(
        matches!(enrichment, compiler::EnrichResult::Enriched),
        "{enrichment:?}"
    );
}

#[tokio::test]
async fn namespace_default_members_retain_compiler_types() {
    for namespace in ["ns", "default"] {
        for callable in [false, true] {
            let mut snapshots = Vec::new();
            let mut directories = Vec::new();
            for constructor in ["Number", "String"] {
                let directory = tempfile::tempdir().unwrap();
                let root = directory.path();
                std::fs::write(
                    root.join("package.json"),
                    r#"{"name":"namespace-types","type":"module","exports":"./index.js"}"#,
                )
                .unwrap();
                std::fs::write(
                    root.join("index.js"),
                    format!("export * as {namespace} from './source.js';"),
                )
                .unwrap();
                let source = if callable {
                    format!("export default function api() {{ return {constructor}(1); }}")
                } else {
                    format!("const value = {constructor}(1); export default value;")
                };
                std::fs::write(root.join("source.js"), source).unwrap();
                let import = if namespace == "default" {
                    "import ns from './index.js';"
                } else {
                    "import {ns} from './index.js';"
                };
                let call = if callable { "()" } else { "" };
                std::fs::write(
                    root.join("consumer.ts"),
                    format!("{import} const value: number = ns.default{call};"),
                )
                .unwrap();
                let output = Command::new("tsc")
                    .current_dir(root)
                    .args([
                        "--noEmit",
                        "--strict",
                        "--allowJs",
                        "--checkJs",
                        "--skipLibCheck",
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
                    constructor == "Number",
                    "{namespace}/{callable}/{constructor}: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                let (mut exports, warnings) = discover::discover_exports(root).unwrap();
                assert!(warnings.is_empty(), "{warnings:?}");
                let enrichment = compiler::enrich_exports(&mut exports, root).await.unwrap();
                assert!(
                    matches!(enrichment, compiler::EnrichResult::Enriched),
                    "{namespace}/{callable}: {enrichment:?}"
                );
                assert!(exports
                    .values()
                    .any(|symbol| symbol.name == format!("{namespace}.default")
                        && !symbol.overloads.is_empty()));
                snapshots.push(exports);
                directories.push(directory);
            }
            let findings = compare::compare_exports(
                &snapshots[0],
                &snapshots[1],
                directories[1].path(),
                false,
            );
            assert!(
                pledge::core::findings::has_breaking_changes(&findings),
                "{namespace}/{callable}: {findings:?}"
            );
        }
    }
}

/// Run the full pipeline: discover -> enrich -> compare
async fn run_pipeline(old_dir: &str, new_dir: &str) -> (Vec<Finding>, bool) {
    let old_root = fixture_path(old_dir);
    let new_root = fixture_path(new_dir);

    let (mut old_exports, _) = discover::discover_exports(&old_root).unwrap();
    let (mut new_exports, _) = discover::discover_exports(&new_root).unwrap();

    // Enrich both (may degrade if LSP not available — test handles both)
    let old_result = compiler::enrich_exports(&mut old_exports, &old_root).await;
    let new_result = compiler::enrich_exports(&mut new_exports, &new_root).await;

    // Assert enrichment succeeded (these tests require LSP)
    assert!(
        matches!(old_result, Ok(compiler::EnrichResult::Enriched)),
        "LSP enrichment failed for old: {:?}",
        old_result
    );
    assert!(
        matches!(new_result, Ok(compiler::EnrichResult::Enriched)),
        "LSP enrichment failed for new: {:?}",
        new_result
    );

    let findings =
        compare::compare_exports(&old_exports, &new_exports, std::path::Path::new("."), true);
    let has_breaking = pledge::core::findings::has_breaking_changes(&findings);
    (findings, has_breaking)
}

/// Test: renamed type alias (Config -> Settings, same shape) does NOT produce false positive.
/// Validates STRUCT-01 (structural shape comparison) and STRUCT-02 (alias resolution).
///
/// configure() param type changes from Config to Settings but shapes are identical —
/// must not report a breaking change to configure().
#[tokio::test]
async fn renamed_type_alias_same_shape_no_false_positive() {
    let (findings, _) = run_pipeline(
        "typescript/types/public-fields/old",
        "typescript/types/public-fields/new",
    )
    .await;

    // Config was removed (different export name), Settings was added
    // Config and Settings expose the same parameter shape to configure().
    let configure_type_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("configure") && f.severity == Severity::Major)
        .collect();
    assert!(
        configure_type_findings.is_empty(),
        "configure() should not have breaking findings when parameter shapes are identical: {:?}",
        configure_type_findings
    );
}

/// Test: nested field removal (AppSettings.database.ssl removed) is detected.
/// Validates STRUCT-03 (recursive nested object comparison).
#[tokio::test]
async fn nested_field_removal_detected() {
    let (findings, has_breaking) = run_pipeline(
        "typescript/types/public-fields/old",
        "typescript/types/public-fields/new",
    )
    .await;

    // AppSettings.database.ssl was removed
    let ssl_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("ssl") || f.message.contains("database"))
        .collect();
    assert!(
        !ssl_findings.is_empty(),
        "Should detect removal of database.ssl field. All findings: {:?}",
        findings
    );
    assert!(has_breaking, "Nested field removal is a breaking change");
}

/// Test: field removal from User type (age removed) is detected.
/// Validates basic field-removed rule.
#[tokio::test]
async fn field_removal_detected() {
    let (findings, _) = run_pipeline(
        "typescript/types/public-fields/old",
        "typescript/types/public-fields/new",
    )
    .await;

    // User.age was removed
    let age_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("age") && f.rule == "field-removed")
        .collect();
    assert!(
        !age_findings.is_empty(),
        "Should detect removal of age field from User. All findings: {:?}",
        findings
    );
}

/// Test: type change (timeout: number -> string) is detected.
/// Validates type-incompatible rule for primitive type changes.
/// Note: rule name is "type-incompatible" (the canonical catch-all for primitive/cross-kind diffs).
#[tokio::test]
async fn type_change_detected() {
    let (findings, _) = run_pipeline(
        "typescript/types/public-fields/old",
        "typescript/types/public-fields/new",
    )
    .await;

    // timeout changed from number to string — emits type-incompatible per
    let timeout_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.message.contains("timeout") && f.rule == "type-incompatible")
        .collect();
    assert!(
        !timeout_findings.is_empty(),
        "Should detect timeout type change number->string. All findings: {:?}",
        findings
    );
}

/// Test: JSDoc-annotated JavaScript files produce type information via LSP.
/// Validates STRUCT-04 (LSP type resolution for JSDoc JavaScript).
/// Note: type change findings use "type-incompatible" or "param-type-narrowed" rules.
#[tokio::test]
async fn jsdoc_javascript_type_resolution() {
    let (findings, has_breaking) =
        run_pipeline("javascript/types/jsdoc/old", "javascript/types/jsdoc/new").await;

    // createUser param 'age' changed from number to string (param-type-narrowed or type-incompatible)
    // maxRetries changed from number to string (type-incompatible)
    let type_findings: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule == "type-incompatible"
                || f.rule == "field-type-changed"
                || f.rule == "param-type-narrowed"
        })
        .collect();
    assert!(
        !type_findings.is_empty(),
        "JSDoc JavaScript should yield type changes. All findings: {:?}",
        findings
    );
    assert!(has_breaking, "JSDoc type changes are breaking");
}

/// Test: SIG-01..05 signature changes detected with LSP enrichment.
/// Validates that all five signature change rules produce findings when type shapes are available.
#[tokio::test]
async fn sig_changes_detected_with_lsp() {
    let (findings, has_breaking) = run_pipeline(
        "typescript/functions/signatures/old",
        "typescript/functions/signatures/new",
    )
    .await;
    let rules: Vec<&str> = findings.iter().map(|f| f.rule).collect();

    // SIG-01: added required param (greet now takes 2 required params instead of 1)
    assert!(
        rules.contains(&"param-added"),
        "SIG-01: param-added not found in rules: {:?}",
        rules
    );

    // SIG-02: narrowed param type (process input: string | number -> string)
    assert!(
        rules.contains(&"param-type-narrowed"),
        "SIG-02: param-type-narrowed not found in rules: {:?}",
        rules
    );

    // SIG-03: widened return type (getData returns string -> string | number)
    assert!(
        rules.contains(&"return-type-widened"),
        "SIG-03: return-type-widened not found in rules: {:?}",
        rules
    );

    // SIG-04: optional param added (configure gets optional port?) — should be Minor
    // param-added is used for both required (Major) and optional (Minor) params
    let param_added_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "param-added")
        .collect();
    assert!(
        param_added_findings
            .iter()
            .any(|f| f.severity == Severity::Minor),
        "SIG-04: should have a Minor param-added finding for optional param, got: {:?}",
        param_added_findings
    );

    // SIG-05: param order changed (transform(a, b) -> transform(b, a))
    assert!(
        rules.contains(&"param-reordered"),
        "SIG-05: param-reordered not found in rules: {:?}",
        rules
    );

    // Overall: signature changes ARE breaking
    assert!(has_breaking, "SIG changes should produce breaking findings");
}

/// A workspace removed after discovery cannot produce a successful verdict.
#[tokio::test]
async fn missing_workspace_degrades_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_path_buf();
    std::fs::write(root.join("package.json"), r#"{"exports":"./index.ts"}"#).unwrap();
    std::fs::write(root.join("index.ts"), "export const api = 1;").unwrap();
    let (mut exports, _) = discover::discover_exports(&root).unwrap();
    assert!(!exports.is_empty());
    drop(directory);

    let result = compiler::enrich_exports(&mut exports, &root).await;
    assert!(
        matches!(result, Ok(compiler::EnrichResult::Degraded(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn export_assignment_resolves_a_function_merged_with_a_namespace() {
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    let mut surfaces = Vec::new();
    for (directory, input, consumer_succeeds) in
        [(&old, "string | number", true), (&new, "string", false)]
    {
        let root = directory.path();
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"callable-namespace","exports":{"types":"./index.d.cts"}}"#,
        )
        .unwrap();
        let declaration = format!(
            "declare namespace api {{ type Input = {input}; const version: string; }}\n\
             declare function api(input: api.Input): string;\n\
             export = api;\n"
        );
        std::fs::write(root.join("index.d.cts"), declaration).unwrap();
        std::fs::write(
            root.join("consumer.cts"),
            "import api = require('./index.cjs');\n\
             const result: string = api(123);\n\
             const version: string = api.version;\n",
        )
        .unwrap();
        let consumer = Command::new("tsc")
            .args([
                "--noEmit",
                "--strict",
                "--target",
                "ES2022",
                "--module",
                "NodeNext",
                "--moduleResolution",
                "NodeNext",
                "consumer.cts",
            ])
            .current_dir(root)
            .output()
            .unwrap();
        assert_eq!(
            consumer.status.success(),
            consumer_succeeds,
            "{}{}",
            String::from_utf8_lossy(&consumer.stdout),
            String::from_utf8_lossy(&consumer.stderr)
        );

        let (mut exports, warnings) = discover::discover_exports(root).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let result = compiler::enrich_prepared_exports(&mut exports, root)
            .await
            .unwrap();
        assert!(
            matches!(result, compiler::EnrichResult::Enriched),
            "{result:?}"
        );
        let api = exports
            .values()
            .find(|export| export.name == "default")
            .unwrap();
        assert!(!api.overloads.is_empty());
        surfaces.push(exports);
    }
    let unchanged = compare::compare_exports(&surfaces[0], &surfaces[0], old.path(), true);
    assert!(unchanged.is_empty(), "{unchanged:?}");
    let changed = compare::compare_exports(&surfaces[0], &surfaces[1], new.path(), true);
    assert!(
        changed
            .iter()
            .any(|finding| finding.rule == "type-incompatible"
                && finding.severity == Severity::Major),
        "{changed:?}"
    );
}
