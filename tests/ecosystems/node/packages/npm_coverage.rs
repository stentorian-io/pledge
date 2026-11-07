//! Manifest contracts use isolated local npm consumers, including loopback HTTP.
use pledge::ecosystems::node::compare::metadata::compare_metadata;
use serde_json::{json, Value};
use std::path::Path;

fn package(manifest: Value, files: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("package.json"), manifest.to_string()).unwrap();
    for (name, contents) in files {
        let path = directory.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    directory
}

fn manifest(extra: Value) -> Value {
    let mut manifest = json!({"name":"manifest-contract", "version":"1.0.0", "main":"index.js"});
    manifest
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    manifest
}

fn has_rule(old: &Path, new: &Path, rule: &str) -> bool {
    compare_metadata(old, new)
        .iter()
        .any(|finding| finding.rule == rule)
}

#[test]
fn ambiguous_directory_commands_are_incomplete() {
    let old = package(
        manifest(json!({"directories":{"bin":"commands"}})),
        &[("commands/a/tool", ""), ("commands/b/tool", "")],
    );
    let new = package(manifest(json!({"bin":{"renamed":"commands/a/tool"}})), &[]);
    let findings = compare_metadata(old.path(), new.path());
    assert!(findings
        .iter()
        .any(|finding| finding.rule == "analysis-incomplete"));
    assert!(!findings
        .iter()
        .any(|finding| finding.rule == "bin-entry-changed"));
}

#[test]
fn empty_bin_directories_do_not_expose_package_files_as_commands() {
    let old = package(
        manifest(json!({"directories":{"bin":""}})),
        &[("index.js", "")],
    );
    let new = package(manifest(json!({})), &[]);
    assert!(!has_rule(old.path(), new.path(), "bin-entry-changed"));
}

#[cfg(unix)]
#[test]
fn unpacked_directory_symlinks_do_not_establish_commands() {
    let old = package(
        manifest(json!({"directories":{"bin":"commands"}})),
        &[("commands/tool", "")],
    );
    std::os::unix::fs::symlink("tool", old.path().join("commands/alias")).unwrap();
    assert!(has_rule(old.path(), old.path(), "analysis-incomplete"));
}

#[cfg(unix)]
#[test]
fn directory_bins_outside_the_package_are_incomplete() {
    let outside = package(json!({}), &[("tool", "")]);
    let old = package(manifest(json!({"directories":{"bin":"commands"}})), &[]);
    std::os::unix::fs::symlink(outside.path(), old.path().join("commands")).unwrap();
    let new = package(manifest(json!({})), &[]);
    let findings = compare_metadata(old.path(), new.path());
    assert!(findings
        .iter()
        .any(|finding| finding.rule == "analysis-incomplete"));
    assert!(!findings
        .iter()
        .any(|finding| finding.rule == "bin-entry-changed"));
}

#[cfg(feature = "lsp-tests")]
mod consumers {
    use super::*;
    use std::process::{Command, Output};

