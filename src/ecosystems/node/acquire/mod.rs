pub mod extract;
mod pack;
mod registry;

use crate::ecosystems::node::npm;
use anyhow::{ensure, Context};
pub use extract::{extract_tgz, extract_tgz_from_bytes};
use serde::Serialize;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub(crate) struct PackageComparison {
    pub name: String,
    pub current_version: String,
    pub baseline_version: Option<String>,
}

/// Own both snapshots for the complete analysis, including their temporary files.
pub(crate) struct AcquiredPackages {
    pub current_root: PathBuf,
    pub previous_root: Option<PathBuf>,
    pub comparison: PackageComparison,
    _current_dir: tempfile::TempDir,
    _previous_dir: Option<tempfile::TempDir>,
}

/// Pack built artifacts and use npm's configured registry for the baseline.
pub(crate) async fn acquire_packages(
    dir: &Path,
    baseline: Option<&str>,
) -> anyhow::Result<AcquiredPackages> {
    let manifest = std::fs::read(dir.join("package.json")).with_context(|| {
        format!(
            "Cannot read {}/package.json; pass a package root directory",
            dir.display()
        )
    })?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&manifest).context("Invalid package.json")?;
    let name = manifest["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .context("package.json is missing a non-empty name")?;
    let version = manifest["version"]
        .as_str()
        .filter(|version| !version.is_empty())
        .context("package.json is missing a non-empty version")?;
    // Registry package specs must not turn into file, URL, or Git acquisitions.
    let parts: Vec<_> = name.split('/').collect();
    let valid_part = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte))
    };
    ensure!(
        match parts.as_slice() {
            [package] => valid_part(package) && !package.starts_with('.'),
            [scope, package] =>
                scope.strip_prefix('@').is_some_and(valid_part) && valid_part(package),
            _ => false,
        },
        "package.json name must be an npm package name"
    );
    if let Some(baseline) = baseline {
        let exact_version = node_semver::Version::parse(baseline).is_ok();
        let tag = !baseline.is_empty()
            && baseline
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            && node_semver::Range::parse(baseline).is_err();
        ensure!(
            exact_version || tag,
            "--against requires an exact version or dist-tag"
        );
    }

    // Resolve the workspace root before disabling workspace selection. Otherwise
    // npm treats a workspace member as its own project and loses the root .npmrc.
    let prefix = npm::output(npm::command(dir).arg("prefix"), "prefix").await?;
    if !prefix.status.success() {
        return Err(npm::failure(&prefix, "prefix"));
    }
    let project = PathBuf::from(
        String::from_utf8(prefix.stdout)
            .context("npm prefix was not UTF-8")?
            .trim(),
    );
    let baseline_version = registry::resolve_published_version(&project, name, baseline).await?;
    let packed = tempfile::tempdir().context("Cannot create packing directory")?;
    let current_tarball = pack::pack_package(&project, dir.as_os_str(), packed.path()).await?;
    let (current_dir, current_root) = extract_tgz(&current_tarball)?;
    let previous = if let Some(version) = &baseline_version {
        let tarball = pack::pack_package(
            &project,
            OsStr::new(&format!("{name}@{version}")),
            packed.path(),
        )
        .await?;
        Some(extract_tgz(&tarball)?)
    } else {
        None
    };

    // Published packages exclude .npmrc. Preserve the caller's project settings
    // for installations in the isolated snapshots without changing user config.
    let config = project.join(".npmrc");
    for root in std::iter::once(&current_root).chain(previous.iter().map(|(_, root)| root)) {
        let destination = root.join(".npmrc");
        // A downloaded archive must not choose npm's installation configuration
        // or supply a symlink that receives the caller's credentials.
        match std::fs::remove_file(&destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("Cannot discard packed npm configuration"),
        }
        if config.is_file() {
            std::fs::copy(&config, destination)
                .context("Cannot preserve project npm configuration")?;
        }
    }
    let (previous_dir, previous_root) = previous.unzip();
    Ok(AcquiredPackages {
        current_root,
        previous_root,
        comparison: PackageComparison {
            name: name.to_owned(),
            current_version: version.to_owned(),
            baseline_version,
        },
        _current_dir: current_dir,
        _previous_dir: previous_dir,
    })
}
