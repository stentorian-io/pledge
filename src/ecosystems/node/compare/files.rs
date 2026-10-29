//! Enumerate packed files without following directory cycles or escaping a package.

use crate::core::findings::{Finding, Severity};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

const MAX_DIRECTORY_DEPTH: usize = 128;

#[derive(Default)]
pub(super) struct PackageFiles {
    pub paths: BTreeSet<String>,
    pub findings: Vec<Finding>,
}

impl PackageFiles {
    fn incomplete(&mut self, relative: &Path, reason: impl std::fmt::Display) {
        let file = if relative.as_os_str().is_empty() {
            PathBuf::from("package.json")
        } else {
            relative.to_path_buf()
        };
        self.findings.push(Finding {
            severity: Severity::Warning,
            rule: "analysis-incomplete",
            message: format!("cannot scan package path `{}`: {reason}", file.display()),
            file,
            line: 1,
            col: 1,
        });
    }
}

pub(super) fn scan_package_files(root: &Path, accepts: fn(&Path) -> bool) -> PackageFiles {
    let mut result = PackageFiles::default();
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            result.incomplete(Path::new(""), error);
            return result;
        }
    };
    collect(
        &root,
        Path::new(""),
        accepts,
        &mut HashSet::new(),
        &mut result,
    );
    result
}

fn collect(
    root: &Path,
    relative: &Path,
    accepts: fn(&Path) -> bool,
    ancestors: &mut HashSet<PathBuf>,
    result: &mut PackageFiles,
) {
    let path = root.join(relative);
    let canonical = match path.canonicalize() {
        Ok(canonical) => canonical,
        Err(error) => {
            result.incomplete(relative, error);
            return;
        }
    };
    if !canonical.starts_with(root) {
        result.incomplete(relative, "symlink target is outside the package");
        return;
    }
    if !canonical.is_dir() {
        if !canonical.is_file() {
            result.incomplete(relative, "path is not a regular file or directory");
            return;
        }
        if accepts(relative) {
            match relative.to_str() {
                Some(relative) => {
                    result.paths.insert(relative.replace('\\', "/"));
                }
                None => result.incomplete(relative, "file name is not UTF-8"),
            }
        }
        return;
    }
    if ancestors.len() >= MAX_DIRECTORY_DEPTH {
        result.incomplete(relative, "directory depth limit reached");
        return;
    }
    if !ancestors.insert(canonical.clone()) {
        result.incomplete(relative, "directory symlink cycle");
        return;
    }
    match std::fs::read_dir(&path) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        result.incomplete(relative, error);
                        continue;
                    }
                };
                let name = entry.file_name();
                if name
                    .to_str()
                    .is_some_and(|name| name.starts_with('.') || name == "node_modules")
                {
                    continue;
                }
                collect(root, &relative.join(name), accepts, ancestors, result);
            }
        }
        Err(error) => result.incomplete(relative, error),
    }
    // Track ancestors rather than all visited directories: a noncyclic alias
    // exposes its own packed paths and must remain visible to comparisons.
    ancestors.remove(&canonical);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn branching_symlink_cycles_are_bounded() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("api.js"), "").unwrap();
        for name in ["again", "also"] {
            std::os::unix::fs::symlink(".", root.path().join(name)).unwrap();
        }

        let result = scan_package_files(root.path(), |_| true);

        assert_eq!(result.paths, BTreeSet::from(["api.js".into()]));
        assert_eq!(result.findings.len(), 2);
        assert!(result
            .findings
            .iter()
            .all(|finding| finding.rule == "analysis-incomplete"));
    }

    #[cfg(unix)]
    #[test]
    fn broken_links_report_missing_evidence() {
        let root = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("absent.js", root.path().join("api.js")).unwrap();

        let result = scan_package_files(root.path(), |_| true);

        assert!(result.paths.is_empty());
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].file, PathBuf::from("api.js"));
    }

    #[test]
    fn directory_depth_is_bounded_and_reported() {
        let root = tempfile::tempdir().unwrap();
        let mut directory = root.path().to_path_buf();
        for _ in 0..MAX_DIRECTORY_DEPTH {
            directory.push("d");
            std::fs::create_dir(&directory).unwrap();
        }

        let result = scan_package_files(root.path(), |_| true);

        assert_eq!(result.findings.len(), 1);
        assert!(result.findings[0].message.contains("depth limit"));
    }
}
