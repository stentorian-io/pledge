use anyhow::Result;
use glob::Pattern;
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
    on_match: OnMatch,
) -> Result<()>
where
    OnMatch: Fn(PathBuf) -> Result<PathBuf>,
{
    for entry in fs::read_dir(dir)? {
        let entry = entry.unwrap();
        let entry_path = entry.path();

        if path_matches_a_pattern_in(&entry_path, exclude_patterns) {
            continue;
        }

        if entry.file_type()?.is_dir() {
            return get_matching_files_in_dir(
                &entry_path,
                buffer,
                include_patterns,
                exclude_patterns,
                on_match,
            );
        }

        if path_matches_a_pattern_in(&entry_path, include_patterns) {
            buffer.insert(on_match(entry_path)?);
        }
    }

    Ok(())
}
