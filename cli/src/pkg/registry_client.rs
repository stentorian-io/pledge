//! Registry client for interacting with npm-compatible registries.
//!
//! This module provides a clean HTTP client interface for:
//! - Checking if a package/version exists in the registry
//! - Fetching package metadata
//! - Downloading package tarballs
//! - Handling different registry URLs (npm public, private registries)

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use flate2::bufread::GzDecoder;
use hmac_sha512::Hash;
use json::JsonValue;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

/// Default npm registry URL.
const DEFAULT_REGISTRY_URL: &str = "https://registry.npmjs.org/";

/// HTTP request timeout in seconds.
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Client for interacting with npm-compatible registries.
pub struct RegistryClient {
    /// The registry URL.
    pub registry_url: Url,
    /// HTTP client for making requests.
    client: reqwest::blocking::Client,
    /// Optional authentication token.
    auth_token: Option<String>,
}

impl RegistryClient {
    /// Create a new registry client with the default npm registry.
    pub fn new() -> Result<Self> {
        Self::with_url(DEFAULT_REGISTRY_URL)
    }

    /// Create a new registry client with a custom registry URL.
    pub fn with_url(url: &str) -> Result<Self> {
        let registry_url = Url::parse(url)
            .with_context(|| format!("Invalid registry URL: {}", url))?;

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            registry_url,
            client,
            auth_token: None,
        })
    }

    /// Create a registry client from package directory configuration.
    ///
    /// This reads .npmrc file and package.json to determine the registry URL.
    pub fn from_pkg_dir(pkg_dir: &Path) -> Result<Self> {
        let registry_url = get_registry_url_from_config(pkg_dir)?;
        let auth_token = get_auth_token_from_npmrc(pkg_dir, &registry_url);

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            registry_url,
            client,
            auth_token,
        })
    }

    /// Set an authentication token for private registry access.
    pub fn with_auth_token(mut self, token: String) -> Self {
        self.auth_token = Some(token);
        self
    }

    /// Check if a package exists in the registry.
    pub fn package_exists(&self, package_name: &str) -> Result<bool> {
        let url = self.registry_url.join(package_name)?;
        let response = self.make_request(&url)?;
        Ok(response.status().is_success())
    }

    /// Check if a specific version of a package exists.
    pub fn version_exists(&self, package_name: &str, version: &str) -> Result<bool> {
        let metadata = match self.get_package_metadata(package_name) {
            Ok(m) => m,
            Err(_) => return Ok(false),
        };

        Ok(metadata.versions.contains_key(version))
    }

    /// Get full package metadata from the registry.
    pub fn get_package_metadata(&self, package_name: &str) -> Result<PackageMetadata> {
        let url = self.registry_url.join(package_name)?;
        let response = self.make_request(&url)?;

        if !response.status().is_success() {
            if response.status().as_u16() == 404 {
                bail!("Package '{}' not found in registry", package_name);
            }
            bail!(
                "Failed to fetch package metadata: HTTP {}",
                response.status()
            );
        }

        let body = response.text()?;
        let json = json::parse(&body)
            .with_context(|| "Failed to parse package metadata JSON")?;

        PackageMetadata::from_json(&json)
    }

    /// Get metadata for the latest version of a package.
    pub fn get_latest_version(&self, package_name: &str) -> Result<VersionMetadata> {
        let metadata = self.get_package_metadata(package_name)?;

        let latest_tag = metadata
            .dist_tags
            .get("latest")
            .with_context(|| "No 'latest' dist-tag found")?;

        metadata
            .versions
            .get(latest_tag)
            .cloned()
            .with_context(|| format!("Version '{}' not found in versions", latest_tag))
    }

    /// Get metadata for a specific version of a package.
    pub fn get_version(&self, package_name: &str, version: &str) -> Result<VersionMetadata> {
        let metadata = self.get_package_metadata(package_name)?;

        metadata
            .versions
            .get(version)
            .cloned()
            .with_context(|| format!("Version '{}' not found", version))
    }

    /// Download a package tarball from the registry.
    ///
    /// # Arguments
    /// * `tarball_url` - The URL of the tarball to download
    /// * `expected_integrity` - Optional integrity hash (sha512) to verify
    ///
    /// # Returns
    /// The raw tarball data (gzipped).
    pub fn download_tarball(
        &self,
        tarball_url: &Url,
        expected_integrity: Option<&str>,
    ) -> Result<Vec<u8>> {
        println!("Downloading tarball from {}...", tarball_url);

        let response = self.make_request(tarball_url)?;

        if !response.status().is_success() {
            bail!(
                "Failed to download tarball: HTTP {}",
                response.status()
            );
        }

        let tarball_data = response.bytes()?.to_vec();

        // Verify integrity if provided
        if let Some(integrity) = expected_integrity {
            verify_integrity(&tarball_data, integrity)?;
            println!("Integrity verified OK");
        }

        Ok(tarball_data)
    }

    /// Download and decompress a package tarball.
    ///
    /// # Returns
    /// The decompressed tar data.
    pub fn download_and_decompress_tarball(
        &self,
        tarball_url: &Url,
        expected_integrity: Option<&str>,
    ) -> Result<Vec<u8>> {
        let tarball_data = self.download_tarball(tarball_url, expected_integrity)?;
        decompress_gzip(&tarball_data)
    }

    /// Download the latest version of a package.
    pub fn download_latest(&self, package_name: &str) -> Result<DownloadedPackage> {
        let version_metadata = self.get_latest_version(package_name)?;
        self.download_version_metadata(&version_metadata)
    }

    /// Download a specific version of a package.
    pub fn download_version(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<DownloadedPackage> {
        let version_metadata = self.get_version(package_name, version)?;
        self.download_version_metadata(&version_metadata)
    }

    /// Download a package using version metadata.
    fn download_version_metadata(
        &self,
        version_metadata: &VersionMetadata,
    ) -> Result<DownloadedPackage> {
        let tarball_url = Url::parse(&version_metadata.dist.tarball)?;
        let tar_data = self.download_and_decompress_tarball(
            &tarball_url,
            Some(&version_metadata.dist.integrity),
        )?;

        Ok(DownloadedPackage {
            name: version_metadata.name.clone(),
            version: version_metadata.version.clone(),
            tar_data,
        })
    }

    /// Make an HTTP request with optional authentication.
    fn make_request(&self, url: &Url) -> Result<reqwest::blocking::Response> {
        let mut request = self.client.get(url.as_str());

        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        request.send().with_context(|| format!("HTTP request failed: {}", url))
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default registry client")
    }
}

