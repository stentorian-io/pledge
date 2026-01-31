use crate::pkg::contents::PkgContents;
use crate::pkg::entries::PkgEntries;
use crate::pkg::pack::{detect_package_manager, pack_package, PackResult};
use crate::pkg::registry_client::RegistryClient;
use crate::pkg::tarball::PkgTarball;
use crate::pkg::Pkg;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use json::JsonValue;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use url::Url;

pub fn load_from_dir(pkg_dir: PathBuf) -> Result<Pkg> {
    // Canonicalize the path to ensure consistent path handling
    // This fixes issues with relative paths and glob pattern matching
    let pkg_dir = pkg_dir.canonicalize()
        .with_context(|| format!("Failed to canonicalize package directory: {}", pkg_dir.display()))?;

    let pkg_json = Pkg::parse_config_in_dir(&pkg_dir)
        .with_context(|| "Failed to load and parse package.json config.")?;

    let pkg_registry_url = Pkg::get_registry_url(&pkg_dir)
        .with_context(|| "Failed to determine package registry URL.")?;

    println!("Will use {} as registry.", &pkg_registry_url);

    let pkg_contents = PkgContents::new(pkg_dir.to_owned(), &pkg_json, None)
        .with_context(|| "Failed to create package contents from file system.")?;

    let pkg_contents = Rc::new(pkg_contents);

    let pkg_entries = PkgEntries::new(&pkg_json, Rc::clone(&pkg_contents))
        .with_context(|| "Failed to create package entries.")?;

    Ok(Pkg::new(
        pkg_dir,
        pkg_json,
        pkg_registry_url,
        pkg_contents,
        pkg_entries,
    ))
}

pub fn fetch_from_server(local_pkg: &Pkg) -> Result<Pkg> {
    let pkg_dir = &local_pkg.dir;
    let pkg_dir_tmp = pkg_dir.join(".tmp");
    let pkg_registry_url = local_pkg.registry_url.to_owned();

    let tarball = fetch_last_published_tarball_of(&pkg_dir_tmp, local_pkg)
        .with_context(|| "Failed to fetch last published tarball from registry.")?;

    let pkg = download_and_unpack_pkg_tarball(pkg_dir.to_owned(), pkg_registry_url, tarball)
        .with_context(|| "Failed to download and unpack package tarball from registry.")?;

    Ok(pkg)
}

fn fetch_last_published_tarball_of(pkg_dir: &PathBuf, local_pkg: &Pkg) -> Result<PkgTarball> {
    let pkg_data_latest = fetch_latest_pkg_info_for(local_pkg)
        .with_context(|| "Failed to request package information from registry.")?;

    let pkg_version_latest = &pkg_data_latest["dist-tags"]["latest"];

    if !pkg_version_latest.is_string() {
        bail!("Unexpected latest dist-tag value for latest package.");
    }

    let pkg_version_latest = pkg_version_latest.to_string();
    let pkg_tarball_name = format!("{}-{}.tar.gz", local_pkg.name, pkg_version_latest);

    let pkg_dist = &pkg_data_latest["versions"][&pkg_version_latest]["dist"];
    let pkg_tarball = get_pkg_tarball_from_dist(pkg_tarball_name, pkg_dir, pkg_dist)
        .with_context(|| "Failed to extract tarball info from latest version dist response.")?;

    Ok(pkg_tarball)
}

fn download_and_unpack_pkg_tarball(
    pkg_dir: PathBuf,
    pkg_registry_url: Url,
    mut pkg_tarball: PkgTarball,
) -> Result<Pkg> {
    pkg_tarball
        .download_if_needed()
        .with_context(|| "Failed to download tarball or load from local cache.")?;

    let pkg_json = get_pkg_json_from_tarball(&mut pkg_tarball)?;

    let pkg_contents = PkgContents::new(pkg_dir.to_owned(), &pkg_json, Some(pkg_tarball))
        .with_context(|| "Failed to create package contents with tarball.")?;

    let pkg_contents = Rc::new(pkg_contents);

    let pkg_entries = PkgEntries::new(&pkg_json, Rc::clone(&pkg_contents))
        .with_context(|| "Failed to create package entries.")?;

    Ok(Pkg::new(
        pkg_dir,
        pkg_json,
        pkg_registry_url,
        pkg_contents,
        pkg_entries,
    ))
}

