//! Resolve a language server together with the JavaScript TypeScript compiler.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use flate2::read::GzDecoder;
use tar::Archive;

use super::LspError;

pub const PINNED_VERSION: &str = "5.1.3";
pub const TYPESCRIPT_VERSION: &str = "5.9.3";

pub enum LspBinary {
    Direct(PathBuf),
    Managed { node: PathBuf, script: PathBuf },
}

pub(crate) struct CompilerBinary {
    pub node: PathBuf,
    pub library: PathBuf,
}

/// Compiler evaluation does not require a language-server process. Prefer the
/// package's compiler, then PATH; the managed pair supplies an offline cache or
/// installation fallback when neither has the JavaScript compiler API.
pub(crate) async fn resolve_compiler(workspace: &Path) -> Result<CompilerBinary, LspError> {
    let node = find_working_binary("node").await.ok_or_else(|| {
        LspError::Unavailable("Node.js is required for TypeScript contract evaluation".into())
    })?;
    let server = workspace.ancestors().find_map(compiler_under).or_else(|| {
        ["tsserver", "tsc"]
            .iter()
            .find_map(|name| compiler_near_binary(&find_in_path(name)?))
    });
    if let Some(library) = server
        .and_then(|server| Some(server.parent()?.join("typescript.js")))
        .filter(|library| library.is_file())
    {
        return Ok(CompilerBinary { node, library });
    }
    let binary = resolve_lsp_binary(workspace).await?;
    let library = binary
        .tsserver_path(workspace)
        .and_then(|server| Some(server.parent()?.join("typescript.js")))
        .ok_or_else(|| {
            LspError::Unavailable("JavaScript TypeScript compiler API is unavailable".into())
        })?;
    Ok(CompilerBinary { node, library })
}

impl LspBinary {
    pub(crate) fn tsserver_path(&self, workspace: &Path) -> Option<PathBuf> {
        if let Self::Managed { script, .. } = self {
            // A managed installation is a tested pair, independent of workspace tooling.
            return compiler_under(script.parent()?.parent()?);
        }
        workspace
            .ancestors()
            .find_map(compiler_under)
            .or_else(|| {
                let Self::Direct(binary) = self else {
                    return None;
                };
                compiler_near_binary(binary)
            })
            .or_else(|| {
                ["tsserver", "tsc"]
                    .iter()
                    .find_map(|name| compiler_near_binary(&find_in_path(name)?))
            })
    }
}

fn compiler_under(root: &Path) -> Option<PathBuf> {
    let compiler = root.join("node_modules/typescript/lib/tsserver.js");
    compiler.is_file().then_some(compiler)
}

fn compiler_near_binary(binary: &Path) -> Option<PathBuf> {
    let resolved = binary.canonicalize().ok()?;
    let sibling = resolved.parent()?.join("tsserver.js");
    if sibling.is_file() {
        return Some(sibling);
    }
    resolved.ancestors().find_map(compiler_under)
}

fn cache_complete(root: &Path) -> bool {
    root.join("lib/cli.mjs").is_file()
        && root.join("package.json").is_file()
        && compiler_under(root).is_some()
        && root
            .join("node_modules/typescript/lib/typescript.js")
            .is_file()
        && root.join("node_modules/typescript/lib/lib.d.ts").is_file()
        && root.join("node_modules/typescript/package.json").is_file()
}

