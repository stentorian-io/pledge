use pledge::ecosystems::node::compare::compare_exports_map_diff;

fn package(exports: &str, index: &str, dependency: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        format!(r#"{{"type":"module","exports":{exports}}}"#),
    )
    .unwrap();
    std::fs::write(dir.path().join("index.mjs"), index).unwrap();
    std::fs::write(dir.path().join("dependency.mjs"), dependency).unwrap();
    dir
}

#[test]
fn top_level_await_breaks_synchronous_loading_through_local_dependencies() {
    for (index, dependency) in [
        ("await Promise.resolve(); export const api = 1;", ""),
        (
            "import './dependency.mjs'; export const api = 1;",
            "await Promise.resolve();",
        ),
        (
            "export * from './dependency.mjs'; export const api = 1;",
            "await Promise.resolve();",
        ),
        ("for await (const value of []) {} export const api = 1;", ""),
    ] {
        let old = package(r#""./index.mjs""#, "export const api = 1;", "");
        let new = package(r#""./index.mjs""#, index, dependency);
        let findings = compare_exports_map_diff(old.path(), new.path());
        assert!(
            findings.iter().any(|f| f.rule == "module-became-async"
                && f.severity == pledge::core::findings::Severity::Major),
            "{findings:?}"
        );
    }
}

#[test]
fn async_bodies_and_import_only_entries_do_not_break_require_consumers() {
    for (exports, new_index) in [
        (
            r#""./index.mjs""#,
            "export async function api() { await Promise.resolve(); }",
        ),
        (
            r#"{"import":"./index.mjs"}"#,
            "await Promise.resolve(); export const api = 1;",
        ),
        (
            r#""./index.mjs""#,
            "export const api = async () => { await Promise.resolve(); };",
        ),
        (
            r#""./index.mjs""#,
            "export class Api { async method() { await Promise.resolve(); } }",
        ),
    ] {
        let old = package(exports, "export const api = 1;", "");
        let new = package(exports, new_index, "");
        let findings = compare_exports_map_diff(old.path(), new.path());
        assert!(
            !findings.iter().any(|f| f.rule == "module-became-async"),
            "{findings:?}"
        );
    }
}

#[test]
fn already_asynchronous_modules_and_cycles_terminate_without_new_breaks() {
    let source = "import './dependency.mjs'; await Promise.resolve(); export const api = 1;";
    let old = package(r#""./index.mjs""#, source, "import './index.mjs';");
    let new = package(r#""./index.mjs""#, source, "import './index.mjs';");
    assert!(compare_exports_map_diff(old.path(), new.path()).is_empty());
}

#[test]
fn shared_cycles_retain_requirements_for_every_entry_and_refresh_between_comparisons() {
    let exports = r#"{"./first":"./index.mjs","./second":"./dependency.mjs"}"#;
    let old = package(
        exports,
        "import './dependency.mjs';",
        "import './index.mjs';",
    );
    let new = package(
        exports,
        "import './dependency.mjs'; await Promise.resolve();",
        "import './index.mjs';",
    );

    let findings = compare_exports_map_diff(old.path(), new.path());

    for subpath in ["./first", "./second"] {
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "module-became-async"
                    && finding.message.contains(&format!("`{subpath}`"))),
            "{findings:?}"
        );
    }
    std::fs::write(new.path().join("index.mjs"), "import './dependency.mjs';").unwrap();
    assert!(compare_exports_map_diff(old.path(), new.path()).is_empty());
}

#[cfg(feature = "lsp-tests")]
#[test]
fn node_consumer_confirms_the_require_regression() {
    let old = package(r#""./index.mjs""#, "export const api = 1;", "");
    let new = package(
        r#""./index.mjs""#,
        "await Promise.resolve(); export const api = 1;",
        "",
    );
    let run = |root: &std::path::Path| {
        std::process::Command::new("node")
            .args(["-e", "require(process.argv[1])"])
            .arg(root.join("index.mjs"))
            .output()
            .unwrap()
    };
    assert!(run(old.path()).status.success());
    let result = run(new.path());
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ERR_REQUIRE_ASYNC_MODULE"));
}

#[test]
fn runtime_directory_loading_uses_javascript_before_typescript_sources() {
    let manifest = r#"{"type":"module","exports":"./index.cjs"}"#;
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    for (package, runtime_source) in [
        (&old, "export const helper=1;"),
        (&new, "await Promise.resolve(); export const helper=1;"),
    ] {
        std::fs::create_dir(package.path().join("dep")).unwrap();
        std::fs::write(package.path().join("package.json"), manifest).unwrap();
        std::fs::write(
            package.path().join("index.cjs"),
            "require('./dep'); exports.ok=1;",
        )
        .unwrap();
        std::fs::write(
            package.path().join("dep/index.ts"),
            "export const helper=1;",
        )
        .unwrap();
        std::fs::write(package.path().join("dep/index.js"), runtime_source).unwrap();
    }
    let findings = compare_exports_map_diff(old.path(), new.path());
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "module-became-async"),
        "{findings:?}"
    );
    assert!(compare_exports_map_diff(old.path(), old.path()).is_empty());
}

#[test]
fn await_using_makes_the_module_graph_asynchronous() {
    let old = package(r#""./index.mjs""#, "export const api=1;", "");
    let asynchronous = package(
        r#""./index.mjs""#,
        "await using resource = { [Symbol.asyncDispose]() {} }; export const api=1;",
        "",
    );
    let synchronous = package(
        r#""./index.mjs""#,
        "using resource = { [Symbol.dispose]() {} }; export const api=1;",
        "",
    );
    assert!(compare_exports_map_diff(old.path(), asynchronous.path())
        .iter()
        .any(|finding| finding.rule == "module-became-async"));
    assert!(!compare_exports_map_diff(old.path(), synchronous.path())
        .iter()
        .any(|finding| finding.rule == "module-became-async"));
}
