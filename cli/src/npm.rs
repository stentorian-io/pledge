use crate::pkg::Pkg;
use base64::{engine::general_purpose::STANDARD as BASE_64_STANDARD, Engine as _};
use flate2::bufread::GzDecoder;
use glob::Pattern;
use hmac_sha512::Hash;
use std::collections::HashSet;
use std::path::PathBuf;
use std::{fs, io};
use tar::Archive;
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
    #[error("Pattern error: {0}")]
    Pattern(#[from] glob::PatternError),
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Validation error: {0}")]
    Validation(String),
}

pub fn fetch_latest_of(pkg: &Pkg) -> Result<Pkg, NpmError> {
    let (version, dir_path, tarball_checksum, tarball_url) = fetch_info_from_registry(&pkg)?;

    let name = pkg.name.clone();
    let registry_url = pkg.registry_url.clone();
    let tarball_url = Url::parse(tarball_url.as_str())?;
    let tarball_checksum = BASE_64_STANDARD.decode(tarball_checksum)?;
    let files: HashSet<PathBuf> = HashSet::new();

    let mut latest_pkg = Pkg {
        name,
        version,
        dir_path,
        registry_url,
        files,
    };

    download_tarball_if_needed(&latest_pkg, &tarball_url, &tarball_checksum)?;
    unpack_tarball_into(&mut latest_pkg)?;

    Ok(latest_pkg)
}

pub fn resolve_pkg_contents_into(
    pkg: &mut Pkg,
    include_patterns: Vec<&str>,
) -> Result<(), NpmError> {
    let include_patterns = if include_patterns.is_empty() {
        to_pkg_path_patterns(&pkg, vec!["**/*"])?
    } else {
        to_pkg_path_patterns(&pkg, include_patterns)?
    };

    // See https://docs.npmjs.com/cli/v10/configuring-npm/package-json#files
    let exclude_patterns: Vec<Pattern> = to_pkg_path_patterns(
        &pkg,
        vec![
            ".git",
            ".npmrc",
            "node_modules",
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
        ],
    )?;

    resolve_pkg_dir_contents(
        pkg,
        &pkg.dir_path.to_owned(),
        &exclude_patterns,
        &include_patterns,
    )?;

    Ok(())
}

fn resolve_pkg_dir_contents(
    pkg: &mut Pkg,
    dir: &PathBuf,
    exclude_patterns: &Vec<Pattern>,
    include_patterns: &Vec<Pattern>,
) -> Result<(), NpmError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry.unwrap();
        let entry_path = entry.path();
        let entry_type = entry.file_type()?;

        if path_matches_a_pattern_in(&entry_path, &exclude_patterns) {
            continue;
        }

        if entry_type.is_dir() {
            return resolve_pkg_dir_contents(pkg, &entry_path, exclude_patterns, include_patterns);
        }

        if path_matches_a_pattern_in(&entry_path, &include_patterns) {
            pkg.files.insert(entry_path.to_owned());
        }
    }

    Ok(())
}

fn to_pkg_path_patterns(pkg: &Pkg, paths: Vec<&str>) -> Result<Vec<Pattern>, NpmError> {
    let mut patterns: Vec<Pattern> = Vec::with_capacity(paths.len());

    for path in paths {
        let path = &pkg.dir_path.join(path);
        let path = path.to_str().unwrap();

        patterns.push(Pattern::new(path)?);
    }

    Ok(patterns)
}

fn path_matches_a_pattern_in(path: &PathBuf, patterns: &Vec<Pattern>) -> bool {
    patterns.iter().any(|pattern| pattern.matches_path(&path))
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

fn download_tarball_if_needed(pkg: &Pkg, url: &Url, checksum: &Vec<u8>) -> Result<(), NpmError> {
    let output_path = pkg.dir_path.join(pkg.get_tarball_name());

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

    if !pkg.dir_path.is_dir() {
        fs::create_dir(&pkg.dir_path)?;
    }

    fs::write(output_path, tarball_data)?;

    Ok(())
}

fn is_tarball_integrity_ok(buffer: &Vec<u8>, checksum: &Vec<u8>) -> bool {
    let mut hash = Hash::new();

    hash.update(&buffer);
    hash.finalize().eq(checksum.as_slice())
}

fn unpack_tarball_into(pkg: &mut Pkg) -> Result<(), NpmError> {
    let tarball_name = pkg.get_tarball_name();
    let tarball_path = pkg.dir_path.join(tarball_name);

    let tarball_buffer = fs::read(&tarball_path)?;
    let tarball_decoder = GzDecoder::new(tarball_buffer.as_slice());
    let mut tarball = Archive::new(tarball_decoder);

    for entry in tarball.entries()? {
        pkg.files.insert(entry?.header().path()?.to_path_buf());
    }

    Ok(())
}
