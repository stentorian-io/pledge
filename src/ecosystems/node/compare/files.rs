//! Enumerate packed files without following directory cycles or escaping a package.

use crate::core::findings::{Finding, Severity};
use std::collections::{BTreeSet, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const MAX_DIRECTORY_DEPTH: usize = 128;
const MAX_DIRECTORY_ENTRIES: usize = 100_000;

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
    scan_package_files_with_limit(root, accepts, MAX_DIRECTORY_ENTRIES)
}

fn scan_package_files_with_limit(
    root: &Path,
    accepts: fn(&Path) -> bool,
    max_directory_entries: usize,
) -> PackageFiles {
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
        &mut ScanBudget {
            remaining_entries: max_directory_entries,
            exhausted: false,
        },
        &mut result,
    );
    result
}

struct ScanBudget {
    remaining_entries: usize,
    exhausted: bool,
}

fn directory_entries(
    path: &Path,
    relative: &Path,
    budget: &mut ScanBudget,
    result: &mut PackageFiles,
) -> Option<Vec<OsString>> {
    let directory = match std::fs::read_dir(path) {
        Ok(directory) => directory,
        Err(error) => {
            result.incomplete(relative, error);
            return None;
        }
    };
    let mut names = Vec::new();
    for entry in directory {
        if budget.remaining_entries == 0 {
            budget.exhausted = true;
            result.incomplete(relative, "directory entry work limit reached");
            return None;
        }
        budget.remaining_entries -= 1;
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
            .is_some_and(|name| matches!(name, ".git" | ".hg" | ".svn" | "node_modules"))
        {
            continue;
        }
        names.push(name);
    }
    // Enumerate each whole directory before descending so filesystem iteration
    // order cannot determine which public paths survive an exhausted budget.
    names.sort();
    Some(names)
}

fn collect(
    root: &Path,
    relative: &Path,
    accepts: fn(&Path) -> bool,
    ancestors: &mut HashSet<PathBuf>,
    budget: &mut ScanBudget,
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
    if let Some(entries) = directory_entries(&path, relative, budget, result) {
        for name in entries {
            collect(
                root,
                &relative.join(name),
                accepts,
                ancestors,
                budget,
                result,
            );
            if budget.exhausted {
                break;
            }
        }
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
    fn branching_acyclic_aliases_exhaust_the_shared_scan_budget() {
        let root = tempfile::tempdir().unwrap();
        let reordered = tempfile::tempdir().unwrap();
        for (directory, levels, names) in [
            (&root, [0, 1, 2, 3, 4, 5], ["a", "b"]),
            (&reordered, [5, 4, 3, 2, 1, 0], ["b", "a"]),
        ] {
            for level in levels {
                std::fs::create_dir(directory.path().join(format!("d{level}"))).unwrap();
            }
            for level in 0..5 {
                for name in names {
                    std::os::unix::fs::symlink(
                        format!("../d{}", level + 1),
                        directory.path().join(format!("d{level}/{name}")),
                    )
                    .unwrap();
                }
            }
            std::fs::write(directory.path().join("d5/api.js"), "").unwrap();
        }

        let result = scan_package_files_with_limit(root.path(), |_| true, 32);
        let reordered_result = scan_package_files_with_limit(reordered.path(), |_| true, 32);

        assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
        assert_eq!(result.findings[0].rule, "analysis-incomplete");
        assert!(result.findings[0].message.contains("entry work limit"));
        assert!(!result.paths.is_empty());
        assert!(result.paths.len() < 32, "{:?}", result.paths);
        assert_eq!(result.paths, reordered_result.paths);
        assert_eq!(reordered_result.findings.len(), 1);
        assert_eq!(result.findings[0].file, reordered_result.findings[0].file);
    }

    #[test]
    fn an_oversized_directory_reports_its_whole_unscanned_scope() {
        let root = tempfile::tempdir().unwrap();
        for name in ["c.js", "b.js", "a.js"] {
            std::fs::write(root.path().join(name), "").unwrap();
        }

        let result = scan_package_files_with_limit(root.path(), |_| true, 2);

        assert!(result.paths.is_empty());
        assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
        assert_eq!(result.findings[0].rule, "analysis-incomplete");
        assert!(result.findings[0].message.contains("entry work limit"));
    }

    #[test]
    fn ignored_entries_still_count_toward_directory_work() {
        let root = tempfile::tempdir().unwrap();
        for name in [".git", ".hg", ".svn"] {
            std::fs::write(root.path().join(name), "").unwrap();
        }

        let result = scan_package_files_with_limit(root.path(), |_| true, 2);

        assert!(result.paths.is_empty());
        assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
        assert!(result.findings[0].message.contains("entry work limit"));
    }

    #[cfg(unix)]
    #[test]
    fn small_aliases_remain_visible_at_the_exact_entry_budget() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("real")).unwrap();
        std::fs::write(root.path().join("real/api.js"), "").unwrap();
        std::os::unix::fs::symlink("real", root.path().join("alias")).unwrap();

        let result = scan_package_files_with_limit(root.path(), |_| true, 4);

        assert!(result.findings.is_empty(), "{:?}", result.findings);
        assert_eq!(
            result.paths,
            BTreeSet::from(["alias/api.js".into(), "real/api.js".into()])
        );
    }

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
