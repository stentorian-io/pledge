use crate::pkg::Pkg;
use crate::tar::{Tarball, TarballError};
use thiserror::Error;

pub struct Npm;

#[derive(Error, Debug)]
pub enum NpmError {
    #[error("JSON error: {0}")]
    Json(#[from] json::JsonError),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Validation error: {0}")]
    Tarball(#[from] TarballError),
    #[error("Tarball error: {0}")]
    Validation(String),
}

impl Npm {
    pub fn fetch_latest_of(pkg: &Pkg) -> Result<Pkg, NpmError> {
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
        let tarball_checksum = &dist["shasum"];

        if !tarball_url.is_string() {
            return Err(NpmError::Validation(
                "Couldn't find tarball URL for latest package".to_string(),
            ));
        }

        if !tarball_checksum.is_string() {
            return Err(NpmError::Validation(
                "Couldn't find tarball checksum for latest package".to_string(),
            ));
        }

        let version = latest_version.to_string();
        let tarball = Some(Tarball::new(
            &tarball_url.to_string(),
            &tarball_checksum.to_string(),
        )?);

        Ok(Pkg {
            version,
            tarball,
            ..pkg.clone()
        })
    }
}