fn get_pkg_json_from_tarball(pkg_tarball: &mut PkgTarball) -> Result<JsonValue> {
    let path = PathBuf::from("package.json");
    let data = pkg_tarball.load_file_by_path(&path)?;
    let data = String::from_utf8(data.unwrap())?;

    Pkg::parse_config_as_json(data)
}

fn fetch_latest_pkg_info_for(pkg: &Pkg) -> Result<JsonValue> {
    let request_url = &pkg.registry_url.join(&pkg.name)?;
    let response = reqwest::blocking::get(request_url.to_string())?.error_for_status();

    if let Err(error) = response {
        bail!(error);
    }

    let response_body = response.unwrap().text()?;
    let response_body = json::parse(&response_body)?;

    Ok(response_body)
}

fn get_pkg_tarball_from_dist(name: String, dir: &PathBuf, dist: &JsonValue) -> Result<PkgTarball> {
    let tarball_url = &dist["tarball"];
    let tarball_checksum = &dist["integrity"];

    if !tarball_url.is_string() {
        bail!("Couldn't find tarball URL for latest package.");
    } else if !tarball_checksum.is_string() {
        bail!("Couldn't find tarball checksum for latest package.");
    }

    let tarball_checksum = tarball_checksum.to_string();
    let tarball_integrity_parts: Vec<&str> = tarball_checksum.split('-').collect();

    if tarball_integrity_parts.len().ne(&2) {
        bail!("Unexpected integrity string format for latest package.");
    }

    let tarball_hash_algorithm = tarball_integrity_parts.first().unwrap();
    let tarball_hash_integrity = tarball_integrity_parts.last().unwrap();

    if tarball_hash_algorithm.ne(&"sha512") {
        bail!("Package integrity can only be verified with SHA-512.");
    }

    let tarball_url = Url::parse(tarball_url.as_str().unwrap())?;
    let tarball_checksum = BASE64_STANDARD.decode(tarball_hash_integrity)?;

    PkgTarball::new(name, dir.to_owned(), tarball_url, tarball_checksum)
}

// ============================================================================
// New API: Native packing and enhanced registry operations
// ============================================================================

/// Load a package from a directory using native pack command.
///
/// This function:
/// 1. Detects the package manager (npm, pnpm, yarn, bun)
/// 2. Runs the native pack command to create a tarball
/// 3. Loads the package from the resulting tarball
///
/// This is more accurate than reading files directly as it respects
/// the package manager's file inclusion/exclusion rules.
pub fn load_from_dir_with_native_pack(pkg_dir: PathBuf) -> Result<PackedPkg> {
    let pkg_dir = pkg_dir
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize package directory: {}", pkg_dir.display()))?;

    // Check that package.json exists
    let pkg_json_path = pkg_dir.join("package.json");
    if !pkg_json_path.is_file() {
        bail!(
            "No package.json found in directory: {}",
            pkg_dir.display()
        );
    }

    // Detect and check package manager
    let pm_info = detect_package_manager(&pkg_dir)
        .with_context(|| "Failed to detect package manager")?;

    println!(
        "Using {} {} for packing",
        pm_info.manager.name(),
        pm_info.version.as_deref().unwrap_or("(version unknown)")
    );

    // Run native pack command
    let pack_result = pack_package(&pkg_dir)
        .with_context(|| "Failed to pack package")?;

    // Load package.json from pack result
    let pkg_json_data = pack_result
        .load_file(Path::new("package.json"))?
        .with_context(|| "package.json not found in pack result")?;

    let pkg_json_str = String::from_utf8(pkg_json_data)?;
    let pkg_json = Pkg::parse_config_as_json(pkg_json_str)?;

    // Get registry URL
    let registry_url = Pkg::get_registry_url(&pkg_dir)?;

    Ok(PackedPkg {
        name: pkg_json["name"].to_string(),
        version: pkg_json["version"].to_string(),
        dir: pkg_dir,
        registry_url,
        pack_result,
        pkg_json,
    })
}