/// Prefer a complete managed pair, then a usable PATH installation, then download.
pub async fn resolve_lsp_binary(workspace: &Path) -> Result<LspBinary, LspError> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| LspError::Unavailable("cannot determine platform cache directory".into()))?
        .join("interlint");
    let root = cache_dir.join(format!(
        "tls-{PINNED_VERSION}-typescript-{TYPESCRIPT_VERSION}"
    ));
    let node = find_working_binary("node").await;
    if cache_complete(&root) {
        if let Some(node) = node.clone() {
            return Ok(LspBinary::Managed {
                node,
                script: root.join("lib/cli.mjs"),
            });
        }
    }
    if let Some(path) = find_working_binary("typescript-language-server").await {
        let binary = LspBinary::Direct(path);
        if binary.tsserver_path(workspace).is_some() {
            return Ok(binary);
        }
    }
    let node = node.ok_or_else(|| {
        LspError::Unavailable(
            "Node.js is required; install Node.js, typescript@5.9.3 and typescript-language-server"
                .into(),
        )
    })?;
    download_and_cache(&cache_dir, &root).await.map_err(|error| LspError::Unavailable(format!(
        "could not install the managed TypeScript toolchain: {error}; install typescript@5.9.3 and typescript-language-server"
    )))?;
    Ok(LspBinary::Managed {
        node,
        script: root.join("lib/cli.mjs"),
    })
}

async fn find_working_binary(name: &str) -> Option<PathBuf> {
    let path = find_in_path(name)?;
    let mut command = tokio::process::Command::new(&path);
    command
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let status = tokio::time::timeout(Duration::from_secs(5), command.status())
        .await
        .ok()?
        .ok()?;
    status.success().then_some(path)
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(target_os = "windows")]
        for extension in ["cmd", "exe"] {
            let candidate = dir.join(format!("{name}.{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

async fn download_and_cache(cache_dir: &Path, destination: &Path) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let server = download_package(&client, "typescript-language-server", PINNED_VERSION).await?;
    let compiler = download_package(&client, "typescript", TYPESCRIPT_VERSION).await?;
    install_toolchain(cache_dir, destination, &server, &compiler)
}

async fn download_package(
    client: &reqwest::Client,
    name: &str,
    version: &str,
) -> anyhow::Result<Vec<u8>> {
    let url = format!("https://registry.npmjs.org/{name}/-/{name}-{version}.tgz");
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec())
}

fn install_toolchain(
    cache_dir: &Path,
    destination: &Path,
    server: &[u8],
    compiler: &[u8],
) -> anyhow::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let staging = tempfile::tempdir_in(cache_dir)?;
    let modules = staging.path().join("node_modules");
    std::fs::create_dir(&modules)?;
    Archive::new(GzDecoder::new(Cursor::new(compiler))).unpack(&modules)?;
    std::fs::rename(modules.join("package"), modules.join("typescript"))?;
    std::fs::create_dir(staging.path().join("lib"))?;
    std::fs::write(
        staging.path().join("package.json"),
        extract_file_from_tgz(server, "package/package.json")?,
    )?;
    std::fs::write(
        staging.path().join("lib/cli.mjs"),
        extract_file_from_tgz(server, "package/lib/cli.mjs")?,
    )?;
    anyhow::ensure!(
        cache_complete(staging.path()),
        "downloaded toolchain is incomplete"
    );
    // Only completed installations become visible. Another process may win this race.
    if cache_complete(destination) {
        return Ok(());
    }
    if destination.exists() {
        std::fs::remove_dir_all(destination)?;
    }
    if let Err(error) = std::fs::rename(staging.path(), destination) {
        if !cache_complete(destination) {
            return Err(error.into());
        }
    }
    Ok(())
}

