use crate::path::path_matches_a_pattern_in;
use crate::pkg::error::PkgError;
use glob::Pattern;
use json::iterators::Members;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub struct PkgContents {
    pub pkg_dir: PathBuf,
    pub resolved_files: HashSet<PathBuf>,
    pub include_patterns: Vec<Pattern>,
    pub exclude_patterns: Vec<Pattern>,
}

impl PkgContents {
    pub fn new(pkg_dir: &Path, pkg_file_globs: Members) -> Result<Self, PkgError> {
        let pkg_dir = pkg_dir.to_owned();
        let resolved_files = HashSet::new();
        let include_patterns = Self::get_file_include_patterns(&pkg_dir, pkg_file_globs)?;
        let exclude_patterns = Self::get_file_exclude_patterns()?;

        Ok(PkgContents {
            pkg_dir,
            resolved_files,
            include_patterns,
            exclude_patterns,
        })
    }

    pub fn resolve_contents_in_dir(&mut self, dir: &PathBuf) -> Result<(), PkgError> {
        for entry in fs::read_dir(dir)? {
            let entry = entry.unwrap();
            let entry_path = entry.path();

            if path_matches_a_pattern_in(&entry_path, &self.exclude_patterns) {
                continue;
            }

            if entry.file_type()?.is_dir() {
                return self.resolve_contents_in_dir(&entry_path);
            }

            if path_matches_a_pattern_in(&entry_path, &self.include_patterns) {
                self.resolved_files.insert(entry_path.to_owned());
            }
        }

        Ok(())
    }

    fn get_file_include_patterns(
        pkg_dir: &Path,
        glob_paths: Members,
    ) -> Result<Vec<Pattern>, PkgError> {
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

    fn get_file_exclude_patterns() -> Result<Vec<Pattern>, PkgError> {
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
            let glob_path = PathBuf::from(glob).join("**/*");
            let glob_path = glob_path.to_str().unwrap();

            patterns.push(Pattern::new(glob_path)?);
        }

        Ok(patterns)
    }
}
