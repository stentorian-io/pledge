#![cfg(feature = "lsp-tests")]

use pledge::core::findings::Severity;
use pledge::ecosystems::node::compare::compare_exports_map_diff;
use std::path::Path;
use std::process::Command;

fn package(index: &str, dependency: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"name":"loading-coverage","type":"module","exports":"./index.mjs"}"#,
    )
    .unwrap();
    std::fs::write(root.path().join("index.mjs"), index).unwrap();
    std::fs::write(root.path().join("dependency.mjs"), dependency).unwrap();
    root
}

fn require(root: &Path) -> std::process::Output {
    Command::new("node")
        .current_dir(root)
        .args(["--eval", "require('loading-coverage')"])
        .output()
        .unwrap()
}

#[test]
fn synchronous_loading_changes_agree_with_node_consumers() {
    for (index, dependency, breaks_require) in [
        ("await Promise.resolve(); export const api=1;", "", true),
        (
            "import './dependency.mjs'; export const api=1;",
            "await Promise.resolve();",
            true,
        ),
        (
            "export * from './dependency.mjs';",
            "await Promise.resolve(); export const api=1;",
            true,
        ),
        (
            "export async function api(){ return import('./dependency.mjs'); }",
            "await Promise.resolve();",
            false,
        ),
        (
            "export async function api(){ await Promise.resolve(); }",
            "",
            false,
        ),
    ] {
        let old = package("export const api=1;", "");
        let new = package(index, dependency);
        let findings = compare_exports_map_diff(old.path(), new.path());

        assert!(require(old.path()).status.success());
        let output = require(new.path());
        assert_eq!(output.status.success(), !breaks_require, "{index}");
        if breaks_require {
            assert!(String::from_utf8_lossy(&output.stderr).contains("ERR_REQUIRE_ASYNC_MODULE"));
        }
        assert_eq!(
            findings
                .iter()
                .any(|finding| finding.rule == "module-became-async"
                    && finding.severity == Severity::Major),
            breaks_require,
            "{index}: {findings:?}"
        );
        assert!(!findings
            .iter()
            .any(|finding| finding.rule == "analysis-incomplete"));
    }
}

#[test]
fn unresolved_static_imports_retain_incomplete_loading_evidence() {
    for specifier in ["./dependency", "./missing.mjs", "./directory"] {
        let old = package("export const api=1;", "");
        let new = package(&format!("import '{specifier}'; export const api=1;"), "");
        std::fs::create_dir(new.path().join("directory")).unwrap();
        std::fs::write(
            new.path().join("directory/index.mjs"),
            "export const extra=1;",
        )
        .unwrap();

        let output = require(new.path());
        assert!(!output.status.success(), "{specifier}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("ERR_MODULE_NOT_FOUND") || error.contains("ERR_UNSUPPORTED_DIR_IMPORT")
        );
        let findings = compare_exports_map_diff(old.path(), new.path());
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"
                    && finding.message.contains(specifier)),
            "{findings:?}"
        );
    }
}

#[test]
fn host_url_reexports_retain_missing_public_interface_evidence() {
    use pledge::ecosystems::node::discovery::discover_exports;

    let local = package("export const api=1;", "");
    let file_url = format!("file://{}", local.path().join("index.mjs").display());
    for (specifier, name) in [
        ("data:text/javascript,export const api=1", "api"),
        ("node:path", "basename"),
        (file_url.as_str(), "api"),
    ] {
        for source in [
            format!("export * from '{specifier}';"),
            format!("export {{{name} as api}} from '{specifier}';"),
        ] {
            let package = package(&source, "");
            let output = Command::new("node")
                .current_dir(package.path())
                .args(["--input-type=module", "--eval", "import * as api from 'loading-coverage'; if(!Object.keys(api).length)throw Error('empty');"])
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let (_, warnings) = discover_exports(package.path()).unwrap();
            assert!(
                warnings.iter().any(|warning| warning.incomplete),
                "{specifier}: {warnings:?}"
            );
        }
    }
}

#[test]
fn non_javascript_module_interfaces_retain_incomplete_evidence() {
    use pledge::ecosystems::node::discovery::discover_exports;

    let version = Command::new("node").arg("--version").output().unwrap();
    let major: u32 = String::from_utf8(version.stdout)
        .unwrap()
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    for (source, filename, bytes, flags) in [
        (
            "export {default as asset} from './asset.json' with {type:'json'};",
            "asset.json",
            b"{\"api\":1}".as_slice(),
            vec![],
        ),
        (
            "export {default as asset} from './asset.txt' with {type:'text'};",
            "asset.txt",
            b"public text".as_slice(),
            vec!["--experimental-import-text"],
        ),
        (
            "export * as asset from './asset.wasm';",
            "asset.wasm",
            b"\0asm\x01\0\0\0".as_slice(),
            vec!["--experimental-wasm-modules"],
        ),
        (
            "import source asset from './asset.wasm'; export {asset};",
            "asset.wasm",
            b"\0asm\x01\0\0\0".as_slice(),
            vec![],
        ),
    ] {
        let package = package(source, "");
        std::fs::write(package.path().join(filename), bytes).unwrap();
        let mut command = Command::new("node");
        command.current_dir(package.path()).args(&flags).args([
            "--input-type=module",
            "--eval",
            "import {asset} from 'loading-coverage'; if(asset===undefined)throw Error('missing');",
        ]);
        let output = command.output().unwrap();
        let version_supports = if filename == "asset.txt" || source.starts_with("import source") {
            major >= 26
        } else {
            true
        };
        assert_eq!(
            output.status.success(),
            version_supports,
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let (_, warnings) = discover_exports(package.path()).unwrap();
        assert!(
            warnings.iter().any(|warning| warning.incomplete),
            "{source}: {warnings:?}"
        );
    }
}

#[test]
fn static_url_dependencies_retain_incomplete_loading_evidence() {
    for specifier in [
        "data:text/javascript,await Promise.resolve()",
        "file:///pledge-missing.mjs",
    ] {
        let old = package("export const api=1;", "");
        let new = package(&format!("import '{specifier}'; export const api=1;"), "");
        let output = require(new.path());
        assert!(!output.status.success());
        let findings = compare_exports_map_diff(old.path(), new.path());
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "analysis-incomplete"
                    && finding.message.contains(specifier)),
            "{findings:?}"
        );
    }
}
