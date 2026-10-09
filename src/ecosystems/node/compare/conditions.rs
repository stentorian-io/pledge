//! Compare APIs selected by ordered conditional branches.

use crate::core::findings;
use crate::core::findings::{Finding, Severity};
use crate::ecosystems::node::compare as package_compare;
use crate::ecosystems::node::compare::exports::{ConditionNode, ExportsMap};
use crate::ecosystems::node::discovery::resolver;
use crate::languages::javascript::compare as javascript_compare;
use crate::languages::javascript::compare::structure;
use crate::languages::javascript::model::ExportMap;
use crate::languages::typescript::config as tsconfig;
use crate::languages::typescript::types::reachability as type_graph;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

pub(super) fn compare_priority(
    old: &ExportsMap,
    new: &ExportsMap,
    old_root: &Path,
    new_root: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (subpath, old_entry) in old {
        let Some(new_entry) = new.get(subpath) else {
            continue;
        };
        if !order_changed(&old_entry.node, &new_entry.node) {
            continue;
        }
        let mut paths = Vec::new();
        leaf_conditions(&old_entry.node, &BTreeSet::new(), &mut paths);
        leaf_conditions(&new_entry.node, &BTreeSet::new(), &mut paths);
        if paths.len() > 128 {
            findings.push(Finding {
                severity: Severity::Warning,
                rule: "analysis-incomplete",
                file: "package.json".into(),
                line: 1,
                col: 1,
                message: format!("condition-priority analysis limit reached for `{subpath}`"),
            });
            continue;
        }
        let mut old_cache = HashMap::new();
        let mut new_cache = HashMap::new();
        let mut pairs = HashSet::new();
        for left in &paths {
            for right in &paths {
                let active: BTreeSet<_> = left.union(right).cloned().collect();
                if active.contains("import") && active.contains("require") {
                    continue;
                }
                let before =
                    package_compare::exports::select_condition(&old_entry.node, &active).flatten();
                let after =
                    package_compare::exports::select_condition(&new_entry.node, &active).flatten();
                if before == after || !pairs.insert((before, after)) {
                    continue;
                }
                let old_surface = old_cache
                    .entry(before)
                    .or_insert_with(|| surface(old_root, before));
                let new_surface = new_cache
                    .entry(after)
                    .or_insert_with(|| surface(new_root, after));
                let severity = match (old_surface, new_surface) {
                    (Some(old), Some(new)) => {
                        let mut changes =
                            javascript_compare::compare_exports(old, new, new_root, false);
                        changes.extend(structure::compare_structural_diff(
                            old_root, new_root, old, new,
                        ));
                        if findings::has_breaking_changes(&changes) {
                            Severity::Major
                        } else {
                            Severity::Minor
                        }
                    }
                    _ => Severity::Warning,
                };
                findings.push(Finding { severity, rule: "condition-priority-changed", file: "package.json".into(), line: 1, col: 1,
                    message: format!("condition priority changed the selected API for `{subpath}` with conditions [{}] from {} to {}", active.into_iter().collect::<Vec<_>>().join(", "), before.unwrap_or("null"), after.unwrap_or("null")) });
            }
        }
    }
    findings
}

fn order_changed(old: &ConditionNode, new: &ConditionNode) -> bool {
    match (old, new) {
        (ConditionNode::Branch(old), ConditionNode::Branch(new)) => {
            let before: Vec<_> = old.keys().filter(|k| new.contains_key(*k)).collect();
            let after: Vec<_> = new.keys().filter(|k| old.contains_key(*k)).collect();
            before != after
                || old
                    .iter()
                    .any(|(key, child)| new.get(key).is_some_and(|next| order_changed(child, next)))
        }
        _ => false,
    }
}

fn leaf_conditions(
    node: &ConditionNode,
    active: &BTreeSet<String>,
    paths: &mut Vec<BTreeSet<String>>,
) {
    match node {
        ConditionNode::Leaf(_) => paths.push(active.clone()),
        ConditionNode::Branch(branches) => {
            for (condition, child) in branches {
                let mut active = active.clone();
                if condition != "default" {
                    active.insert(condition.clone());
                }
                leaf_conditions(child, &active, paths);
            }
        }
    }
}

fn surface(root: &Path, target: Option<&str>) -> Option<ExportMap> {
    let mut exports = ExportMap::new();
    let Some(target) = target else {
        return Some(exports);
    };
    let root = root.canonicalize().ok()?;
    let path = root.join(target.strip_prefix("./").unwrap_or(target));
    let mut warnings = Vec::new();
    resolver::collect_exports(
        &path,
        &root,
        &mut exports,
        &mut HashSet::new(),
        &mut warnings,
        0,
        tsconfig::parse_tsconfig_paths(&root).as_ref(),
        resolver::parse_imports_map_for_pkg(&root).as_ref(),
        ".",
        None,
    )
    .ok()?;
    if warnings.iter().any(|w| w.incomplete) {
        return None;
    }
    type_graph::resolve_public_types(&mut exports, &root, false);
    Some(exports)
}
