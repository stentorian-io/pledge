// Package acquisition: npm pack, registry download, tarball extraction

use anyhow::Context;

pub mod extract;
pub mod pack;
pub mod registry;

pub use extract::{extract_tgz, extract_tgz_from_bytes};
pub use pack::pack_current;
#[allow(unused_imports)]
pub use registry::{download_tarball, fetch_package_metadata, NpmClient, PackageMetadata};

/// The result of package acquisition, representing either a first-publish or a
/// comparison between the current (packed) version and the previous published version.
#[allow(dead_code)]
pub enum AcquireResult {
    /// Both versions are available for comparison.
    BothVersions {
        current_root: std::path::PathBuf,
        previous_root: std::path::PathBuf,
        // TempDirs held here for RAII cleanup when AcquireResult is dropped
        _current_dir: tempfile::TempDir,
        _previous_dir: tempfile::TempDir,
    },
    /// Package has never been published — first-publish case (PKG-03).
    FirstPublish {
        current_root: std::path::PathBuf,
        _current_dir: tempfile::TempDir,
    },
}

/// High-level orchestration: pack current package, fetch previous version from registry,
/// extract both into temp directories, and return the extraction roots.
///
/// If the package has never been published (HTTP 404 from registry), returns
/// `AcquireResult::FirstPublish` with only the current extracted version.
///
/// The local `.tgz` file created by `npm pack` is removed after successful extraction.
#[allow(dead_code)]
pub async fn acquire_packages(dir: &std::path::Path) -> anyhow::Result<AcquireResult> {
    // Read package name from package.json
    let pkg_json_path = dir.join("package.json");
    let pkg_json_bytes = std::fs::read(&pkg_json_path).map_err(|_| {
        anyhow::anyhow!("No package.json found. Run interlint from your package root directory.")
    })?;
    let pkg_json: serde_json::Value =
        serde_json::from_slice(&pkg_json_bytes).context("Failed to parse package.json")?;
    let name = pkg_json["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("package.json is missing the \"name\" field"))?;

    // Pack the current version
    let tarball_path = pack_current(dir)?;

    // Extract current version
    let (current_dir, current_root) = extract_tgz(&tarball_path)?;

    // Clean up the local .tgz created by npm pack
    std::fs::remove_file(&tarball_path).ok();

    // Fetch previous version metadata from registry and download tarball using a shared client
    let npm_client = registry::NpmClient::new()?;
    match npm_client.fetch_package_metadata(name).await? {
        None => {
            // First publish — no previous version to compare
            Ok(AcquireResult::FirstPublish {
                current_root,
                _current_dir: current_dir,
            })
        }
        Some(meta) => {
            // Download and extract previous version (reuses same TCP/TLS session)
            let bytes = npm_client.download_tarball(&meta.tarball_url).await?;
            let (previous_dir, previous_root) = extract_tgz_from_bytes(&bytes)?;

            Ok(AcquireResult::BothVersions {
                current_root,
                previous_root,
                _current_dir: current_dir,
                _previous_dir: previous_dir,
            })
        }
    }
}
