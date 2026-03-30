//! Managed typescript-language-server binary resolution.
//!
//! Check order: managed cache -> PATH -> download and cache.
//! Per decisions D-21, D-22.

use std::io::Cursor;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use tar::Archive;

use super::LspError;

/// Pinned typescript-language-server version for managed download.
pub const PINNED_VERSION: &str = "5.1.3";

/// Resolved typescript-language-server binary specification.
pub enum LspBinary {
    /// Direct executable on PATH (e.g., user's global npm install)
    Direct(PathBuf),
    /// Managed cache: run via `node <script_path> --stdio`
    Managed { node: PathBuf, script: PathBuf },
}

/// Resolve the path to a typescript-language-server binary.
///
/// Check order:
/// 1. Managed cache (`<platform_cache>/interlint/typescript-language-server-{VERSION}.mjs`)
/// 2. `typescript-language-server` on PATH
/// 3. Download from npm registry and cache
///
/// Returns `LspError::Unavailable` with a prescriptive message if all options fail.
pub async fn resolve_lsp_binary() -> Result<LspBinary, LspError> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| {
            LspError::Unavailable("cannot determine platform cache directory".to_string())
        })?
        .join("interlint");

    let cached_script = cache_dir.join(format!("typescript-language-server-{PINNED_VERSION}.mjs"));

    // 1. Check managed cache first
    if cached_script.exists() {
        if let Some(node) = find_node_binary().await {
            tracing::debug!(
                "Using managed typescript-language-server from {}",
                cached_script.display()
            );
            return Ok(LspBinary::Managed {
                node,
                script: cached_script,
            });
        }
        // Managed cache exists but node is not available — fall through to PATH check
        tracing::warn!(
            "Managed LSP cache found but `node` not on PATH; checking PATH for typescript-language-server"
        );
    }

    // 2. Check PATH for typescript-language-server
    if let Some(direct) = find_direct_binary().await {
        tracing::debug!(
            "Using typescript-language-server from PATH: {}",
            direct.display()
        );
        return Ok(LspBinary::Direct(direct));
    }

    // 3. Attempt managed download
    tracing::info!(
        "typescript-language-server not found; downloading version {PINNED_VERSION} to cache"
    );

    // Need node to run managed binary
    let node = find_node_binary().await.ok_or_else(|| {
        LspError::Unavailable(
            "typescript-language-server not found. Install with: npm install -g typescript-language-server".to_string()
        )
    })?;

    match download_and_cache(&cache_dir, &cached_script).await {
        Ok(()) => {
            tracing::info!(
                "Downloaded typescript-language-server to {}",
                cached_script.display()
            );
            Ok(LspBinary::Managed {
                node,
                script: cached_script,
            })
        }
        Err(e) => {
            tracing::warn!("Failed to download typescript-language-server: {e}");
            Err(LspError::Unavailable(
                "typescript-language-server not found. Install with: npm install -g typescript-language-server".to_string()
            ))
        }
    }
}

/// Check if `typescript-language-server` is directly on PATH.
async fn find_direct_binary() -> Option<PathBuf> {
    let output = tokio::process::Command::new("typescript-language-server")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()?;

    if output.status.success() {
        // Find it via `which`-like approach
        find_in_path("typescript-language-server")
    } else {
        None
    }
}

/// Check if `node` is on PATH and return its full path.
async fn find_node_binary() -> Option<PathBuf> {
    let output = tokio::process::Command::new("node")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()?;

    if output.status.success() {
        find_in_path("node")
    } else {
        None
    }
}

/// Find a binary in PATH by spawning `which` (Unix) or equivalent.
fn find_in_path(name: &str) -> Option<PathBuf> {
    // Use std::env::var("PATH") to scan for the binary
    let path_env = std::env::var("PATH").ok()?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
        // On Windows, also try with .cmd / .exe
        #[cfg(target_os = "windows")]
        {
            let with_cmd = dir.join(format!("{name}.cmd"));
            if with_cmd.exists() {
                return Some(with_cmd);
            }
            let with_exe = dir.join(format!("{name}.exe"));
            if with_exe.exists() {
                return Some(with_exe);
            }
        }
    }
    None
}

/// Download typescript-language-server tarball and extract `package/lib/cli.mjs` to cache.
async fn download_and_cache(
    cache_dir: &std::path::Path,
    cached_script: &std::path::Path,
) -> anyhow::Result<()> {
    let url = format!(
        "https://registry.npmjs.org/typescript-language-server/-/typescript-language-server-{PINNED_VERSION}.tgz"
    );

    tracing::debug!("Downloading typescript-language-server from {url}");

    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await.map_err(|e| {
        anyhow::anyhow!("network error downloading typescript-language-server: {e}")
    })?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "HTTP {} downloading typescript-language-server",
            resp.status()
        );
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("failed to read tarball bytes: {e}"))?;

    // Extract package/lib/cli.mjs from the tarball
    let script_bytes = extract_file_from_tgz(&bytes, "package/lib/cli.mjs")?;

    // Write to cache directory
    std::fs::create_dir_all(cache_dir).map_err(|e| {
        anyhow::anyhow!("cannot create cache directory {}: {e}", cache_dir.display())
    })?;

    std::fs::write(cached_script, &script_bytes)
        .map_err(|e| anyhow::anyhow!("cannot write cached script: {e}"))?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(cached_script, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| anyhow::anyhow!("cannot set executable permissions: {e}"))?;
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

/// Remove the managed typescript-language-server cache directory.
///
/// Used by the `--clear-cache` CLI flag (D-21).
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

    #[test]
    fn test_pinned_version_constant() {
        assert_eq!(PINNED_VERSION, "5.1.3");
    }
}
