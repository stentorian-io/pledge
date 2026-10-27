//! Process-level contracts, with isolated npm/compiler stand-ins and no network.
#![cfg(unix)]

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

#[cfg(feature = "lsp-tests")]
#[path = "npm_cli.rs"]
mod npm_cli;

struct CliProject {
    directory: TempDir,
}

impl CliProject {
    fn new(old_source: &str, new_source: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir(root.join("bin")).unwrap();
        let compiler = root.join("node_modules/typescript/lib");
        fs::create_dir_all(&compiler).unwrap();
        fs::write(compiler.join("typescript.js"), "").unwrap();
        fs::write(compiler.join("tsserver.js"), "").unwrap();
        fs::write(root.join("package.json"), manifest("2.0.0").to_string()).unwrap();
        fs::write(root.join("index.js"), new_source).unwrap();
        tarball(&root.join("old.tgz"), "1.0.0", old_source);
        tarball(&root.join("new.tgz"), "2.0.0", new_source);
        executable(
            &root.join("bin/npm"),
            r##"#!/bin/sh
operation="$1"
shift
printf '%s\n' "$operation" "$@" >> "$NPM_TEST_LOG"
case "$operation" in
  view)
    if [ -n "$NPM_TEST_ERROR" ]; then
      printf '{"error":{"code":"%s"}}\n' "$NPM_TEST_ERROR"
      printf 'registry lookup failed: %s\n' "$NPM_TEST_ERROR" >&2
      exit 1
    fi
    printf '"1.0.0"\n'
    ;;
  pack)
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --pack-destination) shift; destination="$1" ;;
      esac
      specification="$1"
      shift
    done
    if [ "$NPM_TEST_BAD_FILENAME" = "1" ]; then
      printf '[{"filename":"../outside.tgz"}]\n'
      exit 0
    fi
    if [ "$specification" = "$NPM_TEST_PROJECT" ]; then
      /bin/cp "$NPM_TEST_CURRENT" "$destination/package.tgz"
    else
      /bin/cp "$NPM_TEST_BASELINE" "$destination/package.tgz"
    fi
    printf '[{"filename":"package.tgz"}]\n'
    ;;
  prefix) pwd ;;
  *) printf 'unexpected npm operation\n' >&2; exit 1 ;;
esac
"##,
        );
        executable(
            &root.join("bin/node"),
            r##"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf 'v22.12.0\n'
  exit 0
fi
/bin/cat > /dev/null
if [ "$NPM_TEST_COMPILER_FAIL" = "1" ]; then
  printf 'compiler fixture unavailable\n' >&2
  exit 1
fi
if /usr/bin/grep -q 'export const' index.js; then
  printf '{"results":[{"id":0,"overloads":[{"Primitive":"Number"}],"signature":"number","typeof_targets":[],"issues":[]}],"issues":[]}\n'
else
  printf '{"results":[],"issues":[]}\n'
fi
"##,
        );
        Self { directory }
    }

    fn command(&self) -> Command {
        let root = self.directory.path();
        let mut command = Command::new(env!("CARGO_BIN_EXE_interlint"));
        command
            .current_dir(root)
            .env("PATH", root.join("bin"))
            .env("NPM_TEST_LOG", root.join("npm.log"))
            .env("NPM_TEST_CURRENT", root.join("new.tgz"))
            .env("NPM_TEST_PROJECT", root.canonicalize().unwrap())
            .env("NPM_TEST_BASELINE", root.join("old.tgz"))
            .env_remove("NPM_TEST_ERROR")
            .env_remove("NPM_TEST_COMPILER_FAIL")
            .env_remove("NPM_TEST_BAD_FILENAME");
        command
    }

    fn json(&self) -> Command {
        let mut command = self.command();
        command.args(["--format", "json"]);
        command
    }
}

fn executable(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn manifest(version: &str) -> Value {
    json!({"name":"@ci/example", "version":version, "type":"module", "exports":"./index.js"})
}

fn tarball(path: &Path, version: &str, source: &str) {
    fs::write(path, tarball_bytes(&manifest(version), source)).unwrap();
}

fn tarball_bytes(manifest: &Value, source: &str) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for (path, contents) in [
        ("package/package.json", manifest.to_string()),
        ("package/index.js", source.to_owned()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, path, contents.as_bytes())
            .unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

fn report(output: &Output, exit: i32, status: &str) -> Value {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], status);
    assert_eq!(report["exit_code"], exit);
    assert!(!output.stdout.contains(&0x1b));
    report
}