/// Metadata about a package from the registry.
#[derive(Debug, Clone)]
pub struct PackageMetadata {
    /// Package name.
    pub name: String,
    /// Package description.
    pub description: Option<String>,
    /// Dist tags (e.g., "latest" -> "1.0.0").
    pub dist_tags: std::collections::HashMap<String, String>,
    /// All versions of the package.
    pub versions: std::collections::HashMap<String, VersionMetadata>,
}

impl PackageMetadata {
    /// Parse package metadata from JSON.
    fn from_json(json: &JsonValue) -> Result<Self> {
        let name = json["name"]
            .as_str()
            .with_context(|| "Missing 'name' field")?
            .to_string();

        let description = json["description"].as_str().map(String::from);

        let mut dist_tags = std::collections::HashMap::new();
        if let JsonValue::Object(obj) = &json["dist-tags"] {
            for (key, value) in obj.iter() {
                if let Some(v) = value.as_str() {
                    dist_tags.insert(key.to_string(), v.to_string());
                }
            }
        }

        let mut versions = std::collections::HashMap::new();
        if let JsonValue::Object(obj) = &json["versions"] {
            for (version, version_json) in obj.iter() {
                if let Ok(metadata) = VersionMetadata::from_json(version_json) {
                    versions.insert(version.to_string(), metadata);
                }
            }
        }

        Ok(Self {
            name,
            description,
            dist_tags,
            versions,
        })
    }
}

