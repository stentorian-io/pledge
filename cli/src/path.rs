use glob::Pattern;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PathError {
    #[error("Pattern error: {0}")]
    Pattern(#[from] glob::PatternError),
}

pub fn path_matches_a_pattern_in(path: &Path, patterns: &[Pattern]) -> bool {
    patterns.iter().any(|pattern| pattern.matches_path(path))
}
