use anyhow::{bail, Context};

#[allow(dead_code)]
const REGISTRY_URL: &str = "https://registry.npmjs.org";

/// Metadata extracted from an npm packument for a published package.
#[allow(dead_code)]
pub struct PackageMetadata {
    pub name: String,
    pub latest_version: String,
    pub tarball_url: String,
}

/// Fetch packument metadata for a package from the npm registry.
///
/// Returns `Ok(None)` when the package has never been published (HTTP 404),
/// which is the first-publish case (PKG-03). All other non-success responses
/// are treated as errors with a prescriptive network error message per D-09.
#[allow(dead_code)]
pub async fn fetch_package_metadata(name: &str) -> anyhow::Result<Option<PackageMetadata>> {
    let url = format!("{}/{}", REGISTRY_URL, name);
    let client = reqwest::Client::new();

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Cannot reach npm registry — check your network connection")?;

    if resp.status().as_u16() == 404 {
        return Ok(None); // first publish — not an error
    }

    if !resp.status().is_success() {
        let status = resp.status();
        bail!(
            "Cannot reach npm registry — check your network connection. (HTTP {status})"
        );
    }

    let packument: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse npm registry response")?;

    let latest_version = packument["dist-tags"]["latest"]
        .as_str()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "npm registry response missing dist-tags.latest for package '{name}'"
            )
        })?
        .to_string();

    let tarball_url = packument["versions"][&latest_version]["dist"]["tarball"]
        .as_str()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "npm registry response missing tarball URL for package '{name}' version '{latest_version}'"
            )
        })?
        .to_string();

    Ok(Some(PackageMetadata {
        name: name.to_string(),
        latest_version,
        tarball_url,
    }))
}

/// Download a tarball from the given URL and return its bytes.
///
/// Buffers the entire response in memory before returning (not streamed),
/// which is required before passing to GzDecoder.
#[allow(dead_code)]
pub async fn download_tarball(url: &str) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::Client::new();

    let resp = client
        .get(url)
        .send()
        .await
        .context("Cannot reach npm registry — check your network connection")?;

    if !resp.status().is_success() {
        bail!("Failed to download tarball from {url}");
    }

    Ok(resp
        .bytes()
        .await
        .context("Failed to read tarball bytes")?
        .to_vec())
}
