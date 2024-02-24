use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum TarballError {
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
}

#[derive(Debug, Clone)]
pub struct Tarball {
    pub url: Url,
    pub checksum: String,
}

impl Tarball {
    pub fn new(url: &str, checksum: &str) -> Result<Tarball, TarballError> {
        let url = Url::parse(url)?;
        let checksum = checksum.to_string();

        Ok(Tarball { url, checksum })
    }
}
