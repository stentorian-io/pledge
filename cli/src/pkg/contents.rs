use crate::fs::file::FileExt;
use crate::fs::path::get_matching_files_in_dir;
use crate::pkg::tarball::PkgTarball;
use anyhow::Result;
use glob::Pattern;
use json::iterators::Members;
use json::JsonValue;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;
use tar::Entry;

pub struct PkgContents {
    pub pkg_dir: PathBuf,
    pub include_patterns: Vec<Pattern>,
    pub exclude_patterns: Vec<Pattern>,
    pkg_tarball: Option<PkgTarball>,
}

impl PkgContents {
    pub fn new(
        pkg_dir: &Path,
        pkg_json: &JsonValue,
        pkg_tarball: Option<PkgTarball>,
    ) -> Result<Self> {
        let pkg_dir = pkg_dir.to_owned();
        let file_globs = pkg_json["files"].members();

        let include_patterns = Self::get_file_include_patterns(&pkg_dir, file_globs)?;
        let exclude_patterns = Self::get_file_exclude_patterns(&pkg_dir)?;

        Ok(PkgContents {
            pkg_dir,
            include_patterns,
            exclude_patterns,
            pkg_tarball,
        })
    }

    pub fn is_tarball(&self) -> bool {
        self.pkg_tarball.is_some()
    }

    pub fn asset_list(&self) -> Result<HashSet<PathBuf>> {
        if self.is_tarball() {
            let tarball = self.pkg_tarball.as_ref().unwrap();
            let tarball_files = tarball.get_files(Some(|entry: &Entry<&[u8]>| {
                let entry_path = &entry.path().unwrap().to_path_buf();
                let entry_ext = FileExt::from(entry_path)?;

                Ok(entry_ext.is_other())
            }))?;

            return Ok(tarball_files);
        }

        let pkg_dir = &self.pkg_dir.to_path_buf();
        let mut matched_files = HashSet::new();
        let mut exclude_patterns = self.exclude_patterns.clone();

        for ext in FileExt::iter() {
            if !ext.is_other() {
                let glob = format!("**/*.{}", ext.to_value());
                let pattern = Pattern::new(glob.as_str())?;

                exclude_patterns.push(pattern);
            }
        }

        get_matching_files_in_dir(
            pkg_dir,
            &mut matched_files,
            &self.include_patterns,
            &exclude_patterns,
            |entry_path| Ok(entry_path.strip_prefix(&self.pkg_dir)?.to_path_buf()),
        )?;

        Ok(matched_files)
    }

    pub fn load_file(&self, file_path: &PathBuf) -> Result<Option<Vec<u8>>> {
        if self.is_tarball() {
            let tarball = self.pkg_tarball.as_ref().unwrap();
            let tarball_file = tarball.load_file_by_path(file_path)?;

            return Ok(tarball_file);
        }

        Ok(Some(fs::read(self.pkg_dir.join(file_path))?))
    }

    fn get_file_include_patterns(pkg_dir: &Path, glob_paths: Members) -> Result<Vec<Pattern>> {
        if glob_paths.len().eq(&0) {
            let glob_path = pkg_dir.join("**/*");
            let glob_path = glob_path.to_str().unwrap();
            let glob_pattern = Pattern::new(glob_path)?;

            return Ok(vec![glob_pattern]);
        }

        let mut patterns: Vec<Pattern> = Vec::with_capacity(glob_paths.len());

        for glob_path in glob_paths.into_iter() {
            let glob_path = glob_path.to_string();

            // We filter out paths that go outside the package root — npm considers them invalid.
            if glob_path.starts_with("../") {
                continue;
            }

            let glob_path = pkg_dir.join(glob_path);
            let glob_path = glob_path.to_str().unwrap();
            let glob_pattern = Pattern::new(glob_path)?;

            patterns.push(glob_pattern)
        }

        Ok(patterns)
    }

    fn get_file_exclude_patterns(pkg_dir: &Path) -> Result<Vec<Pattern>> {
        let mut patterns: Vec<Pattern> = Vec::new();

        // See https://docs.npmjs.com/cli/v10/configuring-npm/package-json#files
        let default_globs = vec![
            ".git",
            ".npmrc",
            "node_modules",
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
        ];

        for glob in default_globs {
            let glob_path = pkg_dir.join(glob);
            let glob_path = glob_path.to_str().unwrap();

            patterns.push(Pattern::new(glob_path)?);
        }

        Ok(patterns)
    }
}