/// A package loaded via native pack command.
#[derive(Debug)]
pub struct PackedPkg {
    /// Package name from package.json.
    pub name: String,
    /// Package version from package.json.
    pub version: String,
    /// Directory the package was loaded from.
    pub dir: PathBuf,
    /// Registry URL for this package.
    pub registry_url: Url,
    /// The pack result containing tarball data.
    pub pack_result: PackResult,
    /// Parsed package.json.
    pub pkg_json: JsonValue,
}

impl PackedPkg {
    /// Load a file from the packed tarball.
    pub fn load_file(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        self.pack_result.load_file(path)
    }

    /// Get all files in the packed tarball.
    pub fn get_files(&self) -> Result<Vec<crate::pkg::pack::PackedFile>> {
        self.pack_result.get_files()
    }

    /// Convert to a Pkg struct for use with existing APIs.
    pub fn to_pkg(self) -> Result<Pkg> {
        let tarball = PackResultTarball {
            name: self.name.clone(),
            version: self.version.clone(),
            pack_result: self.pack_result,
        };

        let pkg_contents = PkgContents::new(
            self.dir.clone(),
            &self.pkg_json,
            Some(tarball.to_pkg_tarball(&self.dir)?),
        )
        .with_context(|| "Failed to create package contents from pack result")?;

        let pkg_contents = Rc::new(pkg_contents);

        let pkg_entries = PkgEntries::new(&self.pkg_json, Rc::clone(&pkg_contents))
            .with_context(|| "Failed to create package entries")?;

        Ok(Pkg::new(
            self.dir,
            self.pkg_json,
            self.registry_url,
            pkg_contents,
            pkg_entries,
        ))
    }
}

/// Wrapper to convert PackResult to PkgTarball.
struct PackResultTarball {
    name: String,
    version: String,
    pack_result: PackResult,
}

impl PackResultTarball {
    fn to_pkg_tarball(&self, pkg_dir: &Path) -> Result<PkgTarball> {
        use hmac_sha512::Hash;

        let tmp_dir = pkg_dir.join(".tmp");
        if !tmp_dir.is_dir() {
            std::fs::create_dir_all(&tmp_dir)?;
        }

        // Calculate checksum from the gzipped tarball data
        let mut hasher = Hash::new();
        hasher.update(&self.pack_result.tarball_data);
        let checksum = hasher.finalize().to_vec();

        // Write tarball to temp file
        let tarball_name = format!("{}-{}.tgz", self.name.replace('/', "-").replace('@', ""), self.version);
        let tarball_path = tmp_dir.join(&tarball_name);
        std::fs::write(&tarball_path, &self.pack_result.tarball_data)?;

        // Create PkgTarball
        let tarball_url = Url::parse(&format!("file://{}", tarball_path.display()))?;
        let mut tarball = PkgTarball::new(tarball_name, tmp_dir, tarball_url, checksum)?;

        // Load the data
        tarball.download_if_needed()?;

        Ok(tarball)
    }
}

/// Check if a package version exists in the registry.
///
/// # Arguments
/// * `pkg_dir` - Package directory (used to determine registry URL)
/// * `package_name` - Name of the package to check
/// * `version` - Version to check for (or None to check if package exists at all)
pub fn version_exists_in_registry(
    pkg_dir: &Path,
    package_name: &str,
    version: Option<&str>,
) -> Result<bool> {
    let client = RegistryClient::from_pkg_dir(pkg_dir)
        .with_context(|| "Failed to create registry client")?;

    match version {
        Some(v) => client.version_exists(package_name, v),
        None => client.package_exists(package_name),
    }
}

