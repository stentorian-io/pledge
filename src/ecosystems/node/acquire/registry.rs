use crate::ecosystems::node::npm;
use anyhow::Context;
use std::path::Path;

/// Let npm resolve registry, scope, authentication, proxy and certificate settings.
/// An explicitly selected baseline must exist, including an explicit `latest`.
pub(crate) async fn resolve_published_version(
    directory: &Path,
    name: &str,
    baseline: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let specification = format!("{name}@{}", baseline.unwrap_or("latest"));
    let output = npm::output(
        npm::command(directory).args([
            "view",
            "--json",
            "--workspaces=false",
            "--",
            &specification,
            "version",
        ]),
        "view",
    )
    .await?;
    if !output.status.success() {
        let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
        if baseline.is_none() && error["error"]["code"] == "E404" {
            return Ok(None);
        }
        return Err(npm::failure(
            &output,
            &format!("view {specification}; check the baseline and registry credentials"),
        ));
    }
    let version: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse npm version response")?;
    let version = version
        .as_str()
        .filter(|version| !version.is_empty())
        .context(
            "Baseline must resolve to one published version; use an exact version or dist-tag",
        )?;
    node_semver::Version::parse(version).context("Registry returned an invalid package version")?;
    Ok(Some(version.to_owned()))
}