/// Extract a single file from a gzip-compressed tar archive in memory.
fn extract_file_from_tgz(bytes: &[u8], target_path: &str) -> anyhow::Result<Vec<u8>> {
    let cursor = Cursor::new(bytes);
    let decoder = GzDecoder::new(cursor);
    let mut archive = Archive::new(decoder);

    for entry in archive
        .entries()
        .map_err(|e| anyhow::anyhow!("failed to read tar entries: {e}"))?
    {
        let mut entry = entry.map_err(|e| anyhow::anyhow!("tar entry error: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| anyhow::anyhow!("tar entry path error: {e}"))?;
        if path.to_string_lossy() == target_path {
            let mut content = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut content)
                .map_err(|e| anyhow::anyhow!("failed to read tar entry contents: {e}"))?;
            return Ok(content);
        }
    }

    anyhow::bail!("file '{target_path}' not found in tarball")
}

/// Remove the managed language-server and compiler cache.
pub fn clear_cache() -> anyhow::Result<()> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine platform cache directory"))?
        .join("interlint");

    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir).map_err(|e| {
            anyhow::anyhow!("cannot remove cache directory {}: {e}", cache_dir.display())
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive.append_data(&mut header, path, *bytes).unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn cache_requires_a_complete_compiler_and_server_pair() {
        let cache = tempfile::tempdir().unwrap();
        let destination = cache.path().join("toolchain");
        let server = tarball(&[
            ("package/lib/cli.mjs", b"server"),
            ("package/package.json", b"{}"),
        ]);
        let incomplete = tarball(&[("package/lib/tsserver.js", b"compiler")]);
        assert!(install_toolchain(cache.path(), &destination, &server, &incomplete).is_err());
        assert!(!destination.exists());
        let compiler = tarball(&[
            ("package/lib/tsserver.js", b"compiler"),
            ("package/lib/typescript.js", b"implementation"),
            ("package/lib/lib.d.ts", b"declarations"),
            ("package/package.json", b"{}"),
        ]);
        install_toolchain(cache.path(), &destination, &server, &compiler).unwrap();
        assert!(cache_complete(&destination));
        install_toolchain(cache.path(), &destination, &server, &compiler).unwrap();
        assert_eq!(
            std::fs::read(destination.join("lib/cli.mjs")).unwrap(),
            b"server"
        );
        assert!(extract_file_from_tgz(&compiler, "missing").is_err());
        let binary = LspBinary::Managed {
            node: "node".into(),
            script: destination.join("lib/cli.mjs"),
        };
        assert_eq!(
            binary.tsserver_path(cache.path()),
            compiler_under(&destination)
        );
    }

    #[test]
    fn workspace_compiler_is_found_without_a_global_installation() {
        let workspace = tempfile::tempdir().unwrap();
        let lib = workspace.path().join("node_modules/typescript/lib");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("tsserver.js"), "").unwrap();
        let binary = LspBinary::Direct(workspace.path().join("absent-server"));
        assert_eq!(
            binary.tsserver_path(workspace.path()),
            Some(lib.join("tsserver.js"))
        );
        assert_eq!(
            compiler_near_binary(&lib.join("tsserver.js")),
            Some(lib.join("tsserver.js").canonicalize().unwrap())
        );
    }

    #[cfg(feature = "lsp-tests")]
    #[tokio::test]
    async fn downloaded_toolchain_enriches_without_workspace_typescript() {
        let cache = tempfile::tempdir().unwrap();
        let destination = cache.path().join("toolchain");
        download_and_cache(cache.path(), &destination)
            .await
            .unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join("package.json"),
            r#"{"name":"managed-test","exports":"./index.ts"}"#,
        )
        .unwrap();
        std::fs::write(
            workspace.path().join("index.ts"),
            "export function api(input: string | number): string { return String(input); }",
        )
        .unwrap();
        let root = workspace.path().canonicalize().unwrap();
        let (mut exports, _) = crate::discover::discover_exports(&root).unwrap();
        let binary = LspBinary::Managed {
            node: find_working_binary("node").await.unwrap(),
            script: destination.join("lib/cli.mjs"),
        };
        let result =
            crate::lsp::enrich_with_binary(&mut exports, &root, &binary, Duration::from_secs(30))
                .await
                .unwrap();
        assert!(
            matches!(result, crate::lsp::EnrichResult::Enriched),
            "{result:?}"
        );
        assert!(exports.values().all(|symbol| symbol
            .type_signature
            .as_ref()
            .is_some_and(|signature| signature.contains("string | number"))));
        assert!(!root.join("node_modules/typescript").exists());
    }
}
