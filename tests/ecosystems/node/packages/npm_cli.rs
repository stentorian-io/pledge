//! Real npm and compiler checks against an authenticated, local-only registry.

use super::{manifest, report, tarball_bytes};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

struct Registry {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Registry {
    fn start(package: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let dependency = json!({"name":"@ci/dependency","version":"1.0.0","type":"module","exports":"./index.js"});
        let metadata = |manifest: &Value, tarball: &str| {
            let mut version = manifest.clone();
            version["dist"] = json!({"tarball":format!("{url}/{tarball}")});
            json!({"name":manifest["name"],"dist-tags":{"latest":"1.0.0"},"versions":{"1.0.0":version}}).to_string().into_bytes()
        };
        let package_metadata = metadata(&package, "package.tgz");
        let dependency_metadata = metadata(&dependency, "dependency.tgz");
        let package_tarball = tarball_bytes(&package, "export const api = 1;");
        let dependency_tarball = tarball_bytes(&dependency, "export const dependency = 1;");
        let stopped = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let worker_stopped = Arc::clone(&stopped);
        let worker_requests = Arc::clone(&requests);
        let responses = Arc::new(HashMap::from([
            ("/@ci%2fexample", package_metadata),
            ("/@ci%2fdependency", dependency_metadata),
            ("/package.tgz", package_tarball),
            ("/dependency.tgz", dependency_tarball),
        ]));
        let thread = thread::spawn(move || {
            thread::scope(|scope| {
                while !worker_stopped.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let requests = Arc::clone(&worker_requests);
                            let responses = Arc::clone(&responses);
                            scope.spawn(move || respond(stream, &requests, &responses));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("registry accept failed: {error}"),
                    }
                }
            });
        });
        Self {
            url,
            requests,
            stopped,
            thread: Some(thread),
        }
    }
}

fn respond(
    mut stream: TcpStream,
    requests: &Mutex<Vec<String>>,
    responses: &HashMap<&str, Vec<u8>>,
) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(length) => request.extend_from_slice(&buffer[..length]),
        }
    }
    let request = String::from_utf8_lossy(&request).to_lowercase();
    let path = request.split_whitespace().nth(1).unwrap_or("");
    requests.lock().unwrap().push(path.to_owned());
    let (status, body): (&str, &[u8]) = if !request.contains("authorization: bearer fixture-token")
    {
        (
            "401 Unauthorized",
            br#"{"error":"authentication required"}"#,
        )
    } else if let Some(body) = responses.get(path) {
        ("200 OK", body)
    } else {
        ("404 Not Found", br#"{"error":"not found"}"#)
    };
    let content_type = if path.ends_with(".tgz") {
        "application/octet-stream"
    } else {
        "application/json"
    };
    if write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).is_ok() {
        let _ = stream.write_all(body);
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let result = self.thread.take().unwrap().join();
        if !thread::panicking() {
            result.unwrap();
        }
    }
}

#[test]
fn npm_workspace_analysis_preserves_inputs_and_uses_scoped_registry_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let package = root.join("packages/api");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{"private":true,"workspaces":["packages/*"]}"#,
    )
    .unwrap();
    let lockfile = r#"{"lockfileVersion":3,"packages":{}}"#;
    fs::write(root.join("package-lock.json"), lockfile).unwrap();
    // Only node/npm go on the child's PATH: TypeScript must come from the
    // original workspace, because npm pack excludes its dev dependencies.
    fs::create_dir(root.join("node_modules")).unwrap();
    let typescript = typescript_root();
    symlink(typescript, root.join("node_modules/typescript")).unwrap();
    let binaries = root.join("bin");
    fs::create_dir(&binaries).unwrap();
    for name in ["node", "npm"] {
        symlink(binary(name), binaries.join(name)).unwrap();
    }
    let mut old_manifest = manifest("1.0.0");
    old_manifest["dependencies"] = json!({"@ci/dependency":"1.0.0"});
    let registry = Registry::start(old_manifest.clone());
    let config = format!("@ci:registry={}\n//{}/:_authToken=fixture-token\ncache={}\nfetch-retries=0\nfetch-timeout=5000\nupdate-notifier=false\n", registry.url, registry.url.trim_start_matches("http://"), root.join("cache").display());
    fs::write(root.join(".npmrc"), &config).unwrap();
    let mut new_manifest = old_manifest;
    new_manifest["version"] = json!("2.0.0");
    new_manifest["scripts"] =
        json!({"prepack":"node -e \"require('fs').writeFileSync('prepack-ran', 'bad')\""});
    new_manifest["files"] = json!(["index.js"]);
    fs::write(package.join("package.json"), new_manifest.to_string()).unwrap();
    fs::write(package.join("index.js"), "export {};").unwrap();
    let existing_tarball = package.join("ci-example-2.0.0.tgz");
    fs::write(&existing_tarball, b"existing user artifact").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pledge"))
        .args(["--against", "latest", "--format", "json"])
        .arg(&package)
        .current_dir(root)
        .env("PATH", &binaries)
        .output()
        .unwrap();
    let result = report(&output, 1, "breaking");
    assert_eq!(result["package"]["baseline_version"], "1.0.0");
    assert!(
        result["warnings"].as_array().unwrap().is_empty(),
        "{result}"
    );
    assert_eq!(
        fs::read(&existing_tarball).unwrap(),
        b"existing user artifact"
    );
    assert_eq!(fs::read_to_string(root.join(".npmrc")).unwrap(), config);
    assert!(!package.join("prepack-ran").exists());
    assert!(!package.join("node_modules").exists());
    assert!(!package.join("package-lock.json").exists());
    assert_eq!(
        fs::read_to_string(root.join("package-lock.json")).unwrap(),
        lockfile
    );
    let requests = registry.requests.lock().unwrap();
    assert!(requests.iter().any(|path| path == "/@ci%2fexample"));
    assert!(requests.iter().any(|path| path == "/package.tgz"));
    assert!(requests.iter().any(|path| path == "/@ci%2fdependency"));
    assert!(requests.iter().any(|path| path == "/dependency.tgz"));
}

fn binary(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("{name} must be installed on PATH for this test"))
}

fn typescript_root() -> PathBuf {
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap()) {
        for name in ["tsserver", "tsc", "typescript-language-server"] {
            let Ok(binary) = directory.join(name).canonicalize() else {
                continue;
            };
            for ancestor in binary.ancestors() {
                for candidate in [
                    ancestor.to_path_buf(),
                    ancestor.join("node_modules/typescript"),
                ] {
                    if candidate.join("lib/typescript.js").is_file() {
                        return candidate;
                    }
                }
            }
        }
    }
    use pledge::languages::typescript::compiler::toolchain::{PINNED_VERSION, TYPESCRIPT_VERSION};
    let cached = dirs::cache_dir()
        .unwrap()
        .join("pledge")
        .join(format!(
            "tls-{PINNED_VERSION}-typescript-{TYPESCRIPT_VERSION}"
        ))
        .join("node_modules/typescript");
    assert!(
        cached.join("lib/typescript.js").is_file(),
        "Install typescript@5.9.3 on PATH or populate the managed toolchain cache before this test"
    );
    cached
}
