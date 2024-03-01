use crate::pkg::contents::PkgContents;
use crate::pkg::entries::PkgEntries;
use anyhow::{bail, Result};
use json::JsonValue;
use std::collections::HashSet;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use url::Url;

pub mod contents;
pub mod entries;
pub mod tarball;

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
        files: HashSet<PathBuf>,
    ) -> Result<Self> {
        let name = pkg_json["name"].to_string();
        let version = pkg_json["version"].to_string();
        let file_globs = pkg_json["files"].members();

        let mut contents = PkgContents::new(&dir_path, file_globs)?;
        let entries = PkgEntries::new(&contents, &pkg_json)?;

        if !files.is_empty() {
            contents.resolved_files = files;
        }

        Ok(Pkg {
            name,
            version,
            dir_path,
            registry_url,
            contents,
            entries,
        })
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

    pub fn get_registry_url(dir_path: &Path) -> Result<Url> {
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

    pub fn resolve_dir_contents(mut self) -> Result<Self> {
        self.contents.resolve_contents_in_dir(&self.dir_path)?;
        Ok(self)
    }
}
