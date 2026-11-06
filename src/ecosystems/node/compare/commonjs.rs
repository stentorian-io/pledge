//! Compare the Node loader's synthetic CommonJS named-import contracts.

use super::rules::RULE_CJS_NAMED_EXPORT_REMOVED;
use crate::core::findings::{Finding, Severity};
use crate::ecosystems::node::discovery::commonjs::NamedImports;
use std::path::{Path, PathBuf};

pub(super) fn compare_named_imports(
    before: &NamedImports,
    after: &NamedImports,
    current_root: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (side, evidence) in [("baseline", before), ("current", after)] {
        for error in evidence
            .errors
            .iter()
            .cloned()
            .chain(evidence.entries.iter().filter_map(|entry| {
                entry
                    .error
                    .as_ref()
                    .map(|error| format!("subpath `{}`: {error}", entry.subpath))
            }))
        {
            findings.push(Finding {
                severity: Severity::Warning,
                rule: "analysis-incomplete",
                message: format!("{side}: cannot verify CommonJS named imports: {error}"),
                file: PathBuf::from("package.json"),
                line: 1,
                col: 1,
            });
        }
    }
    for old in &before.entries {
        let Some(old_names) = &old.names else {
            continue;
        };
        let Some(new) = after.entries.iter().find(|entry| {
            entry.subpath == old.subpath && entry.context == old.context && entry.names.is_some()
        }) else {
            continue;
        };
        let Some(new_names) = &new.names else {
            continue;
        };
        for name in old_names.difference(new_names) {
            findings.push(Finding {
                severity: Severity::Major,
                rule: RULE_CJS_NAMED_EXPORT_REMOVED,
                message: format!("named ESM import `{name}` is no longer exposed by CommonJS subpath `{}` under Node {}", old.subpath, after.node),
                file: new.path.strip_prefix(current_root).unwrap_or(&new.path).to_path_buf(),
                line: 1,
                col: 1,
            });
        }
    }
    findings.sort_by(|left, right| left.message.cmp(&right.message));
    findings.dedup_by(|left, right| left.rule == right.rule && left.message == right.message);
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecosystems::node::discovery::commonjs::ImportEntry;

    fn evidence(names: Option<&[&str]>, context: &[&str]) -> NamedImports {
        NamedImports {
            node: "v24.0.0".into(),
            entries: vec![ImportEntry {
                path: PathBuf::from("/package/index.cjs"),
                subpath: ".".into(),
                condition: None,
                context: context.iter().map(|name| (*name).into()).collect(),
                names: names.map(|names| names.iter().map(|name| (*name).into()).collect()),
                error: None,
            }],
            errors: Vec::new(),
        }
    }

    #[test]
    fn missing_evidence_is_distinct_from_a_verified_empty_namespace() {
        let old = evidence(Some(&["api"]), &[]);
        let absent = evidence(None, &[]);
        assert!(compare_named_imports(&old, &absent, Path::new("/package")).is_empty());
        assert!(compare_named_imports(&absent, &old, Path::new("/package")).is_empty());

        let empty = evidence(Some(&[]), &[]);
        let findings = compare_named_imports(&old, &empty, Path::new("/package"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RULE_CJS_NAMED_EXPORT_REMOVED);
        assert_eq!(findings[0].severity, Severity::Major);
        assert_eq!(findings[0].file, Path::new("index.cjs"));
    }

    #[test]
    fn named_imports_require_the_same_condition_context_and_selected_entry() {
        let old = evidence(Some(&["api"]), &["browser"]);
        let other_context = evidence(Some(&[]), &[]);
        assert!(compare_named_imports(&old, &other_context, Path::new("/package")).is_empty());

        let mut new = evidence(None, &["browser"]);
        let mut selected = evidence(Some(&["api"]), &["browser"]).entries.remove(0);
        selected.condition = Some("default".into());
        new.entries.push(selected);
        assert!(compare_named_imports(&old, &new, Path::new("/package")).is_empty());
    }

    #[test]
    fn parser_failures_report_missing_evidence_without_inventing_removals() {
        let old = evidence(Some(&["api"]), &[]);
        let mut new = evidence(None, &[]);
        new.entries[0].error = Some("unresolved local re-export".into());
        new.errors.push("installed parser is unavailable".into());

        let findings = compare_named_imports(&old, &new, Path::new("/package"));
        assert_eq!(findings.len(), 2);
        assert!(findings
            .iter()
            .all(|finding| finding.rule == "analysis-incomplete"
                && finding.severity == Severity::Warning));
    }
}
