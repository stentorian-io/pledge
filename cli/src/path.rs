use glob::Pattern;
use std::path::Path;

pub fn path_matches_a_pattern_in(path: &Path, patterns: &[Pattern]) -> bool {
    patterns.iter().any(|pattern| pattern.matches_path(path))
}
