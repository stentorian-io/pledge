use crate::pkg::error::PkgError;
use crate::pkg::Pkg;
use base64::{engine::general_purpose::STANDARD as BASE_64_STANDARD, Engine as _};
use flate2::bufread::GzDecoder;
use hmac_sha512::Hash;
use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;
use std::{fs, io};
use tar::Archive;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum NpmError {
    #[error("Package error: {0}")]
    Pkg(#[from] Box<PkgError>),
    #[error("JSON error: {0}")]
    Json(#[from] json::JsonError),
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Base64 Decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Pattern error: {0}")]
    Pattern(#[from] glob::PatternError),
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Validation error: {0}")]
    Validation(String),
}

impl From<PkgError> for NpmError {
    fn from(error: PkgError) -> Self {
        NpmError::Pkg(Box::new(error))
    }
}

pub fn fetch_latest_of(pkg: &Pkg) -> Result<Pkg, NpmError> {
    let pkg_dir = pkg.dir_path.join(".tmp");
    let pkg_registry_url = pkg.registry_url.clone();

    let (latest_version, tarball_url, tarball_checksum) =
        fetch_latest_pkg_info_from_registry(&pkg)?;

    let tarball_name = format!("{}-{}.tar.gz", pkg.name, latest_version);
    let tarball_info = (tarball_name, tarball_url, tarball_checksum);
    let tarball_path = download_tarball_if_needed(&pkg_dir, tarball_info)?;

    let mut pkg_files = HashSet::new();
    let mut pkg_config = String::new();

    decode_and_unpack_tarball(&tarball_path, &mut pkg_config, &mut pkg_files)?;

    let pkg_json = Pkg::parse_config_as_json(pkg_config)?;
    let mut pkg_latest = Pkg::new(pkg_dir, pkg_json, pkg_registry_url)?;

    pkg_latest.contents.resolved_files = pkg_files;

    Ok(pkg_latest)
}

fn fetch_latest_pkg_info_from_registry(pkg: &Pkg) -> Result<(String, Url, Vec<u8>), NpmError> {
    let request_url = &pkg.registry_url.join(&pkg.name)?;
    let response = reqwest::blocking::get(request_url.to_string())?;

    if !response.status().is_success() {
        return Err(NpmError::Validation(format!(
            "Failed to fetch package information from registry: {}.",
            response.status()
        )));
    }

    let response_body = response.text()?;
    let response_body = json::parse(&response_body)?;

    let dist_tags = &response_body["dist-tags"];

    if !dist_tags.is_object() {
        return Err(NpmError::Validation(
            "Registry package missing dist-tags information.".to_string(),
        ));
    }

    let latest_version = &dist_tags["latest"];

    if !latest_version.is_string() {
        return Err(NpmError::Validation(
            "Unexpected latest dist-tag value for latest package".to_string(),
        ));
    }

    let latest_version = latest_version.to_string();
    let dist = &response_body["versions"][&latest_version]["dist"];
    let tarball_url = &dist["tarball"];
    let tarball_checksum = &dist["integrity"];

    if !tarball_url.is_string() {
        return Err(NpmError::Validation(
            "Couldn't find tarball URL for latest package.".to_string(),
        ));
    }

    if !tarball_checksum.is_string() {
        return Err(NpmError::Validation(
            "Couldn't find tarball checksum for latest package.".to_string(),
        ));
    }

    let tarball_checksum = tarball_checksum.to_string();
    let tarball_integrity_parts: Vec<&str> = tarball_checksum.split('-').collect();

    if tarball_integrity_parts.len().ne(&2) {
        return Err(NpmError::Validation(
            "Unexpected integrity string format for latest package.".into(),
        ));
    }

    let tarball_hash_algorithm = tarball_integrity_parts.first().unwrap();
    let tarball_hash_integrity = tarball_integrity_parts.last().unwrap();

    if tarball_hash_algorithm.ne(&"sha512") {
        return Err(NpmError::Validation(
            "Package integrity can only be verified with SHA-512.".into(),
        ));
    }

    let tarball_url = Url::parse(tarball_url.as_str().unwrap())?;
    let tarball_checksum = BASE_64_STANDARD.decode(tarball_hash_integrity)?;

    Ok((latest_version, tarball_url, tarball_checksum))
}

fn download_tarball_if_needed(
    output_dir: &PathBuf,
    tarball_info: (String, Url, Vec<u8>),
) -> Result<PathBuf, NpmError> {
    let (tarball_name, tarball_url, tarball_checksum) = tarball_info;
    let tarball_path = output_dir.join(tarball_name);

    if tarball_path.is_file() {
        let tarball_data = fs::read(&tarball_path)?;

        if is_tarball_integrity_ok(&tarball_data, &tarball_checksum) {
            println!("Valid tarball exists on file system. Will use existing...");

            return Ok(tarball_path);
        }

        println!("Found existing tarball but integrity check failed. Will remove existing...");
        fs::remove_file(&tarball_path)?;
    }

    println!("Downloading tarball from registry...");

    let response = reqwest::blocking::get(tarball_url.as_str())?.error_for_status();

    if let Err(error) = response {
        return Err(NpmError::Request(error));
    }

    let tarball_data = response.unwrap().bytes()?;

    if !is_tarball_integrity_ok(&tarball_data.to_vec(), &tarball_checksum) {
        return Err(NpmError::Validation(
            "Could not verify integrity of downloaded tarball.".into(),
        ));
    }

    println!("Integrity OK, storing on the file system...");

    if !output_dir.is_dir() {
        fs::create_dir(&output_dir)?;
    }

    fs::write(&tarball_path, tarball_data)?;

    Ok(tarball_path)
}

fn is_tarball_integrity_ok(buffer: &Vec<u8>, checksum: &Vec<u8>) -> bool {
    let mut hash = Hash::new();

    hash.update(&buffer);
    hash.finalize().eq(checksum.as_slice())
}

fn decode_and_unpack_tarball(
    tarball_path: &PathBuf,
    pkg_config_buffer: &mut String,
    pkg_files: &mut HashSet<PathBuf>,
) -> Result<(), NpmError> {
    let tarball_buffer = fs::read(&tarball_path)?;
    let tarball_decoder = GzDecoder::new(tarball_buffer.as_slice());
    let mut tarball_data = Archive::new(tarball_decoder);

    for entry in tarball_data.entries()? {
        let mut entry = entry.unwrap();
        let entry_path = entry.header().path()?.to_path_buf();
        let entry_name = entry_path.file_name().unwrap();

        if entry_name.eq("package.json") {
            entry.read_to_string(pkg_config_buffer)?;
        }

        pkg_files.insert(entry_path);
    }

    Ok(())
}
