//! Shared deterministic structural pipeline for reference verification and curation.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use interlint::{compare, discover::discover_exports};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub package: String,
    pub from_version: String,
    pub to_version: String,
    pub analysis_mode: String,
    pub exit_code: i32,
    pub breaking: bool,
    pub old_surfaces: BTreeMap<String, usize>,
    pub new_surfaces: BTreeMap<String, usize>,
    pub findings: Vec<Finding>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub severity: String,
    pub rule: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Warning {
    pub version: String,
    pub incomplete: bool,
    pub message: String,
}

pub fn run_pipeline(old_root: &Path, new_root: &Path) -> Snapshot {
    let old_root = old_root.canonicalize().unwrap();
    let new_root = new_root.canonicalize().unwrap();
    let normalize = |text: &str| {
        text.replace(&*old_root.to_string_lossy(), "<old>")
            .replace(&*new_root.to_string_lossy(), "<new>")
    };
    let (old, old_warnings) = discover_exports(&old_root).unwrap();
    let (new, new_warnings) = discover_exports(&new_root).unwrap();
    let mut findings = compare::compare_exports(&old, &new, &new_root, false);
    findings.extend(compare::metadata::compare_metadata(&old_root, &new_root));
    findings.extend(compare::compare_exports_map_diff(&old_root, &new_root));
    findings.extend(compare::compare_structural_diff(
        &old_root, &new_root, &old, &new,
    ));
    findings.extend(compare::compare_asset_diff(&old_root, &new_root));
    let mut seen = HashSet::new();
    findings.retain(|finding| {
        seen.insert((
            finding.rule,
            finding.file.clone(),
            finding.line,
            finding.col,
            finding.message.clone(),
        ))
    });
    let breaking = compare::has_breaking_changes(&findings);
    let mut findings: Vec<_> = findings
        .into_iter()
        .map(|finding| Finding {
            severity: format!("{:?}", finding.severity),
            rule: finding.rule.into(),
            file: normalize(&finding.file.to_string_lossy()),
            line: finding.line,
            col: finding.col,
            message: normalize(&finding.message),
        })
        .collect();
    findings.sort();
    let mut warnings: Vec<_> = [("old", old_warnings), ("new", new_warnings)]
        .into_iter()
        .flat_map(|(version, warnings)| {
            warnings.into_iter().map(move |warning| Warning {
                version: version.into(),
                incomplete: warning.incomplete,
                message: warning.message,
            })
        })
        .collect();
    for warning in &mut warnings {
        warning.message = normalize(&warning.message);
    }
    warnings.sort();
    warnings.dedup();
    let manifest = |root: &Path| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(root.join("package.json")).unwrap()).unwrap()
    };
    let before = manifest(&old_root);
    let after = manifest(&new_root);
    let surfaces = |exports: &interlint::types::ExportMap| {
        let mut counts = BTreeMap::new();
        for key in exports.keys() {
            *counts
                .entry(format!(
                    "{} | {} | {:?}",
                    key.subpath,
                    key.condition.as_deref().unwrap_or("default"),
                    key.space
                ))
                .or_default() += 1;
        }
        counts
    };
    Snapshot {
        package: after["name"].as_str().unwrap().into(),
        from_version: before["version"].as_str().unwrap().into(),
        to_version: after["version"].as_str().unwrap().into(),
        analysis_mode: "structural-only".into(),
        // Without LSP evidence the CLI returns incomplete, even with known breaks.
        exit_code: 3,
        breaking,
        old_surfaces: surfaces(&old),
        new_surfaces: surfaces(&new),
        findings,
        warnings,
    }
}