/// Download a specific version of a package from the registry.
///
/// Returns None if the package/version doesn't exist.
pub fn download_version_from_registry(
    pkg_dir: &Path,
    package_name: &str,
    version: &str,
) -> Result<Option<Pkg>> {
    let client = RegistryClient::from_pkg_dir(pkg_dir)
        .with_context(|| "Failed to create registry client")?;

    // Check if version exists
    if !client.version_exists(package_name, version)? {
        return Ok(None);
    }

    // Download the version
    let downloaded = client
        .download_version(package_name, version)
        .with_context(|| format!("Failed to download {} v{}", package_name, version))?;

    // Convert to Pkg
    let pkg = downloaded_to_pkg(pkg_dir, &client.registry_url, downloaded)?;

    Ok(Some(pkg))
}

/// Convert a downloaded package to a Pkg struct.
fn downloaded_to_pkg(
    pkg_dir: &Path,
    registry_url: &Url,
    downloaded: crate::pkg::registry_client::DownloadedPackage,
) -> Result<Pkg> {
    let pkg_json = downloaded.get_package_json()?;

    // Create a temporary tarball wrapper
    let tarball = TarballData {
        name: downloaded.name.clone(),
        version: downloaded.version.clone(),
        tar_data: downloaded.tar_data,
    };

    let pkg_contents = PkgContents::new(
        pkg_dir.to_path_buf(),
        &pkg_json,
        Some(tarball.to_pkg_tarball(pkg_dir)?),
    )
    .with_context(|| "Failed to create package contents")?;

    let pkg_contents = Rc::new(pkg_contents);

    let pkg_entries = PkgEntries::new(&pkg_json, Rc::clone(&pkg_contents))
        .with_context(|| "Failed to create package entries")?;

    Ok(Pkg::new(
        pkg_dir.to_path_buf(),
        pkg_json,
        registry_url.clone(),
        pkg_contents,
        pkg_entries,
    ))
}

/// Temporary struct to hold downloaded tarball data.
struct TarballData {
    name: String,
    version: String,
    tar_data: Vec<u8>,
}

impl TarballData {
    /// Convert to PkgTarball by writing to a temp file.
    fn to_pkg_tarball(&self, pkg_dir: &Path) -> Result<PkgTarball> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use hmac_sha512::Hash;
        use std::io::Write;

        let tmp_dir = pkg_dir.join(".tmp");
        if !tmp_dir.is_dir() {
            std::fs::create_dir_all(&tmp_dir)?;
        }

        // Compress tar data back to gzip for PkgTarball
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&self.tar_data)?;
        let gzipped = encoder.finish()?;

        // Calculate checksum
        let mut hasher = Hash::new();
        hasher.update(&gzipped);
        let checksum = hasher.finalize().to_vec();

        // Write to file
        let tarball_name = format!("{}-{}.tgz", self.name, self.version);
        let tarball_path = tmp_dir.join(&tarball_name);
        std::fs::write(&tarball_path, &gzipped)?;

        // Create PkgTarball (it will validate and decompress when needed)
        let tarball_url = Url::parse(&format!("file://{}", tarball_path.display()))?;
        let mut tarball = PkgTarball::new(tarball_name, tmp_dir, tarball_url, checksum)?;

        // Trigger download/load which will decompress
        tarball.download_if_needed()?;

        Ok(tarball)
    }
}

/// Fetch the previous version of a package using the enhanced registry client.
///
/// This is an alternative to `fetch_from_server` that uses the new RegistryClient.
pub fn fetch_previous_version(local_pkg: &Pkg) -> Result<Pkg> {
    let client = RegistryClient::from_pkg_dir(&local_pkg.dir)
        .with_context(|| "Failed to create registry client")?;

    println!("Fetching latest version from {}...", client.registry_url);

    let downloaded = client
        .download_latest(&local_pkg.name)
        .with_context(|| format!("Failed to download latest version of {}", local_pkg.name))?;

    println!("Downloaded {} v{}", downloaded.name, downloaded.version);

    downloaded_to_pkg(&local_pkg.dir, &client.registry_url, downloaded)
}