/// Metadata about a specific version of a package.
#[derive(Debug, Clone)]
pub struct VersionMetadata {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Package description.
    pub description: Option<String>,
    /// Main entry point.
    pub main: Option<String>,
    /// Distribution information (tarball URL, integrity hash).
    pub dist: DistInfo,
    /// Dependencies.
    pub dependencies: std::collections::HashMap<String, String>,
    /// Dev dependencies.
    pub dev_dependencies: std::collections::HashMap<String, String>,
    /// Peer dependencies.
    pub peer_dependencies: std::collections::HashMap<String, String>,
}

impl VersionMetadata {
    /// Parse version metadata from JSON.
    fn from_json(json: &JsonValue) -> Result<Self> {
        let name = json["name"]
            .as_str()
            .with_context(|| "Missing 'name' field")?
            .to_string();

        let version = json["version"]
            .as_str()
            .with_context(|| "Missing 'version' field")?
            .to_string();

        let description = json["description"].as_str().map(String::from);
        let main = json["main"].as_str().map(String::from);

        let dist = DistInfo::from_json(&json["dist"])?;

        let dependencies = parse_dependencies(&json["dependencies"]);
        let dev_dependencies = parse_dependencies(&json["devDependencies"]);
        let peer_dependencies = parse_dependencies(&json["peerDependencies"]);

        Ok(Self {
            name,
            version,
            description,
            main,
            dist,
            dependencies,
            dev_dependencies,
            peer_dependencies,
        })
    }
}

/// Distribution information for a package version.
#[derive(Debug, Clone)]
pub struct DistInfo {
    /// URL to download the tarball.
    pub tarball: String,
    /// Integrity hash (e.g., "sha512-...").
    pub integrity: String,
    /// SHA sum (legacy).
    pub shasum: Option<String>,
}

impl DistInfo {
    /// Parse distribution info from JSON.
    fn from_json(json: &JsonValue) -> Result<Self> {
        let tarball = json["tarball"]
            .as_str()
            .with_context(|| "Missing 'tarball' field in dist")?
            .to_string();

        let integrity = json["integrity"]
            .as_str()
            .with_context(|| "Missing 'integrity' field in dist")?
            .to_string();

        let shasum = json["shasum"].as_str().map(String::from);

        Ok(Self {
            tarball,
            integrity,
            shasum,
        })
    }
}

/// A downloaded package.
#[derive(Debug)]
pub struct DownloadedPackage {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Decompressed tar data.
    pub tar_data: Vec<u8>,
}

impl DownloadedPackage {
    /// Load a file from the downloaded package.
    pub fn load_file(&self, file_path: &Path) -> Result<Option<Vec<u8>>> {
        use tar::Archive;

        let mut archive = Archive::new(&self.tar_data[..]);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let entry_path = entry
                .header()
                .path()?
                .strip_prefix("package")
                .unwrap_or(&entry.header().path()?)
                .to_path_buf();

            let normalized_path = normalize_path(file_path);

            if entry_path == normalized_path {
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer)?;
                return Ok(Some(buffer));
            }
        }

        Ok(None)
    }

    /// Get package.json from the downloaded package.
    pub fn get_package_json(&self) -> Result<JsonValue> {
        let data = self
            .load_file(Path::new("package.json"))?
            .with_context(|| "package.json not found in tarball")?;

        let content = String::from_utf8(data)?;
        json::parse(&content).with_context(|| "Failed to parse package.json")
    }
}

