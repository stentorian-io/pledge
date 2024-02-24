use base64::{engine::general_purpose::STANDARD, Engine as _};
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum TarballError {
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Base64 Decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Clone)]
pub struct Tarball {
    pub url: Url,
    pub checksum: Vec<u8>,
}

impl Tarball {
    pub fn new(url: &str, integrity: &str) -> Result<Tarball, TarballError> {
        let integrity_parts: Vec<&str> = integrity.split('-').collect();

        if integrity_parts.len().ne(&2) {
            return Err(TarballError::Validation(
                "Unexpected package integrity string format.".into(),
            ));
        }

        let hash_algorithm = integrity_parts.first().unwrap();
        let hash_integrity = integrity_parts.last().unwrap();

        if hash_algorithm.ne(&"sha512") {
            return Err(TarballError::Validation(
                "Package integrity can only be verified with SHA-512.".into(),
            ));
        }

        Ok(Tarball {
            url: Url::parse(url)?,
            checksum: STANDARD.decode(hash_integrity)?,
        })
    }
}
