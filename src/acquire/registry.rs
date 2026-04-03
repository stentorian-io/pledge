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

/// Encode a package name for use in an npm registry URL.
///
/// Scoped packages (e.g. `@scope/package`) require the `/` separator
/// to be percent-encoded as `%2f` so the URL path is not ambiguous.
/// Unscoped packages are passed through unchanged.
fn encode_package_name(name: &str) -> String {
    if name.starts_with('@') {
        name.replacen("/", "%2f", 1)
    } else {
        name.to_string()
    }
}

/// HTTP client for npm registry operations.
///
/// Holds a shared `reqwest::Client` for connection pooling and consistent
/// User-Agent identification. Build once via `NpmClient::new()`, reuse
/// across all registry calls.
#[allow(dead_code)]
pub struct NpmClient {
    client: reqwest::Client,
    registry_url: String,
}

#[allow(dead_code)]
impl NpmClient {
    /// Create a new NpmClient with connection pooling and User-Agent header.
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("interlint/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            client,
            registry_url: REGISTRY_URL.to_string(),
        })
    }

    /// Fetch packument metadata for a package from the npm registry.
    ///
    /// Returns `Ok(None)` when the package has never been published (HTTP 404),
    /// which is the first-publish case (PKG-03). All other non-success responses
    /// are treated as errors with a prescriptive network error message per D-09.
    pub async fn fetch_package_metadata(
        &self,
        name: &str,
    ) -> anyhow::Result<Option<PackageMetadata>> {
        let encoded_name = encode_package_name(name);
        let url = format!("{}/{}", self.registry_url, encoded_name);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Cannot reach npm registry \u{2014} check your network connection")?;

        if resp.status().as_u16() == 404 {
            return Ok(None); // first publish — not an error
        }

        if !resp.status().is_success() {
            let status = resp.status();
            bail!("Cannot reach npm registry \u{2014} check your network connection. (HTTP {status})");
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
    pub async fn download_tarball(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("Cannot reach npm registry \u{2014} check your network connection")?;

        if !resp.status().is_success() {
            bail!("Failed to download tarball from {url}");
        }

        Ok(resp
            .bytes()
            .await
            .context("Failed to read tarball bytes")?
            .to_vec())
    }
}

/// Convenience wrapper — creates a one-shot NpmClient.
/// Prefer using NpmClient::new() directly for multiple calls.
#[allow(dead_code)]
pub async fn fetch_package_metadata(name: &str) -> anyhow::Result<Option<PackageMetadata>> {
    NpmClient::new()?.fetch_package_metadata(name).await
}

/// Convenience wrapper — creates a one-shot NpmClient.
/// Prefer using NpmClient::new() directly for multiple calls.
#[allow(dead_code)]
pub async fn download_tarball(url: &str) -> anyhow::Result<Vec<u8>> {
    NpmClient::new()?.download_tarball(url).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the URL that would be used by fetch_package_metadata without making HTTP calls.
    fn registry_url_for(name: &str) -> String {
        let encoded = encode_package_name(name);
        format!("{}/{}", REGISTRY_URL, encoded)
    }

    #[test]
    fn test_scoped_package_encodes_slash() {
        let url = registry_url_for("@scope/pkg");
        assert!(
            url.contains("@scope%2fpkg"),
            "scoped package should have / encoded as %2f; got: {url}"
        );
        assert!(
            !url.contains("@scope/pkg"),
            "raw / should not appear after the scope separator; got: {url}"
        );
    }

    #[test]
    fn test_unscoped_package_no_encoding() {
        let url = registry_url_for("lodash");
        assert!(
            url.ends_with("/lodash"),
            "unscoped package should not be encoded; got: {url}"
        );
        assert!(
            !url.contains("%2f"),
            "unscoped package should contain no percent-encoding; got: {url}"
        );
    }

    #[test]
    fn test_scoped_package_with_hyphen() {
        let url = registry_url_for("@babel/core");
        assert!(
            url.contains("@babel%2fcore"),
            "scoped package with hyphen should be encoded; got: {url}"
        );
    }

    #[test]
    fn test_npm_client_builds_successfully() {
        let result = NpmClient::new();
        assert!(
            result.is_ok(),
            "NpmClient::new() should succeed; got: {:?}",
            result.err()
        );
    }
}
