use json::JsonValue;
use std::fs::read_to_string;
use std::io;
use std::path::PathBuf;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum PkgError {
    #[error("JSON error: {0}")]
    Json(#[from] json::JsonError),
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("Validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Clone)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    pub dir_path: PathBuf,
    pub registry_url: Url,
}

impl Pkg {
    pub fn new(dir_path: PathBuf) -> Result<Self, PkgError> {
        let pkg = Self::read_config_as_string(&dir_path)?;
        let pkg = Self::parse_config_as_json(&pkg)?;

        let name = pkg["name"].to_string();
        let version = pkg["version"].to_string();
        let registry_url = Self::get_registry_url(&dir_path)?;

        println!("Will use {registry_url} as registry.");

        Ok(Pkg {
            name,
            version,
            dir_path,
            registry_url,
        })
    }

    fn read_config_as_string(path: &PathBuf) -> Result<String, PkgError> {
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

    fn parse_config_as_json(content: &str) -> Result<JsonValue, PkgError> {
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

        Ok(pkg)
    }

    fn get_registry_url(dir_path: &PathBuf) -> Result<Url, PkgError> {
        let npmrc_path = dir_path.join(".npmrc");

        if npmrc_path.is_file() {
            let npmrc = read_to_string(npmrc_path)?;

            for line in npmrc.split('\n') {
                if line.trim_start().starts_with("registry=") {
                    return Ok(Url::parse(line.split('=').last().unwrap())?);
                }
            }
        }

        Ok(Url::parse("https://registry.npmjs.org/")?)
    }
}
