use crate::pkg::contents::PkgContents;
use crate::pkg::entries::PkgEntries;
use crate::pkg::error::PkgError;
use json::JsonValue;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use url::Url;

pub mod contents;
pub mod entries;
pub mod error;

pub struct Pkg {
    pub name: String,
    pub version: String,
    pub dir_path: PathBuf,
    pub registry_url: Url,
    pub entries: PkgEntries,
    pub contents: PkgContents,
}

impl Pkg {
    pub fn new(
        dir_path: PathBuf,
        pkg_json: JsonValue,
        registry_url: Url,
    ) -> Result<Self, PkgError> {
        let name = pkg_json["name"].to_string();
        let version = pkg_json["version"].to_string();
        let file_globs = pkg_json["files"].members();

        let contents = PkgContents::new(&dir_path, file_globs)?;
        let entries = PkgEntries::new(&contents, &pkg_json)?;

        Ok(Pkg {
            name,
            version,
            dir_path,
            registry_url,
            contents,
            entries,
        })
    }

    pub fn parse_config_in_dir(path: &Path) -> Result<JsonValue, PkgError> {
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

        let data = read_to_string(path)?;
        let config = Self::parse_config_as_json(data)?;

        Ok(config)
    }

    pub fn parse_config_as_json(data: String) -> Result<JsonValue, PkgError> {
        let pkg = json::parse(&data)?;

        let pkg_name = &pkg["name"];
        let pkg_version = &pkg["version"];

        if !pkg_name.is_string() {
            return Err(PkgError::Validation(
                "Expected package 'name' field to be a string.".into(),
            ));
        }

        if !pkg_version.is_string() {
            return Err(PkgError::Validation(
                "Expected package 'version' field to be a string.".into(),
            ));
        }

        Ok(pkg)
    }

    pub fn get_registry_url(dir_path: &Path) -> Result<Url, PkgError> {
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