    fn npm(root: &Path, arguments: &[&str]) -> Output {
        let command_state = tempfile::Builder::new()
            .prefix("pledge-npm-command-")
            .tempdir()
            .unwrap();
        let output = Command::new("npm").arg("--version").output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "11.19.0",
            "manifest oracle requires pinned npm 11.19.0"
        );
        Command::new("npm")
            .args(arguments)
            .args([
                "--offline",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
                "--package-lock=false",
            ])
            .env("npm_config_cache", command_state.path().join("cache"))
            .env(
                "npm_config_userconfig",
                command_state.path().join("empty.npmrc"),
            )
            .env("npm_config_update_notifier", "false")
            .current_dir(root)
            .output()
            .unwrap()
    }

    fn configure_preparation(source: &Path, settings: &str) -> tempfile::TempDir {
        let preparation_state = tempfile::Builder::new()
            .prefix("pledge-npm-preparation-")
            .tempdir()
            .unwrap();
        std::fs::write(
            source.join(".npmrc"),
            format!(
                "{settings}cache={}\nupdate-notifier=false\n",
                preparation_state.path().join("cache").display()
            ),
        )
        .unwrap();
        preparation_state
    }

    fn packed(source: &Path) -> tempfile::TempDir {
        let packed = tempfile::tempdir().unwrap();
        let output = npm(
            source,
            &[
                "pack",
                "--json",
                "--pack-destination",
                packed.path().to_str().unwrap(),
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        let archive =
            std::fs::File::open(packed.path().join(result[0]["filename"].as_str().unwrap()))
                .unwrap();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap();
            assert!(
                !path.components().any(|component| matches!(
                    component.as_os_str().to_str(),
                    Some("cache" | "_logs" | "_cacache" | ".npmrc" | "empty.npmrc")
                )),
                "npm command state leaked into packed fixture: {}",
                path.display()
            );
            entry.unpack_in(packed.path()).unwrap();
        }
        packed
    }

    fn installed(source: &Path, extra: &[&str]) -> (tempfile::TempDir, Output) {
        let consumer = package(json!({"name":"consumer", "version":"1.0.0"}), &[]);
        let mut arguments = vec!["install", source.to_str().unwrap(), "--install-links"];
        arguments.extend(extra.iter().copied());
        let output = npm(consumer.path(), &arguments);
        (consumer, output)
    }

    fn assert_success(output: &Output, expected: bool) {
        assert_eq!(
            output.status.success(),
            expected,
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn archive(root: &Path) -> std::path::PathBuf {
        std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|extension| extension == "tgz"))
            .unwrap()
    }

    #[test]
    fn npm_peer_conflicts_agree_with_narrowed_requirements() {
        let host = package(json!({"name":"manifest-host", "version":"1.0.0"}), &[]);
        let host = packed(host.path());
        let old = package(
            manifest(json!({"peerDependencies":{"manifest-host":"^1"}})),
            &[],
        );
        for (range, breaks) in [("^2", true), (">=1 <3", false)] {
            let new = package(
                manifest(json!({"peerDependencies":{"manifest-host":range}})),
                &[],
            );
            assert_eq!(
                has_rule(old.path(), new.path(), "peer-dep-version-bumped"),
                breaks
            );
            for (root, expected) in [(old.path(), true), (new.path(), !breaks)] {
                let plugin = packed(root);
                let consumer = package(json!({"name":"consumer", "version":"1.0.0"}), &[]);
                let output = npm(
                    consumer.path(),
                    &[
                        "install",
                        archive(host.path()).to_str().unwrap(),
                        archive(plugin.path()).to_str().unwrap(),
                        "--strict-peer-deps",
                    ],
                );
                assert_success(&output, expected);
                if !expected {
                    assert!(String::from_utf8_lossy(&output.stderr).contains("ERESOLVE"));
                }
            }
        }
    }

    #[test]
    fn npm_optional_peer_absence_agrees_with_required_transition() {
        let old = package(
            manifest(
                json!({"peerDependencies":{"manifest-host":"^1"}, "peerDependenciesMeta":{"manifest-host":{"optional":true}}}),
            ),
            &[],
        );
        for (extra, breaks) in [
            (json!({"peerDependencies":{"manifest-host":"^1"}}), true),
            (
                json!({"peerDependencies":{"manifest-host":">=1 <3"}, "peerDependenciesMeta":{"manifest-host":{"optional":true}}}),
                false,
            ),
        ] {
            let new = package(manifest(extra), &[]);
            assert_eq!(
                has_rule(old.path(), new.path(), "peer-dep-optional-removed"),
                breaks
            );
            for (root, expected) in [(old.path(), true), (new.path(), !breaks)] {
                let (_, output) = installed(root, &[]);
                assert_success(&output, expected);
                if !expected {
                    assert!(String::from_utf8_lossy(&output.stderr).contains("ENOTCACHED"));
                }
            }
        }
    }

    #[test]
    fn browser_string_entry_exports_agree_with_direct_node_consumers() {
        use pledge::ecosystems::node::discovery::discover_exports;
        use pledge::languages::javascript::compare::compare_exports;
        let old = package(
            manifest(json!({"browser":"./browser.js"})),
            &[
                ("index.js", "module.exports = {};"),
                ("browser.js", "exports.feature = function() {};"),
            ],
        );
        let (before, _) = discover_exports(old.path()).unwrap();
        for (source, breaks) in [
            ("exports.other = function() {};", true),
            ("exports.feature = function() { return 42; };", false),
        ] {
            let new = package(
                manifest(json!({"browser":"./browser.js"})),
                &[("index.js", "module.exports = {};"), ("browser.js", source)],
            );
            let (after, _) = discover_exports(new.path()).unwrap();
            let findings = compare_exports(&before, &after, new.path(), false);
            assert_eq!(
                findings
                    .iter()
                    .any(|finding| finding.rule == "export-removed"),
                breaks,
                "{findings:?}"
            );
            for (root, succeeds) in [(old.path(), true), (new.path(), !breaks)] {
                let output = Command::new("node")
                    .args([
                        "--eval",
                        "require(require('./package.json').browser).feature();",
                    ])
                    .current_dir(root)
                    .output()
                    .unwrap();
                assert_success(&output, succeeds);
            }
        }
    }

    #[tokio::test]
    async fn dependency_preparation_ignores_published_package_root_overrides() {
        let transitive = package(
            json!({"name":"manifest-transitive", "version":"1.0.0", "main":"index.js"}),
            &[("index.js", "exports.feature = function() {};")],
        );
        let transitive = packed(transitive.path());
        let dependency = package(
            json!({"name":"manifest-host", "version":"1.0.0", "main":"index.js", "dependencies":{"manifest-transitive":format!("file:{}", archive(transitive.path()).display())}}),
            &[(
                "index.js",
                "module.exports = require('manifest-transitive');",
            )],
        );
        let dependency = packed(dependency.path());
        let source = package(
            manifest(
                json!({"dependencies":{"manifest-host":format!("file:{}", archive(dependency.path()).display())}, "overrides":{"manifest-transitive":"99.0.0"}}),
            ),
            &[("index.js", "module.exports = require('manifest-host');")],
        );
        let _preparation_state = configure_preparation(source.path(), "offline=true\n");

        let published = packed(source.path());
        let (consumer, output) = installed(&archive(published.path()), &[]);
        assert_success(&output, true);
        let output = Command::new("node")
            .args(["--eval", "require('manifest-contract').feature();"])
            .current_dir(consumer.path())
            .output()
            .unwrap();
        assert_success(&output, true);

        let original = std::fs::read(source.path().join("package.json")).unwrap();
        let prepared =
            pledge::ecosystems::node::dependencies::prepare_dependencies(source.path()).await;
        assert!(
            prepared.is_ok(),
            "consumer ignores the installed dependency's overrides: {prepared:?}"
        );
        assert_eq!(
            std::fs::read(source.path().join("package.json")).unwrap(),
            original
        );
        let output = Command::new("node")
            .args(["--eval", "require('./index.js').feature();"])
            .current_dir(source.path())
            .output()
            .unwrap();
        assert_success(&output, true);
    }

    #[tokio::test]
    async fn npm_http_tarball_exports_agree_with_node_consumers() {
        use pledge::ecosystems::node::{
            dependencies::prepare_dependencies, discovery::discover_exports,
        };
        use pledge::languages::javascript::compare::compare_exports;
        use std::io::{Read, Write};
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        struct LocalRegistry {
            stopped: Arc<AtomicBool>,
            thread: Option<std::thread::JoinHandle<()>>,
        }
        impl Drop for LocalRegistry {
            fn drop(&mut self) {
                self.stopped.store(true, Ordering::Relaxed);
                self.thread.take().unwrap().join().unwrap();
            }
        }
        let mut archives = Vec::new();
        for declaration in [
            "export function feature() {}",
            "export function other() {}",
            "export function feature() { return 42; }",
        ] {
            let dependency = package(
                json!({"name":"manifest-host", "version":"1.0.0", "type":"module", "exports":"./index.js"}),
                &[("index.js", declaration)],
            );
            let dependency = packed(dependency.path());
            archives.push(std::fs::read(archive(dependency.path())).unwrap());
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (accepted, connections) = std::sync::mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let shutdown = stopped.clone();
        let server = LocalRegistry {
            stopped,
            thread: Some(std::thread::spawn(move || {
                while !shutdown.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            // macOS inherits the listener's nonblocking mode. A
                            // partial header must wait for the remaining bytes.
                            stream.set_nonblocking(false).unwrap();
                            stream
                                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                                .unwrap();
                            let _ = accepted.send(());
                            let mut request = Vec::new();
                            let mut byte = [0];
                            while !request.ends_with(b"\r\n\r\n") && request.len() < 16_384 {
                                match stream.read(&mut byte) {
                                    Ok(0) => break,
                                    Ok(_) => request.push(byte[0]),
                                    Err(error) => panic!(
                                        "reading local tarball request after {:?}: {error}",
                                        String::from_utf8_lossy(&request)
                                    ),
                                }
                            }
                            let request = String::from_utf8_lossy(&request);
                            let index = request
                                .split_whitespace()
                                .nth(1)
                                .and_then(|path| path.strip_prefix('/'))
                                .and_then(|path| path.strip_suffix(".tgz"))
                                .and_then(|index| index.parse::<usize>().ok());
                            if let Some(body) = index.and_then(|index| archives.get(index)) {
                                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                                stream.write_all(body).unwrap();
                            } else {
                                stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(5))
                        }
                        Err(error) => panic!("local registry: {error}"),
                    }
                }
            })),
        };
        // Exercise an accepted connection before any header bytes arrive, then
        // split the request target across writes instead of relying on packet timing.
        let mut fragmented = std::net::TcpStream::connect(address).unwrap();
        fragmented
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        connections
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        fragmented.write_all(b"GET /0").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(25));
        fragmented
            .write_all(b".tgz HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut response = Vec::new();
        fragmented.read_to_end(&mut response).unwrap();
        assert!(
            response.starts_with(b"HTTP/1.1 200 OK\r\n"),
            "fragmented request: {}",
            String::from_utf8_lossy(&response)
        );

        let mut packages = Vec::new();
        let mut maps = Vec::new();
        for index in 0..3 {
            let source = package(
                manifest(
                    json!({"type":"module", "exports":"./index.js", "dependencies":{"manifest-host":format!("http://{address}/{index}.tgz")}}),
                ),
                &[("index.js", "export * from 'manifest-host';")],
            );
            let _preparation_state = configure_preparation(
                source.path(),
                &format!("registry=http://{address}/registry/\nfetch-retries=0\n"),
            );
            assert!(prepare_dependencies(source.path()).await.unwrap());
            let (exports, warnings) = discover_exports(source.path()).unwrap();
            assert!(warnings.is_empty(), "{warnings:?}");
            maps.push(exports);
            packages.push(source);
        }
        for (index, breaks) in [(1, true), (2, false)] {
            let findings = compare_exports(&maps[0], &maps[index], packages[index].path(), false);
            assert_eq!(
                findings
                    .iter()
                    .any(|finding| finding.rule == "export-removed"),
                breaks,
                "{findings:?}"
            );
            for (index, succeeds) in [(0, true), (index, !breaks)] {
                let output = Command::new("node")
                    .args([
                        "--input-type=module",
                        "--eval",
                        "import {feature} from 'manifest-contract'; feature();",
                    ])
                    .current_dir(packages[index].path())
                    .output()
                    .unwrap();
                assert_success(&output, succeeds);
            }
        }
        drop(server);
    }

    #[tokio::test]
    async fn npm_local_git_revision_exports_agree_with_node_consumers() {
        use pledge::ecosystems::node::{
            dependencies::prepare_dependencies, discovery::discover_exports,
        };
        use pledge::languages::javascript::compare::compare_exports;
        let repository = package(
            json!({"name":"manifest-host", "version":"1.0.0", "type":"module", "exports":"./index.js"}),
            &[],
        );
        let git = |arguments: &[&str]| {
            let output = Command::new("git")
                .args([
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "user.name=Pledge fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                ])
                .args(arguments)
                .current_dir(repository.path())
                .output()
                .unwrap();
            assert_success(&output, true);
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        };
        git(&["init", "--quiet"]);
        let mut packages = Vec::new();
        let mut maps = Vec::new();
        for declaration in [
            "export function feature() {}",
            "export function other() {}",
            "export function feature() { return 42; }",
        ] {
            std::fs::write(repository.path().join("index.js"), declaration).unwrap();
            git(&["add", "package.json", "index.js"]);
            git(&["commit", "--quiet", "-m", "local consumer fixture"]);
            let revision = git(&["rev-parse", "HEAD"]);
            let source = package(
                manifest(
                    json!({"type":"module", "exports":"./index.js", "dependencies":{"manifest-host":format!("git+file://{}#{revision}", repository.path().display())}}),
                ),
                &[("index.js", "export * from 'manifest-host';")],
            );
            let _preparation_state = configure_preparation(source.path(), "offline=true\n");
            assert!(prepare_dependencies(source.path()).await.unwrap());
            let (exports, warnings) = discover_exports(source.path()).unwrap();
            assert!(warnings.is_empty(), "{warnings:?}");
            maps.push(exports);
            packages.push(source);
        }
        for (index, breaks) in [(1, true), (2, false)] {
            let findings = compare_exports(&maps[0], &maps[index], packages[index].path(), false);
            assert_eq!(
                findings
                    .iter()
                    .any(|finding| finding.rule == "export-removed"),
                breaks,
                "{findings:?}"
            );
            for (index, succeeds) in [(0, true), (index, !breaks)] {
                let output = Command::new("node")
                    .args([
                        "--input-type=module",
                        "--eval",
                        "import {feature} from 'manifest-contract'; feature();",
                    ])
                    .current_dir(packages[index].path())
                    .output()
                    .unwrap();
                assert_success(&output, succeeds);
            }
        }
    }

    #[tokio::test]
    async fn unavailable_dependency_evidence_stays_incomplete() {
        let source = package(
            manifest(
                json!({"type":"module", "exports":"./index.js", "dependencies":{"manifest-host":"file:./missing.tgz"}}),
            ),
            &[("index.js", "export * from 'manifest-host';")],
        );
        let _preparation_state = configure_preparation(source.path(), "offline=true\n");
        assert!(
            pledge::ecosystems::node::dependencies::prepare_dependencies(source.path())
                .await
                .is_err()
        );
        let (mut exports, _) =
            pledge::ecosystems::node::discovery::discover_exports(source.path()).unwrap();
        let result =
            pledge::languages::typescript::compiler::enrich_exports(&mut exports, source.path())
                .await
                .unwrap();
        assert!(
            matches!(
                result,
                pledge::languages::typescript::compiler::EnrichResult::Degraded(_)
            ),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn npm_local_dependency_exports_agree_with_node_consumers() {
        use pledge::ecosystems::node::{
            dependencies::prepare_dependencies, discovery::discover_exports,
        };
        use pledge::languages::javascript::compare::compare_exports;
        for dependency_field in ["dependencies", "optionalDependencies", "bundleDependencies"] {
            let mut packages = Vec::new();
            let mut maps = Vec::new();
            for declaration in [
                "export function feature() {}",
                "export function other() {}",
                "export function feature() { return 42; }",
            ] {
                let dependency = package(
                    json!({"name":"manifest-host", "version":"1.0.0", "type":"module", "exports":"./index.js"}),
                    &[("index.js", declaration)],
                );
                let dependency = packed(dependency.path());
                let source = package(
                    manifest(
                        json!({"type":"module", "exports":"./index.js", "dependencies":{"manifest-host":format!("file:{}", archive(dependency.path()).display())}}),
                    ),
                    &[("index.js", "export * from 'manifest-host';")],
                );
                if dependency_field != "dependencies" {
                    let path = source.path().join("package.json");
                    let mut package_manifest: Value =
                        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                    if dependency_field == "optionalDependencies" {
                        package_manifest["optionalDependencies"] =
                            package_manifest["dependencies"].take();
                        package_manifest
                            .as_object_mut()
                            .unwrap()
                            .remove("dependencies");
                    } else {
                        package_manifest["bundleDependencies"] = json!(["manifest-host"]);
                    }
                    std::fs::write(path, package_manifest.to_string()).unwrap();
                }
                let _preparation_state = configure_preparation(source.path(), "offline=true\n");
                assert!(prepare_dependencies(source.path()).await.unwrap());
                if dependency_field == "bundleDependencies" {
                    let bundled = packed(source.path());
                    assert!(bundled
                        .path()
                        .join("package/node_modules/manifest-host/index.js")
                        .is_file());
                    let output = Command::new("node")
                        .args([
                            "--input-type=module",
                            "--eval",
                            "import {feature} from 'manifest-contract'; feature();",
                        ])
                        .current_dir(bundled.path().join("package"))
                        .output()
                        .unwrap();
                    assert_success(&output, !declaration.contains("other"));
                }
                let (exports, warnings) = discover_exports(source.path()).unwrap();
                assert!(warnings.is_empty(), "{warnings:?}");
                maps.push(exports);
                packages.push((source, dependency));
            }
            for (index, breaks) in [(1, true), (2, false)] {
                let findings =
                    compare_exports(&maps[0], &maps[index], packages[index].0.path(), false);
                assert_eq!(
                    findings
                        .iter()
                        .any(|finding| finding.rule == "export-removed"),
                    breaks,
                    "{findings:?}"
                );
                for (index, succeeds) in [(0, true), (index, !breaks)] {
                    let output = Command::new("node")
                        .args([
                            "--input-type=module",
                            "--eval",
                            "import {feature} from 'manifest-contract'; feature();",
                        ])
                        .current_dir(packages[index].0.path())
                        .output()
                        .unwrap();
                    assert_success(&output, succeeds);
                }
            }
        }
    }

    #[test]
    fn npm_platform_and_engine_rejections_agree_with_metadata() {
        let environment = Command::new("node")
            .args([
                "--eval",
                "console.log(JSON.stringify({os:process.platform,cpu:process.arch}))",
            ])
            .output()
            .unwrap();
        let environment: Value = serde_json::from_slice(&environment.stdout).unwrap();
        for (field, old, broken, compatible, rule, arguments) in [
            (
                "os",
                json!([environment["os"], "another-os"]),
                json!(["another-os"]),
                json!([environment["os"], "another-os", "third-os"]),
                "platform-narrowed",
                vec![],
            ),
            (
                "cpu",
                json!([environment["cpu"], "another-cpu"]),
                json!(["another-cpu"]),
                json!([environment["cpu"], "another-cpu", "third-cpu"]),
                "platform-narrowed",
                vec![],
            ),
            (
                "libc",
                Value::Null,
                json!(["unsupported-c-library"]),
                Value::Null,
                "platform-narrowed",
                vec![],
            ),
            (
                "engines",
                json!({"node":">=22"}),
                json!({"node":">=99"}),
                json!({"node":">=20"}),
                "engine-version-bumped",
                vec!["--engine-strict"],
            ),
        ] {
            let old = package(
                manifest(json!({field:old})),
                &[("index.js", "module.exports = 42;")],
            );
            for (new, breaks) in [(broken, true), (compatible, false)] {
                let new = package(
                    manifest(json!({field:new})),
                    &[("index.js", "module.exports = 42;")],
                );
                assert_eq!(has_rule(old.path(), new.path(), rule), breaks, "{field}");
                for (root, expected) in [(old.path(), true), (new.path(), !breaks)] {
                    let (_, output) = installed(root, &arguments);
                    assert_eq!(
                        output.status.success(),
                        expected,
                        "{field}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if !expected {
                        assert!(String::from_utf8_lossy(&output.stderr).contains(
                            if field == "engines" {
                                "EBADENGINE"
                            } else {
                                "EBADPLATFORM"
                            }
                        ));
                    }
                }
            }
        }
    }

    #[test]
    fn npm_platform_negations_and_scalar_lists_preserve_membership() {
        let environment = Command::new("node")
            .args([
                "--eval",
                "console.log(JSON.stringify({os:process.platform,cpu:process.arch}))",
            ])
            .output()
            .unwrap();
        let environment: Value = serde_json::from_slice(&environment.stdout).unwrap();
        for field in ["os", "cpu"] {
            let host = environment[field].as_str().unwrap();
            for (old, new, breaks) in [
                (
                    json!(["!another-platform"]),
                    json!(["!another-platform", format!("!{host}")]),
                    true,
                ),
                (
                    json!([host, "another-platform"]),
                    json!([host, "another-platform", format!("!{host}")]),
                    true,
                ),
                (json!(host), json!([host]), false),
                (json!([host]), json!([]), false),
                (json!([host]), json!(["any"]), false),
            ] {
                let old = package(manifest(json!({field:old})), &[]);
                let new = package(manifest(json!({field:new})), &[]);
                assert_eq!(
                    has_rule(old.path(), new.path(), "platform-narrowed"),
                    breaks
                );
                for (root, expected) in [(old.path(), true), (new.path(), !breaks)] {
                    let (_, output) = installed(root, &[]);
                    assert_success(&output, expected);
                }
            }
        }
    }

    #[test]
    fn npm_packed_files_preserve_or_remove_deep_imports() {
        use pledge::ecosystems::node::compare::compare_exports_map_diff;
        let files = [
            ("index.js", "module.exports = 42;"),
            ("feature.js", "module.exports = 42;"),
        ];
        let old = package(manifest(json!({"files":["*.js"]})), &files);
        let old = packed(old.path());
        for (files_field, breaks) in [
            (json!(["index.js"]), true),
            (json!(["index.js", "feature.js"]), false),
        ] {
            let new = package(manifest(json!({"files":files_field})), &files);
            let new = packed(new.path());
            let findings =
                compare_exports_map_diff(&old.path().join("package"), &new.path().join("package"));
            assert_eq!(
                findings
                    .iter()
                    .any(|finding| finding.rule == "importable-file-removed"),
                breaks,
                "{findings:?}"
            );
            for (root, expected) in [
                (old.path().join("package"), true),
                (new.path().join("package"), !breaks),
            ] {
                let (consumer, output) = installed(&root, &[]);
                assert_success(&output, true);
                let output = Command::new("node")
                    .args(["--eval", "require('manifest-contract/feature.js')"])
                    .current_dir(consumer.path())
                    .output()
                    .unwrap();
                assert_success(&output, expected);
            }
        }
    }

    #[test]
    fn npm_directory_bins_normalize_nested_and_hidden_candidates() {
        let files = [
            ("commands/tool", "#!/usr/bin/env node\nconsole.log(42)\n"),
            (
                "commands/nested/deep.js",
                "#!/usr/bin/env node\nconsole.log(42)\n",
            ),
            ("commands/.hidden", ""),
            ("commands/.hidden-dir/secret", ""),
        ];
        for directory in ["commands", "../commands", "/commands"] {
            let old = package(manifest(json!({"directories":{"bin":directory}})), &files);
            let explicit = package(
                manifest(
                    json!({"bin":{"tool":"commands/tool", "nested":"commands/nested", "deep.js":"commands/nested/deep.js"}}),
                ),
                &files,
            );
            assert!(!has_rule(old.path(), explicit.path(), "bin-entry-changed"));
            assert!(!has_rule(explicit.path(), old.path(), "bin-entry-changed"));
            let (consumer, output) = installed(old.path(), &[]);
            assert_success(&output, true);
            let mut commands: Vec<_> = std::fs::read_dir(consumer.path().join("node_modules/.bin"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            commands.sort();
            assert_eq!(commands, ["deep.js", "nested", "tool"]);
            assert!(consumer.path().join("node_modules/.bin/nested").is_dir());
        }
    }

    #[test]
    fn npm_directory_and_string_bins_preserve_installed_commands() {
        let files = [
            ("index.js", "module.exports = 42;"),
            ("commands/tool", "#!/usr/bin/env node\nconsole.log(42)\n"),
        ];
        let old = package(manifest(json!({"directories":{"bin":"commands"}})), &files);
        for (extra, breaks) in [
            (json!({"bin":{"other":"commands/tool"}}), true),
            (json!({"bin":{"tool":"./commands/tool"}}), false),
        ] {
            let new = package(manifest(extra), &files);
            assert_eq!(
                has_rule(old.path(), new.path(), "bin-entry-changed"),
                breaks
            );
            for (root, expected) in [(old.path(), true), (new.path(), !breaks)] {
                let (consumer, output) = installed(root, &[]);
                assert_success(&output, true);
                let command = consumer.path().join("node_modules/.bin/tool");
                assert_eq!(command.exists(), expected);
                if expected {
                    let output = Command::new("node").arg(command).output().unwrap();
                    assert_success(&output, true);
                    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
                }
            }
        }
        let old = package(
            json!({"name":"@review/tool", "version":"1.0.0", "bin":"./commands/tool"}),
            &files,
        );
        let compatible = package(
            json!({"name":"@review/tool", "version":"1.0.0", "bin":{"tool":"commands/tool"}}),
            &files,
        );
        assert!(!has_rule(
            old.path(),
            compatible.path(),
            "bin-entry-changed"
        ));
    }
}
