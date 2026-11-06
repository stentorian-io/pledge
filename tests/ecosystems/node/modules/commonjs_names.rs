#![cfg(feature = "lsp-tests")]

use pledge::core::findings::Severity;
use pledge::ecosystems::node::compare::compare_commonjs_named_imports;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

fn package(exports: Value, source: &str, dependency: Option<&str>) -> tempfile::TempDir {
    let package = tempfile::tempdir().unwrap();
    std::fs::write(
        package.path().join("package.json"),
        json!({
            "name":"named-import-probe", "exports":exports,
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(package.path().join("index.cjs"), source).unwrap();
    if let Some(dependency) = dependency {
        std::fs::write(package.path().join("dependency.cjs"), dependency).unwrap();
    }
    package
}

fn consumer(root: &Path, mode: &str, conditions: &[&str]) -> bool {
    let mut command = Command::new("node");
    for condition in conditions {
        command.arg(format!("--conditions={condition}"));
    }
    if mode == "import" {
        command.args([
            "--input-type=module",
            "--eval",
            "import {api} from 'named-import-probe'; if(api!==1)throw Error('missing API');",
        ]);
    } else {
        command.args([
            "--eval",
            "require('node:assert/strict').equal(require('named-import-probe').api,1)",
        ]);
    }
    command.current_dir(root).output().unwrap().status.success()
}

#[tokio::test]
async fn commonjs_object_rewrites_preserve_require_but_can_remove_named_imports() {
    for (old_source, new_source, breaking) in [
        (
            "exports.api=Number(1);",
            "function create(){return{api:Number(1)}}module.exports=create();",
            true,
        ),
        (
            "module.exports={api:Number(1)};",
            "function create(){return{api:Number(1)}}module.exports=create();",
            true,
        ),
        (
            "exports.api=Number(1);",
            r"\u0065xports.api=Number(1);",
            true,
        ),
        (
            "module.exports.api=Number(1);",
            r"\u006dodule.exports.api=Number(1);",
            true,
        ),
        (
            "module.exports.api=Number(1);",
            r"module.\u0065xports.api=Number(1);",
            true,
        ),
        (
            "exports.api=Number(1);",
            "exports . api = Number(1);",
            false,
        ),
        (
            "module.exports={api:Number(1)};",
            "module.exports = { api: Number(1) };",
            false,
        ),
    ] {
        let old = package(json!("./index.cjs"), old_source, None);
        let new = package(json!("./index.cjs"), new_source, None);

        let findings = compare_commonjs_named_imports(old.path(), new.path()).await;

        assert!(findings.iter().all(|finding| finding.file.is_relative()));
        assert!(
            !findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{findings:?}"
        );
        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.rule == "cjs-named-export-removed"
                    && finding.severity == Severity::Major),
            breaking,
            "{new_source}: {findings:?}"
        );
        assert!(consumer(old.path(), "require", &[]) && consumer(new.path(), "require", &[]));
        assert!(consumer(old.path(), "import", &[]));
        assert_eq!(consumer(new.path(), "import", &[]), !breaking);
    }
}

#[tokio::test]
async fn import_conditions_and_local_reexports_select_the_actual_node_surface() {
    for exports in [
        json!({"require":"./index.cjs"}),
        json!({"types":"./index.cjs"}),
    ] {
        let old = package(exports.clone(), "exports.api=1;", None);
        let new = package(exports, r"\u0065xports.api=1;", None);
        assert!(compare_commonjs_named_imports(old.path(), new.path())
            .await
            .is_empty());
        assert!(!consumer(old.path(), "import", &[]) && !consumer(new.path(), "import", &[]));
    }
    let old = package(
        json!("./index.cjs"),
        "module.exports=require('./dependency.cjs');",
        Some("exports.api=1;"),
    );
    let new = package(
        json!("./index.cjs"),
        "module.exports=require('./dependency.cjs');",
        Some(r"\u0065xports.api=1;"),
    );
    let findings = compare_commonjs_named_imports(old.path(), new.path()).await;
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "cjs-named-export-removed"),
        "{findings:?}"
    );
    assert!(consumer(old.path(), "import", &[]) && !consumer(new.path(), "import", &[]));

    for (exports, selected) in [
        (
            json!({"browser":"./dependency.cjs", "default":"./index.cjs"}),
            true,
        ),
        (
            json!({"node":"./index.cjs", "browser":"./dependency.cjs"}),
            false,
        ),
    ] {
        let old = package(exports.clone(), "exports.api=1;", Some("exports.api=1;"));
        let new = package(exports, "exports.api=1;", Some(r"\u0065xports.api=1;"));
        let findings = compare_commonjs_named_imports(old.path(), new.path()).await;
        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.rule == "cjs-named-export-removed"),
            selected,
            "{findings:?}"
        );
        assert!(consumer(old.path(), "import", &["browser"]));
        assert_eq!(consumer(new.path(), "import", &["browser"]), !selected);
        assert!(consumer(old.path(), "import", &[]) && consumer(new.path(), "import", &[]));
    }
}