/// Get registry URL from package directory configuration.
fn get_registry_url_from_config(pkg_dir: &Path) -> Result<Url> {
    // Check .npmrc file first
    let npmrc_path = pkg_dir.join(".npmrc");
    if npmrc_path.is_file() {
        let npmrc = fs::read_to_string(&npmrc_path)?;
        for line in npmrc.lines() {
            let line = line.trim();
            if line.starts_with("registry=") {
                let url = line.strip_prefix("registry=").unwrap();
                return Url::parse(url).with_context(|| format!("Invalid registry URL in .npmrc: {}", url));
            }
        }
    }

    // Check package.json for publishConfig.registry
    let pkg_json_path = pkg_dir.join("package.json");
    if pkg_json_path.is_file() {
        let content = fs::read_to_string(&pkg_json_path)?;
        let pkg_json = json::parse(&content)?;

        if let Some(registry) = pkg_json["publishConfig"]["registry"].as_str() {
            return Url::parse(registry)
                .with_context(|| format!("Invalid registry URL in package.json: {}", registry));
        }
    }

    // Fall back to default
    Url::parse(DEFAULT_REGISTRY_URL).context("Invalid default registry URL")
}

/// Get authentication token from .npmrc file.
fn get_auth_token_from_npmrc(pkg_dir: &Path, registry_url: &Url) -> Option<String> {
    let npmrc_paths = [
        pkg_dir.join(".npmrc"),
        dirs_home().map(|h| h.join(".npmrc")).unwrap_or_default(),
    ];

    for npmrc_path in npmrc_paths {
        if !npmrc_path.is_file() {
            continue;
        }

        if let Ok(npmrc) = fs::read_to_string(&npmrc_path) {
            // Look for //registry.example.com/:_authToken=xxx
            let registry_host = registry_url.host_str().unwrap_or("");
            for line in npmrc.lines() {
                let line = line.trim();
                if line.contains(registry_host) && line.contains(":_authToken=") {
                    if let Some(token) = line.split(":_authToken=").last() {
                        return Some(token.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Get the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Parse dependencies from JSON object.
fn parse_dependencies(json: &JsonValue) -> std::collections::HashMap<String, String> {
    let mut deps = std::collections::HashMap::new();

    if let JsonValue::Object(obj) = json {
        for (key, value) in obj.iter() {
            if let Some(v) = value.as_str() {
                deps.insert(key.to_string(), v.to_string());
            }
        }
    }

    deps
}

/// Verify the integrity of downloaded data.
fn verify_integrity(data: &[u8], integrity: &str) -> Result<()> {
    let parts: Vec<&str> = integrity.split('-').collect();

    if parts.len() != 2 {
        bail!("Invalid integrity format: {}", integrity);
    }

    let algorithm = parts[0];
    let expected_hash = parts[1];

    if algorithm != "sha512" {
        bail!(
            "Unsupported integrity algorithm: {}. Only sha512 is supported.",
            algorithm
        );
    }

    let expected_bytes = BASE64_STANDARD
        .decode(expected_hash)
        .with_context(|| "Invalid base64 in integrity hash")?;

    let mut hasher = Hash::new();
    hasher.update(data);
    let actual_hash = hasher.finalize();

    if actual_hash.as_slice() != expected_bytes.as_slice() {
        bail!("Integrity check failed: hash mismatch");
    }

    Ok(())
}

/// Decompress gzipped data.
fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(data);
    let mut buffer = Vec::new();
    decoder
        .read_to_end(&mut buffer)
        .with_context(|| "Failed to decompress gzip data")?;
    Ok(buffer)
}

/// Normalize a path by removing leading "./" or "/".
fn normalize_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix("./") {
        PathBuf::from(stripped)
    } else if let Some(stripped) = path_str.strip_prefix('/') {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path(Path::new("./foo.js")),
            PathBuf::from("foo.js")
        );
        assert_eq!(
            normalize_path(Path::new("/foo.js")),
            PathBuf::from("foo.js")
        );
        assert_eq!(
            normalize_path(Path::new("foo.js")),
            PathBuf::from("foo.js")
        );
    }

    #[test]
    fn test_verify_integrity_format() {
        // Test invalid format
        let result = verify_integrity(b"test", "invalid");
        assert!(result.is_err());

        // Test unsupported algorithm
        let result = verify_integrity(b"test", "sha256-abc");
        assert!(result.is_err());
    }
}
