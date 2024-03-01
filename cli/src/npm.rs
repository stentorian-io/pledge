use crate::pkg::tarball::PkgTarball;
use crate::pkg::Pkg;
use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as BASE_64_STANDARD, Engine as _};
use json::JsonValue;
use std::collections::HashSet;
use std::path::PathBuf;
use url::Url;

pub fn load_from_dir(dir: PathBuf) -> Result<Pkg> {
    let config_json = Pkg::parse_config_in_dir(&dir)?;
    let registry_url = Pkg::get_registry_url(&dir)?;

    println!("Will use {} as registry.", &registry_url);

    let pkg = Pkg::new(dir, config_json, registry_url, HashSet::new())?;
    let pkg = pkg.resolve_dir_contents()?;

    Ok(pkg)
}

pub fn fetch_from_registry(local_pkg: &Pkg) -> Result<Pkg> {
    let pkg_dir = local_pkg.dir_path.join(".tmp");
    let pkg_registry_url = local_pkg.registry_url.clone();

    let tarball = fetch_last_published_tarball_of(&pkg_dir, local_pkg)?;
    let pkg = unpack_tarball_as_pkg(pkg_dir, pkg_registry_url, tarball)?;

    Ok(pkg)
}

fn fetch_last_published_tarball_of(pkg_dir: &PathBuf, local_pkg: &Pkg) -> Result<PkgTarball> {
    let pkg_data_latest = fetch_latest_pkg_info_for(local_pkg)?;
    let pkg_version_latest = &pkg_data_latest["dist-tags"]["latest"];

    if !pkg_version_latest.is_string() {
        bail!("Unexpected latest dist-tag value for latest package.");
    }

    let pkg_version_latest = pkg_version_latest.to_string();
    let pkg_tarball_name = format!("{}-{}.tar.gz", local_pkg.name, pkg_version_latest);

    let pkg_dist = &pkg_data_latest["versions"][&pkg_version_latest]["dist"];
    let pkg_tarball = get_pkg_tarball_from_dist(pkg_tarball_name, pkg_dir, pkg_dist)?;

    Ok(pkg_tarball)
}

pub fn unpack_tarball_as_pkg(
    pkg_dir: PathBuf,
    pkg_registry_url: Url,
    pkg_tarball: PkgTarball,
) -> Result<Pkg> {
    let mut pkg_files = HashSet::new();
    let mut pkg_config = String::new();

    pkg_tarball.download_to_disk_if_needed()?;
    pkg_tarball.unpack_into(&mut pkg_config, &mut pkg_files)?;

    let pkg_json = Pkg::parse_config_as_json(pkg_config)?;
    let pkg_latest = Pkg::new(pkg_dir, pkg_json, pkg_registry_url, pkg_files)?;

    Ok(pkg_latest)
}

fn fetch_latest_pkg_info_for(pkg: &Pkg) -> Result<JsonValue> {
    let request_url = &pkg.registry_url.join(&pkg.name)?;
    let response = reqwest::blocking::get(request_url.to_string())?;

    if !response.status().is_success() {
        bail!(
            "Failed to fetch package information from registry: {}.",
            response.status()
        );
    }

    let response_body = response.text()?;
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
    let tarball_checksum = BASE_64_STANDARD.decode(tarball_hash_integrity)?;

    PkgTarball::new(name, dir.to_owned(), tarball_url, tarball_checksum)
}