#[tokio::test]
async fn condition_wrappers_and_removed_branches_preserve_consumer_context() {
    for (old_exports, new_exports, old_source, new_source, conditions, breaking) in [
        (
            json!("./index.cjs"),
            json!({"default":"./index.cjs"}),
            "exports.api=1;",
            r"\u0065xports.api=1;",
            vec![],
            true,
        ),
        (
            json!({"default":"./index.cjs"}),
            json!("./index.cjs"),
            "exports.api=1;",
            "exports . api=1;",
            vec![],
            false,
        ),
        (
            json!({"browser":"./dependency.cjs", "default":"./index.cjs"}),
            json!("./index.cjs"),
            r"\u0065xports.api=1;",
            r"\u0065xports.api=1;",
            vec!["browser"],
            true,
        ),
    ] {
        let old = package(old_exports, old_source, Some("exports.api=1;"));
        let new = package(new_exports, new_source, Some("exports.api=1;"));

        let findings = compare_commonjs_named_imports(old.path(), new.path()).await;

        assert!(
            !findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{findings:?}"
        );
        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.rule == "cjs-named-export-removed"),
            breaking,
            "{findings:?}"
        );
        assert!(consumer(old.path(), "import", &conditions));
        assert_eq!(consumer(new.path(), "import", &conditions), !breaking);
    }
}

#[tokio::test]
async fn typeless_javascript_uses_the_installed_node_module_classifier() {
    let old = package(json!("./index.js"), "", None);
    let new = package(json!("./index.js"), "", None);
    for (package, source) in [
        (&old, "export const api=1;"),
        (&new, "const api=1; export {api};"),
    ] {
        std::fs::write(package.path().join("index.js"), source).unwrap();
    }

    let findings = compare_commonjs_named_imports(old.path(), new.path()).await;

    assert!(findings.is_empty(), "{findings:?}");
    assert!(consumer(old.path(), "import", &[]) && consumer(new.path(), "import", &[]));
}

#[tokio::test]
async fn combined_custom_conditions_retain_node_branch_precedence() {
    let old = package(
        json!({"browser":"./index.cjs", "custom":"./dependency.cjs", "default":"./dependency.cjs"}),
        "exports.api=1;",
        Some(r"\u0065xports.api=1;"),
    );
    let new = package(
        json!({"custom":"./dependency.cjs", "browser":"./index.cjs", "default":"./dependency.cjs"}),
        "exports.api=1;",
        Some(r"\u0065xports.api=1;"),
    );

    let findings = compare_commonjs_named_imports(old.path(), new.path()).await;

    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "cjs-named-export-removed"),
        "{findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"),
        "{findings:?}"
    );
    assert!(consumer(old.path(), "import", &["browser", "custom"]));
    assert!(!consumer(new.path(), "import", &["browser", "custom"]));
    for conditions in [vec![], vec!["browser"], vec!["custom"]] {
        assert_eq!(
            consumer(old.path(), "import", &conditions),
            consumer(new.path(), "import", &conditions)
        );
    }
}

#[tokio::test]
async fn commonjs_evidence_does_not_execute_package_code_and_reports_limits() {
    let old = package(
        json!("./index.cjs"),
        "exports.api=1;require('node:fs').writeFileSync(__dirname+'/executed','yes');",
        None,
    );
    let new = package(
        json!("./index.cjs"),
        "exports.other=1;throw Error('must not execute');",
        None,
    );

    let findings = compare_commonjs_named_imports(old.path(), new.path()).await;

    assert!(!old.path().join("executed").exists());
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "cjs-named-export-removed"),
        "{findings:?}"
    );
    for source in [
        "module.exports=require('./missing.cjs');".to_string(),
        " ".repeat(pledge::core::source::MAX_SOURCE_BYTES + 1),
    ] {
        std::fs::write(new.path().join("index.cjs"), source).unwrap();
        let findings = compare_commonjs_named_imports(old.path(), new.path()).await;
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"),
            "{findings:?}"
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.severity == Severity::Major),
            "{findings:?}"
        );
    }
}
