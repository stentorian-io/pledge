use crate::ecosystems::node::npm;
use anyhow::{ensure, Context};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// Pack exactly one package into an owned temporary directory. Package lifecycle
/// scripts are deliberately disabled: callers must build publishable files first.
pub(crate) async fn pack_package(
    directory: &Path,
    specification: &OsStr,
    destination: &Path,
) -> anyhow::Result<PathBuf> {
    let output = npm::output(
        npm::command(directory)
            .args([
                "pack",
                "--json",
                "--ignore-scripts",
                "--workspaces=false",
                "--pack-destination",
            ])
            .arg(destination)
            .arg("--")
            .arg(specification),
        "pack",
    )
    .await?;
    if !output.status.success() {
        return Err(npm::failure(&output, "pack"));
    }
    let packages: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse npm pack JSON output")?;
    ensure!(
        packages
            .as_array()
            .is_some_and(|packages| packages.len() == 1),
        "npm pack must produce exactly one package; pass the path of a single package"
    );
    let filename = packages[0]["filename"]
        .as_str()
        .context("npm pack produced no filename in JSON output")?;
    let mut components = Path::new(filename).components();
    ensure!(
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none(),
        "npm pack returned an invalid tarball filename"
    );
    let tarball = destination.join(filename);
    ensure!(
        std::fs::metadata(&tarball)?.len() <= 500 * 1024 * 1024,
        "Packed tarball exceeds the 500MB analysis limit"
    );
    Ok(tarball)
}
