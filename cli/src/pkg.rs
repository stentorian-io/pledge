use std::fs::read_to_string;
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PkgError {
    #[error("JSON error: {0}")]
    Json(#[from] json::JsonError),
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("Validation error: {0}")]
    Validation(String),
}

#[derive(Debug)]
pub struct Pkg {
    name: String,
    version: String,
}

impl Pkg {
    pub fn new(file_path: &str) -> Result<Self, PkgError> {
        let pkg = read_to_string(file_path)?;
        let pkg = json::parse(&pkg)?;

        let name = &pkg["name"];
        let version = &pkg["version"];

        if !name.is_string() {
            return Err(PkgError::Validation(format!(
                "Unexpected value '{}' for 'name' property.",
                name.to_string()
            )));
        }

        if !version.is_string() {
            return Err(PkgError::Validation(format!(
                "Unexpected value '{}' for 'version' property.",
                version.to_string()
            )));
        }

        Ok(Pkg {
            name: name.to_string(),
            version: version.to_string(),
        })
    }
}
