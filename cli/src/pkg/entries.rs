use crate::path::path_matches_a_pattern_in;
use crate::pkg::error::PkgError;
use glob::Pattern;
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
    pub fn new(
        file_include_patterns: &Vec<Pattern>,
        pkg_dir: &PathBuf,
        pkg_json: &JsonValue,
        should_skip_validation: bool,
    ) -> Result<Self, PkgError> {
        let main = &pkg_json["main"];
        let main = if main.is_null() {
            Self::resolve_and_validate_file_path(
                &pkg_dir,
                &file_include_patterns,
                should_skip_validation,
                "index.js".to_string(),
            )?
        } else {
            Self::resolve_and_validate_file_path(
                &pkg_dir,
                &file_include_patterns,
                should_skip_validation,
                main.to_string(),
            )?
        };

        let browser = Self::resolve_string_or_object_entries(
            "browser".into(),
            &pkg_dir,
            &pkg_json,
            &file_include_patterns,
            should_skip_validation,
        )?;

        let bin = Self::resolve_string_or_object_entries(
            "bin".into(),
            &pkg_dir,
            &pkg_json,
            &file_include_patterns,
            should_skip_validation,
        )?;

        let exports = Self::resolve_string_or_object_entries(
            "exports".into(),
            &pkg_dir,
            &pkg_json,
            &file_include_patterns,
            should_skip_validation,
        )?;

        Ok(PkgEntries {
            main,
            bin,
            browser,
            exports,
        })
    }

    fn resolve_string_or_object_entries(
        field_name: String,
        pkg_dir: &PathBuf,
        pkg_json: &JsonValue,
        file_include_patterns: &Vec<Pattern>,
        should_skip_validation: bool,
    ) -> Result<HashMap<String, PathBuf>, PkgError> {
        let property = &pkg_json[&field_name];
        let mut entries: HashMap<String, PathBuf> = HashMap::new();

        if property.is_string() {
            let entry_path = property.to_string();
            let entry_path = Self::resolve_and_validate_file_path(
                &pkg_dir,
                &file_include_patterns,
                should_skip_validation,
                entry_path,
            )?;

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
                let entry_path = Self::resolve_and_validate_file_path(
                    &pkg_dir,
                    &file_include_patterns,
                    should_skip_validation,
                    entry_path,
                )?;

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
                let entry_path = Self::resolve_and_validate_file_path(
                    &pkg_dir,
                    &file_include_patterns,
                    should_skip_validation,
                    entry_path,
                )?;

                entries.insert(sub_entry_name.into(), entry_path);
            }
        }

        Ok(entries)
    }

    fn resolve_and_validate_file_path(
        pkg_dir: &PathBuf,
        file_include_patterns: &Vec<Pattern>,
        should_skip_validation: bool,
        file_path: String,
    ) -> Result<PathBuf, PkgError> {
        let file_path = pkg_dir.join(file_path);

        if should_skip_validation {
            return Ok(file_path);
        }

        if !file_path.try_exists()? {
            return Err(PkgError::Validation(format!(
                "File '{}' does not exist.",
                file_path.display()
            )));
        }

        if !path_matches_a_pattern_in(&file_path, &file_include_patterns) {
            return Err(PkgError::Validation(format!(
                "File '{}' exists but is not included in 'files'.",
                file_path.display()
            )));
        }

        Ok(file_path)
    }
}
