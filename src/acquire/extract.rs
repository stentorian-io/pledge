use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use flate2::read::GzDecoder;
use tar::Archive;
use tempfile::TempDir;

/// Extract a `.tgz` tarball from a file path into a temporary directory.
///
/// npm tarballs always nest content under a `package/` subdirectory inside the archive.
/// Returns `(TempDir, PathBuf)` where `TempDir` must be kept alive for the path to remain valid,
/// and `PathBuf` points to the `package/` root inside the temp dir.
#[allow(dead_code)]
pub fn extract_tgz(tgz_path: &Path) -> anyhow::Result<(TempDir, PathBuf)> {
    let dir = TempDir::new().context("Failed to create temporary directory")?;

    let file =
        std::fs::File::open(tgz_path).with_context(|| format!("Cannot open {}", tgz_path.display()))?;

    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(dir.path())
        .with_context(|| format!("Failed to extract {}", tgz_path.display()))?;

    let pkg_root = dir.path().join("package");

    if !pkg_root.join("package.json").exists() {
        bail!("Extracted tarball does not contain package/package.json — unexpected tarball layout");
    }

    Ok((dir, pkg_root))
}

/// Extract a `.tgz` tarball from in-memory bytes into a temporary directory.
///
/// Equivalent to `extract_tgz` but reads from a byte slice instead of a file.
/// Used for tarballs downloaded from the npm registry (buffered in memory).
#[allow(dead_code)]
pub fn extract_tgz_from_bytes(bytes: &[u8]) -> anyhow::Result<(TempDir, PathBuf)> {
    let dir = TempDir::new().context("Failed to create temporary directory")?;

    let cursor = Cursor::new(bytes);
    let decoder = GzDecoder::new(cursor);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(dir.path())
        .context("Failed to extract tarball from bytes")?;

    let pkg_root = dir.path().join("package");

    if !pkg_root.join("package.json").exists() {
        bail!("Extracted tarball does not contain package/package.json — unexpected tarball layout");
    }

    Ok((dir, pkg_root))
}
