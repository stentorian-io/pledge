use crate::path::path_matches_a_pattern_in;
use crate::pkg::contents::PkgContents;
use crate::pkg::error::PkgError;
use json::JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct PkgEntries {
    pub main: PathBuf,
    pub bin: HashMap<String, PathBuf>,
    pub browser: HashMap<String, PathBuf>,
    pub exports: HashMap<String, PathBuf>,
}

impl PkgEntries {
    pub fn new(pkg_contents: &PkgContents, pkg_json: &JsonValue) -> Result<Self, PkgError> {
        let main = Self::resolve_main_entry(pkg_json, pkg_contents)?;
        let browser = Self::resolve_browser_entries(pkg_json, pkg_contents)?;
        let bin = Self::resolve_bin_entries(pkg_json, pkg_contents)?;
        let exports = Self::resolve_exports_entries(pkg_json, pkg_contents)?;

        Ok(PkgEntries {
            main,
            bin,
            browser,
            exports,
        })
    }

    fn resolve_main_entry(
        pkg_json: &JsonValue,
        pkg_contents: &PkgContents,
    ) -> Result<PathBuf, PkgError> {
        let main_file_path = &pkg_json["main"];
        let main_file_path = if main_file_path.is_string() {
            main_file_path.to_string()
        } else {
            "index.js".to_string()
        };

        Self::resolve_file_path(pkg_contents, main_file_path)
    }

    fn resolve_browser_entries(
        pkg_json: &JsonValue,
        pkg_contents: &PkgContents,
    ) -> Result<HashMap<String, PathBuf>, PkgError> {
        Self::resolve_string_or_object_entries("browser".into(), pkg_json, pkg_contents)
    }

    fn resolve_bin_entries(
        pkg_json: &JsonValue,
        pkg_contents: &PkgContents,
    ) -> Result<HashMap<String, PathBuf>, PkgError> {
        Self::resolve_string_or_object_entries("bin".into(), pkg_json, pkg_contents)
    }

    fn resolve_exports_entries(
        pkg_json: &JsonValue,
        pkg_contents: &PkgContents,
    ) -> Result<HashMap<String, PathBuf>, PkgError> {
        Self::resolve_string_or_object_entries("exports".into(), pkg_json, pkg_contents)
    }

    fn resolve_string_or_object_entries(
        field_name: String,
        pkg_json: &JsonValue,
        pkg_contents: &PkgContents,
    ) -> Result<HashMap<String, PathBuf>, PkgError> {
        let property = &pkg_json[&field_name];
        let mut entries: HashMap<String, PathBuf> = HashMap::new();

        if property.is_string() {
            let entry_path = property.to_string();
            let entry_path = Self::resolve_file_path(pkg_contents, entry_path)?;

            entries.insert(field_name.to_owned(), entry_path);

            return Ok(entries);
        } else if !property.is_object() {
            return Ok(entries);
        }

        for (entry_name, entry_value) in property.entries() {
            // Ignore exclusion entries — if property is browser.
            if entry_value.is_boolean() {
                continue;
            }

            if entry_value.is_string() {
                let entry_path = entry_value.to_string();
                let entry_path = Self::resolve_file_path(pkg_contents, entry_path)?;

                entries.insert(entry_name.into(), entry_path);

                continue;
            }

            if !entry_value.is_object() {
                return Err(PkgError::Validation(format!(
                    "Expected '{field_name}' field to be an object."
                )));
            }

            for (sub_entry_name, sub_entry_value) in entry_value.entries() {
                if !sub_entry_value.is_string() {
                    return Err(PkgError::Validation(format!(
                        "Expected '{sub_entry_name}' in '{field_name}' field to be a string."
                    )));
                }

                let entry_path = sub_entry_value.to_string();
                let entry_path = Self::resolve_file_path(pkg_contents, entry_path)?;

                entries.insert(sub_entry_name.into(), entry_path);
            }
        }

        Ok(entries)
    }

    fn resolve_file_path(
        pkg_contents: &PkgContents,
        file_path: String,
    ) -> Result<PathBuf, PkgError> {
        let file_path = pkg_contents.pkg_dir.join(file_path);
        let should_skip_validation = pkg_contents.pkg_dir.ends_with(".tmp");

        if should_skip_validation {
            return Ok(file_path);
        }

        if !file_path.try_exists()? {
            return Err(PkgError::Validation(format!(
                "File '{}' is missing.",
                file_path.display()
            )));
        }

        if !path_matches_a_pattern_in(&file_path, &pkg_contents.include_patterns) {
            return Err(PkgError::Validation(format!(
                "File '{}' exists but does not mach any globs in 'files'.",
                file_path.display()
            )));
        }

        Ok(file_path)
    }
}
