use anyhow::{Context, Result};
use glob::{glob, Pattern};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn path_matches_a_pattern_in(path: &Path, patterns: &[Pattern]) -> bool {
    patterns.iter().any(|pattern| pattern.matches_path(path))
}

pub fn get_matching_files_in_dir<OnMatch>(
    dir: &PathBuf,
    buffer: &mut HashSet<PathBuf>,
    include_patterns: &Vec<Pattern>,
    exclude_patterns: &Vec<Pattern>,
    exclude_negation_patterns: &Vec<Pattern>,
    on_match: &OnMatch,
) -> Result<()>
where
    OnMatch: Fn(&PathBuf) -> Result<PathBuf>,
{
    for entry in fs::read_dir(dir)? {
        let entry = entry.unwrap();
        let entry_path = entry.path();

        if path_matches_a_pattern_in(&entry_path, exclude_patterns) {
            continue;
        }

        if entry.file_type()?.is_dir() {
            get_matching_files_in_dir(
                &entry_path,
                buffer,
                include_patterns,
                exclude_patterns,
                exclude_negation_patterns,
                on_match,
            )
            .with_context(|| {
                format!(
                    "Failed to get matching files in dir: {}",
                    entry_path.display()
                )
            })?;

            continue;
        }

        if path_matches_a_pattern_in(&entry_path, include_patterns) {
            buffer.insert(on_match(&entry_path).with_context(|| {
                format!(
                    "Failed to process include file match: {}",
                    entry_path.display()
                )
            })?);
        }
    }

    // Explicitly include any files excluded under the exclusion patterns
    // that are also covered by a negated exclusion pattern.
    for pattern in exclude_negation_patterns {
        for file_path in glob(pattern.as_str())? {
            let file_path = file_path.unwrap();

            buffer.insert(on_match(&file_path).with_context(|| {
                format!(
                    "Failed to process exclude negation file match: {}",
                    file_path.display()
                )
            })?);
        }
    }

    Ok(())
}
