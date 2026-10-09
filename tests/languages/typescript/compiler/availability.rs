#![cfg(all(feature = "lsp-tests", unix))]

use interlint::{discover::discover_exports, lsp, types::shape::TypeShape};
use std::{path::PathBuf, process::Command};

#[tokio::test]
async fn compiler_enrichment_does_not_require_a_language_server() {
    const CHILD_ROOT: &str = "PLEDGE_COMPILER_ONLY_TEST_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let (mut exports, warnings) = discover_exports(&root).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let result = lsp::enrich_exports(&mut exports, &root).await.unwrap();
        assert!(matches!(result, lsp::EnrichResult::Enriched), "{result:?}");
        let api = exports
            .values()
            .find(|symbol| symbol.name == "api")
            .unwrap();
        assert!(
            matches!(
                api.satisfies_type,
                Some(TypeShape::Satisfies {
                    narrowed: Some(_),
                    ..
                })
            ),
            "{api:?}"
        );
        assert!(
            exports
                .typeof_targets
                .values()
                .any(|targets| targets.values().any(Option::is_some)),
            "{exports:?}"
        );
        assert!(
            !root.join("tsconfig.json").exists(),
            "compiler evaluation should not create package files"
        );
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let binaries = root.join("bin");
    std::fs::create_dir(&binaries).unwrap();
    let node = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    std::os::unix::fs::symlink(node.trim(), binaries.join("node")).unwrap();
    let compiler = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("tsc"))
        .find(|binary| binary.is_file())
        .unwrap();
    std::os::unix::fs::symlink(compiler.canonicalize().unwrap(), binaries.join("tsc")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"compiler-only","type":"module","exports":"./index.ts"}"#,
    )
    .unwrap();
    std::fs::write(root.join("index.ts"), "export const api={value:true} satisfies {value:boolean}; export type Dependency=typeof import('./dependency.js').value;").unwrap();
    std::fs::write(root.join("dependency.ts"), "export const value='ready';").unwrap();
    let qualified = concat!(
        module_path!(),
        "::compiler_enrichment_does_not_require_a_language_server"
    );
    let (_, test_name) = qualified.split_once("::").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_name, "--nocapture"])
        .env(CHILD_ROOT, root)
        .env("PATH", binaries)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
}