#[test]
fn json_reports_breaking_changes_with_the_resolved_baseline() {
    let project = CliProject::new("export const api = 1;", "export {};");
    let output = project
        .json()
        .args(["--against", "next", "--color", "always"])
        .output()
        .unwrap();
    let report = report(&output, 1, "breaking");
    assert!(output.stderr.is_empty());
    assert_eq!(
        report["package"],
        json!({"name":"@ci/example", "current_version":"2.0.0", "baseline_version":"1.0.0"})
    );
    assert!(report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["rule"] == "export-removed" && finding["severity"] == "major"));
    assert!(report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|finding| !Path::new(finding["file"].as_str().unwrap()).is_absolute()));
    let log = fs::read_to_string(project.directory.path().join("npm.log")).unwrap();
    assert!(log.contains("@ci/example@next"));
    assert!(log.contains("@ci/example@1.0.0"));
    assert!(log.contains("--ignore-scripts"));
    assert!(log.contains("--workspaces=false"));
}

#[test]
fn clean_reports_are_stable_and_plain_when_redirected() {
    let project = CliProject::new("export const api = 1;", "export const api = 1;");
    let first = project.json().output().unwrap();
    let second = project.json().output().unwrap();
    let result = report(&first, 0, "clean");
    assert_eq!(result["summary"]["major"], 0);
    assert_eq!(first.stdout, second.stdout);
    let text = project.command().output().unwrap();
    assert!(text.status.success());
    assert!(!text.stdout.contains(&0x1b));
    assert!(String::from_utf8_lossy(&text.stdout).contains("No breaking changes found."));
}

#[test]
fn warning_findings_are_counted_without_failing_the_check() {
    let project = CliProject::new(
        "export const api = 1;",
        "/** @deprecated */\nexport const api = 1;",
    );
    let output = project.json().output().unwrap();
    let result = report(&output, 0, "clean");
    assert_eq!(result["summary"]["major"], 0);
    assert_eq!(result["summary"]["warning"], 1);
    assert!(result["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["rule"] == "export-deprecated"));
}

#[test]
fn incomplete_analysis_takes_precedence_over_breaking_findings() {
    let project = CliProject::new("export const api = 1;", "export {};");
    let output = project
        .json()
        .env("NPM_TEST_COMPILER_FAIL", "1")
        .output()
        .unwrap();
    let report = report(&output, 3, "incomplete");
    assert!(report["summary"]["major"].as_u64().unwrap() > 0);
    let warnings = report["warnings"].as_array().unwrap();
    assert!(warnings.iter().any(|warning| warning["message"]
        .as_str()
        .unwrap()
        .starts_with("baseline:")));
    assert!(warnings
        .iter()
        .any(|warning| warning["message"].as_str().unwrap().starts_with("current:")));
    assert!(warnings.iter().all(|warning| warning["incomplete"] == true));
}

#[test]
fn incomplete_analysis_never_prints_a_success_message() {
    let project = CliProject::new("export {};", "export {};");
    let output = project
        .command()
        .env("NPM_TEST_COMPILER_FAIL", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("compatibility could not be established"));
    assert!(!text.contains("No breaking changes found"));
}

#[test]
fn first_publish_is_explicit_and_missing_requested_baselines_fail() {
    let project = CliProject::new("export {};", "export const api = 1;");
    let output = project
        .json()
        .env("NPM_TEST_ERROR", "E404")
        .output()
        .unwrap();
    let result = report(&output, 0, "clean");
    assert!(result["package"]["baseline_version"].is_null());
    assert!(result["summary"]["minor"].as_u64().unwrap() > 0);
    let missing = project
        .json()
        .args(["--against", "latest"])
        .env("NPM_TEST_ERROR", "E404")
        .output()
        .unwrap();
    report(&missing, 2, "error");
    let unauthorized = project
        .json()
        .env("NPM_TEST_ERROR", "E401")
        .output()
        .unwrap();
    report(&unauthorized, 2, "error");
}

#[test]
fn invalid_input_and_pack_paths_produce_machine_readable_errors() {
    let project = CliProject::new("export {};", "export {};");
    let missing = project.json().arg("missing").output().unwrap();
    report(&missing, 2, "error");
    let invalid_baseline = project
        .json()
        .args(["--against", "file:../other"])
        .output()
        .unwrap();
    report(&invalid_baseline, 2, "error");
    let range = project.json().args(["--against", "1"]).output().unwrap();
    report(&range, 2, "error");
    let invalid_tarball = project
        .json()
        .env("NPM_TEST_BAD_FILENAME", "1")
        .output()
        .unwrap();
    report(&invalid_tarball, 2, "error");
    fs::write(project.directory.path().join("package.json"), "{").unwrap();
    let invalid_manifest = project.json().output().unwrap();
    let result = report(&invalid_manifest, 2, "error");
    assert!(result["error"]
        .as_str()
        .unwrap()
        .contains("Invalid package.json"));
}

#[test]
fn help_version_and_invalid_options_have_conventional_exit_codes() {
    let project = CliProject::new("export {};", "export {};");
    let help = project.command().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Exit codes:"));
    let version = project.command().arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("interlint "));
    let invalid = project.command().arg("--unknown").output().unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
}
