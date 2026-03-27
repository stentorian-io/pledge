use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};

/// Pack the current package using `npm pack --json` and return the path to the produced tarball.
///
/// The filename is always read from npm's JSON output, never constructed manually.
/// This handles scoped packages correctly: `@myorg/pkg` produces `myorg-pkg-1.0.0.tgz`.
pub fn pack_current(dir: &Path) -> anyhow::Result<PathBuf> {
    // Check npm is available before attempting pack
    Command::new("npm")
        .arg("--version")
        .output()
        .map_err(|_| anyhow::anyhow!("npm not found. Install Node.js: https://nodejs.org"))?;

    let output = Command::new("npm")
        .args(["pack", "--json"])
        .current_dir(dir)
        .output()
        .context("Failed to spawn npm")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "npm pack failed: {stderr}\n\nMake sure npm is installed and you are in a valid package directory."
        );
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse npm pack JSON output")?;

    let filename = json[0]["filename"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("npm pack produced no filename in JSON output"))?;

    Ok(dir.join(filename))
}
