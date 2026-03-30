// Comparison engine: diff old vs new ExportMaps

pub mod metadata;
pub mod subtype;
pub mod type_diff;

use std::path::PathBuf;

use crate::types::{ExportMap, Finding, Severity};

/// Compare two export maps and produce a list of findings.
///
/// - Exports present in `old` but absent in `new` → `export-removed` (Major)
/// - Exports present in `new` but absent in `old` → `export-added` (Minor)
///
/// Findings are sorted by severity: Major first, then Minor, then Patch (D-01).
pub fn compare_exports(old: &ExportMap, new: &ExportMap) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    // Removed exports (DET-01, DET-02)
    for (name, sym) in old {
        if !new.contains_key(name) {
            let message = if name == "default" {
                "default export was removed".to_string()
            } else {
                format!("exported symbol `{}` was removed", name)
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message,
            });
        }
    }

    // New exports (for completeness — Phase 3 DET-03/DET-04 will refine)
    for (name, sym) in new {
        if !old.contains_key(name) {
            findings.push(Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!("exported symbol `{}` was added", name),
            });
        }
    }

    // Type-aware comparison (D-08, D-09): only when both symbols have TypeShape
    for (name, old_sym) in old {
        if let Some(new_sym) = new.get(name) {
            if let (Some(old_shape), Some(new_shape)) =
                (&old_sym.type_shape, &new_sym.type_shape)
            {
                let type_findings = type_diff::diff_types(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                );
                findings.extend(type_findings);
            }
        }
    }

    // Sort by severity: Major < Minor < Patch (derived Ord on Severity enum) per D-01
    findings.sort_by(|a, b| a.severity.cmp(&b.severity));

    findings
}

/// Returns true if any finding has Major severity (per CLI-02).
pub fn has_breaking_changes(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Major)
}

/// Produce a synthetic list of findings for a first-publish package (PKG-03).
///
/// All current exports are treated as `export-added` minor findings.
#[allow(dead_code)]
pub fn first_publish_findings(exports: &ExportMap) -> Vec<Finding> {
    let mut findings: Vec<Finding> = exports
        .iter()
        .map(|(name, sym)| Finding {
            severity: Severity::Minor,
            rule: "export-added",
            file: sym.file.clone(),
            line: sym.line,
            col: sym.col,
            message: format!("exported symbol `{}` was added", name),
        })
        .collect();

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
    });

    findings
}

/// A zero-sized placeholder used only when a PathBuf is needed but no file
/// position is available (e.g. synthetic findings). Not used in production paths.
#[allow(dead_code)]
pub(crate) fn unknown_path() -> PathBuf {
    PathBuf::from("<unknown>")
}
