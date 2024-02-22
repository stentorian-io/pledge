use std::fs::read_to_string;
use std::io;
use std::path::PathBuf;
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
    pub fn new(dir_path: &str) -> Result<Self, PkgError> {
        let pkg = Self::read_config_as_string(dir_path)?;
        let pkg = Self::parse_config_as_json(&pkg)?;

        Ok(pkg)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    fn read_config_as_string(dir_path: &str) -> Result<String, PkgError> {
        let path = PathBuf::from(dir_path);

        if !path.is_dir() {
            return Err(PkgError::Validation(
                "Expected package path to be a directory.".into(),
            ));
        }

        let path = path.join("package.json");

        if !path.is_file() {
            return Err(PkgError::Validation(
                "Expected package path to contain a package.json file.".into(),
            ));
        }

        Ok(read_to_string(path)?)
    }

    fn parse_config_as_json(content: &str) -> Result<Self, PkgError> {
        let pkg = json::parse(&content)?;

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
