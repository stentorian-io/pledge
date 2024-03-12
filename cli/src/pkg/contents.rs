use crate::fs::file::FileExt;
use crate::fs::path::get_matching_files_in_dir;
use crate::pkg::tarball::PkgTarball;
use anyhow::Result;
use glob::Pattern;
use json::iterators::Members;
use json::JsonValue;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use strum::IntoEnumIterator;
use tar::Entry;

pub struct PkgContents {
    pub pkg_dir: PathBuf,
    pub include_patterns: Vec<Pattern>,
    pub exclude_patterns: Vec<Pattern>,
    pub exclude_negation_patterns: Vec<Pattern>,
    pkg_tarball: Option<PkgTarball>,
}

impl PkgContents {
    pub fn new(
        pkg_dir: PathBuf,
        pkg_json: &JsonValue,
        pkg_tarball: Option<PkgTarball>,
    ) -> Result<Self> {
        let mut contents = PkgContents {
            pkg_dir,
            pkg_tarball,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            exclude_negation_patterns: Vec::new(),
        };

        if contents.pkg_tarball.is_none() {
            let include_globs = pkg_json["files"].members();
            let include_patterns = contents.get_file_include_patterns(include_globs)?;

            let (exclude_patterns, exclude_negation_patterns) =
                contents.get_file_exclude_patterns(&include_patterns)?;

            contents.include_patterns = include_patterns;
            contents.exclude_patterns = exclude_patterns;
            contents.exclude_negation_patterns = exclude_negation_patterns;
        }

        Ok(contents)
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

        let mut matched_files = HashSet::new();
        let mut exclude_patterns = self.exclude_patterns.to_owned();

        for ext in FileExt::iter() {
            if !ext.is_other() {
                let glob_path = format!("**/*.{}", ext.to_value());
                let glob_path = self.pkg_dir.join(glob_path);
                let glob_pattern = Pattern::new(glob_path.to_str().unwrap())?;

                exclude_patterns.push(glob_pattern);
            }
        }

        get_matching_files_in_dir(
            &self.pkg_dir.to_path_buf(),
            &mut matched_files,
            &self.include_patterns,
            &exclude_patterns,
            &self.exclude_negation_patterns,
            &|entry_path| Ok(entry_path.strip_prefix(&self.pkg_dir)?.to_path_buf()),
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

    pub fn get_file_include_patterns(&self, include_globs: Members) -> Result<Vec<Pattern>> {
        if include_globs.len().eq(&0) {
            let glob_path = self.pkg_dir.join("**/*");
            let glob_path = glob_path.to_str().unwrap();
            let glob_pattern = Pattern::new(glob_path)?;

            return Ok(vec![glob_pattern]);
        }

        let mut patterns: Vec<Pattern> = Vec::with_capacity(include_globs.len());

        for glob_path in include_globs {
            let glob_path = glob_path.to_string();

            // We filter out paths that go outside the package root — npm considers them invalid.
            if glob_path.starts_with("../") {
                continue;
            }

            let glob_path = self.pkg_dir.join(glob_path);
            let glob_path = glob_path.to_str().unwrap();
            let glob_pattern = Pattern::new(glob_path)?;

            patterns.push(glob_pattern)
        }

        Ok(patterns)
    }

    // See https://docs.npmjs.com/cli/v10/configuring-npm/package-json#files
    pub fn get_file_exclude_patterns(
        &self,
        include_patterns: &Vec<Pattern>,
    ) -> Result<(Vec<Pattern>, Vec<Pattern>)> {
        let mut exclude_patterns: HashSet<Pattern> = HashSet::new();
        let mut exclude_negation_patterns: HashSet<Pattern> = HashSet::new();

        // These files are always excluded, regardless of settings.
        let hard_exclude_globs = vec![
            "**/.git",
            "**/.npmrc",
            "**/node_modules",
            "**/package-lock.json",
            "**/pnpm-lock.yaml",
            "**/yarn.lock",
        ];

        for glob in hard_exclude_globs {
            exclude_patterns.insert(self.to_pkg_file_pattern(glob)?);
        }

        // These files are excluded by default, but can be configured to be included.
        let soft_exclude_globs = vec![
            "**/*.orig",
            "**/.*.swp",
            "**/.DS_Store",
            "**/._*",
            "**/.hg",
            "**/.lock-wscript",
            "**/.svn",
            "**/.wafpickle-N",
            "**/CVS",
            "**/config.gypi",
            "**/npm-debug.log",
        ];

        for soft_exclude_glob in soft_exclude_globs {
            let mut should_keep_glob = true;

            for include_pattern in include_patterns {
                if include_pattern.matches_path(&PathBuf::from(soft_exclude_glob)) {
                    should_keep_glob = false;
                    break;
                }
            }

            if should_keep_glob {
                exclude_patterns.insert(self.to_pkg_file_pattern(soft_exclude_glob)?);
            }
        }

        // These files are always included, regardless of settings.
        let exclude_negation_globs = HashSet::from([
            "package.json",
            // README, LICENSE, and LICENCE can have any casing or extension.
            "[rR][eE][aA][dD][mM][eE]",
            "[rR][eE][aA][dD][mM][eE].*",
            "[lL][iI][cC][eE][nN][sS][eE]",
            "[lL][iI][cC][eE][nN][sS][eE].*",
            "[lL][iI][cC][eE][nN][cC][eE]",
            "[lL][iI][cC][eE][nN][cC][eE].*",
        ]);

        for exclude_negation_glob in exclude_negation_globs {
            exclude_negation_patterns.insert(self.to_pkg_file_pattern(exclude_negation_glob)?);
        }

        self.get_npm_ignore_globs_if_any_in(&mut exclude_patterns, &mut exclude_negation_patterns)?;

        let exclude_patterns = exclude_patterns.into_iter().collect();
        let exclude_negation_patterns = exclude_negation_patterns.into_iter().collect();

        Ok((exclude_patterns, exclude_negation_patterns))
    }

    pub fn get_npm_ignore_globs_if_any_in(
        &self,
        exclude_patterns: &mut HashSet<Pattern>,
        exclude_negation_patterns: &mut HashSet<Pattern>,
    ) -> Result<()> {
        let file_contents = fs::read_to_string(self.pkg_dir.join(".npmignore"));

        if file_contents.is_err() {
            return Ok(());
        }

        for line in file_contents.unwrap().split("\n") {
            let line = line.trim_start();

            if line.is_empty() {
                continue;
            } else if line.starts_with("#") {
                continue;
            }

            if let Some(line) = line.strip_prefix("!") {
                if !line.is_empty() {
                    exclude_negation_patterns.insert(self.to_pkg_file_pattern(line.trim_end())?);
                }
            } else {
                exclude_patterns.insert(self.to_pkg_file_pattern(line.trim_end())?);
            }
        }

        Ok(())
    }

    fn to_pkg_file_pattern(&self, glob: &str) -> Result<Pattern> {
        let glob_path = self.pkg_dir.join(glob);
        let glob_path = glob_path.to_str().unwrap();

        Ok(Pattern::new(glob_path)?)
    }
}
