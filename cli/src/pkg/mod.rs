use crate::pkg::contents::PkgContents;
use crate::pkg::entries::PkgEntries;
use anyhow::{bail, Result};
use json::JsonValue;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use url::Url;

pub mod contents;
pub mod entries;
pub mod registry;
pub mod tarball;

pub struct Pkg {
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
    pub registry_url: Url,
    pub entries: PkgEntries,
    pub contents: Rc<PkgContents>,
}

impl Pkg {
    pub fn new(
        dir: PathBuf,
        config: JsonValue,
        registry_url: Url,
        contents: Rc<PkgContents>,
        entries: PkgEntries,
    ) -> Self {
        let name = config["name"].to_string();
        let version = config["version"].to_string();
        let contents = Rc::clone(&contents);

        Self {
            name,
            version,
            dir,
            registry_url,
            contents,
            entries,
        }
    }

    pub fn parse_config_in_dir(path: &Path) -> Result<JsonValue> {
        if !path.is_dir() {
            bail!("Expected package path to be a directory.");
        }

        let path = path.join("package.json");

        if !path.is_file() {
            bail!("Expected package path to contain a package.json file.");
        }

        let data = read_to_string(path)?;
        let config = Self::parse_config_as_json(data)?;

        Ok(config)
    }

    pub fn parse_config_as_json(data: String) -> Result<JsonValue> {
        let pkg = json::parse(&data)?;

        let pkg_name = &pkg["name"];
        let pkg_version = &pkg["version"];

        if !pkg_name.is_string() {
            bail!("Expected package 'name' field to be a string.");
        } else if !pkg_version.is_string() {
            bail!("Expected package 'version' field to be a string.");
        }

        Ok(pkg)
    }

    pub fn get_registry_url(pkg_dir: &Path) -> Result<Url> {
        let npmrc_path = pkg_dir.join(".npmrc");

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
