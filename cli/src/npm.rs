use crate::pkg::Pkg;
use base64::{engine::general_purpose::STANDARD as BASE_64_STANDARD, Engine as _};
use hmac_sha512::Hash;
use std::path::PathBuf;
use std::{fs, io};
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum NpmError {
    #[error("JSON error: {0}")]
    Json(#[from] json::JsonError),
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Base64 Decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Validation error: {0}")]
    Validation(String),
}

pub fn fetch_latest_of(pkg: &Pkg) -> Result<Pkg, NpmError> {
    let (version, dir_path, tarball_checksum, tarball_url) = fetch_info_from_registry(&pkg)?;

    let tarball_url = Url::parse(tarball_url.as_str())?;
    let tarball_checksum = BASE_64_STANDARD.decode(tarball_checksum)?;
    let tarball_out_name = format!("{}-{}.tar.gz", pkg.name, version);

    download_tarball_if_needed(
        &dir_path,
        &tarball_out_name,
        &tarball_url,
        &tarball_checksum,
    )?;

    Ok(Pkg {
        version,
        dir_path,
        ..pkg.clone()
    })
}

fn fetch_info_from_registry(pkg: &Pkg) -> Result<(String, PathBuf, String, String), NpmError> {
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

    let dist = &response_body["versions"][latest_version.to_string()]["dist"];
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

    let pkg_dir_path = pkg.dir_path.join(".tmp");
    let pkg_version = latest_version.to_string();

    Ok((
        pkg_version,
        pkg_dir_path,
        tarball_hash_integrity.to_string(),
        tarball_url.to_string(),
    ))
}

fn download_tarball_if_needed(
    output_dir: &PathBuf,
    file_name: &String,
    url: &Url,
    checksum: &Vec<u8>,
) -> Result<(), NpmError> {
    let output_path = output_dir.join(file_name);

    if output_path.is_file() {
        let tarball_data = fs::read(&output_path)?;

        if is_tarball_integrity_ok(&tarball_data, &checksum) {
            println!("Valid tarball exists on file system. Will use existing...");

            return Ok(());
        }

        println!("Found existing tarball but integrity check failed. Will remove existing...");
        fs::remove_file(&output_path)?;
    }

    println!("Downloading tarball from registry...");

    let response = reqwest::blocking::get(url.as_str())?.error_for_status();

    if let Err(error) = response {
        return Err(NpmError::Request(error));
    }

    let tarball_data = response.unwrap().bytes()?;

    if !is_tarball_integrity_ok(&tarball_data.to_vec(), &checksum) {
        return Err(NpmError::Validation(
            "Could not verify integrity of downloaded tarball.".into(),
        ));
    }

    println!("Integrity OK, storing on the file system...");

    if !output_dir.is_dir() {
        fs::create_dir(&output_dir)?;
    }

    fs::write(output_path, tarball_data)?;

    Ok(())
}

fn is_tarball_integrity_ok(buffer: &Vec<u8>, checksum: &Vec<u8>) -> bool {
    let mut hash = Hash::new();

    hash.update(&buffer);
    hash.finalize().eq(checksum.as_slice())
}
