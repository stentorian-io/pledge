//! Compare runtime loading and syntax requirements of local module graphs.

use crate::core::findings::{Finding, Severity};
use crate::ecosystems::node::compare::metadata::parse_engine_range;
use crate::ecosystems::node::compare::rules::RULE_NODE_SYNTAX_REQUIREMENT_RAISED;
use crate::ecosystems::node::discovery::{entry_points, resolver};
use crate::languages::javascript::syntax::grammar;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
struct ModuleRequirements {
    asynchronous: Option<PathBuf>,
    resources: Option<PathBuf>,
}

type RequirementsResult = Result<ModuleRequirements, String>;

pub(super) fn compare_module_loading(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let (Ok(old_canonical), Ok(new_canonical)) = (old_root.canonicalize(), new_root.canonicalize())
    else {
        return Vec::new();
    };
    let (old_root, new_root) = (old_canonical.as_path(), new_canonical.as_path());
    let (Ok(old_entries), Ok(new_entries)) = (
        entry_points::resolve_entry_points(old_root),
        entry_points::resolve_entry_points(new_root),
    ) else {
        return Vec::new();
    };
    let mut old_graph = ModuleGraph::new(old_root);
    let mut new_graph = ModuleGraph::new(new_root);
    let old_node = node_engine(old_root);
    let new_node = node_engine(new_root);
    let mut findings = Vec::new();
    for old in old_entries.iter().filter(|entry| runtime_entry(entry)) {
        let Some(new) = new_entries.iter().find(|entry| {
            runtime_entry(entry) && entry.subpath == old.subpath && entry.condition == old.condition
        }) else {
            continue;
        };
        let before = old_graph.analyze(&old.path, &mut HashSet::new(), 0);
        let after = new_graph.analyze(&new.path, &mut HashSet::new(), 0);
        match (before, after) {
            (Ok(before), Ok(after)) => {
                if require_accessible(old) && before.asynchronous.is_none() {
                    if let Some(cause) = after.asynchronous {
                        findings.push(Finding {
                            severity: Severity::Major,
                            rule: "module-became-async",
                            file: new.path.strip_prefix(new_root).unwrap_or(&new.path).to_path_buf(),
                            line: 1,
                            col: 1,
                            message: format!(
                                "subpath `{}` can no longer be loaded synchronously with require(): top-level await in `{}`",
                                old.subpath,
                                cause.strip_prefix(new_root).unwrap_or(&cause).display(),
                            ),
                        });
                    }
                }
                if before.resources.is_none() {
                    if let Some(cause) = after.resources {
                        if let Some(finding) = resource_syntax_finding(
                            &cause,
                            new_root,
                            old_node.as_ref(),
                            new_node.as_ref(),
                        ) {
                            findings.push(finding);
                        }
                    }
                }
            }
            (Err(cause), _) | (_, Err(cause)) => findings.push(Finding {
                severity: Severity::Warning,
                rule: "analysis-incomplete",
                file: "package.json".into(),
                line: 1,
                col: 1,
                message: format!("cannot verify module loading of `{}`: {cause}", old.subpath),
            }),
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings.dedup_by(|a, b| a.rule == b.rule && a.message == b.message);
    findings
}

fn require_accessible(entry: &entry_points::EntryPoint) -> bool {
    runtime_entry(entry)
        && !entry
            .condition
            .as_deref()
            .unwrap_or("")
            .split('/')
            .any(|condition| condition == "import")
}

pub(super) fn runtime_entry(entry: &entry_points::EntryPoint) -> bool {
    let declaration = entry
        .path
        .file_name()
        .and_then(|p| p.to_str())
        .is_some_and(|p| p.ends_with(".d.ts") || p.ends_with(".d.mts") || p.ends_with(".d.cts"));
    !declaration
        && !entry
            .condition
            .as_deref()
            .unwrap_or("")
            .split('/')
            .any(|c| c == "types" || c.starts_with("types@") || c == "typesVersions")
}

fn node_engine(root: &Path) -> Option<node_semver::Range> {
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("package.json")).ok()?).ok()?;
    parse_engine_range(manifest.get("engines")?.get("node")?.as_str()?)
}

fn resource_syntax_finding(
    cause: &Path,
    root: &Path,
    old_node: Option<&node_semver::Range>,
    new_node: Option<&node_semver::Range>,
) -> Option<Finding> {
    // Feature support is tied to the Node release line. Do not infer that a
    // prerelease of Node 24 lacks syntax based only on its semver precedence.
    let unsupported = parse_engine_range("<24.0.0-0")?;
    if new_node.is_some_and(|range| range.intersect(&unsupported).is_none()) {
        return None;
    }
    let (severity, rule, message) = match (old_node, new_node) {
        (Some(old), Some(new)) => {
            old.intersect(new)?.intersect(&unsupported)?;
            (Severity::Major, RULE_NODE_SYNTAX_REQUIREMENT_RAISED, "new resource-management syntax requires Node.js 24.0.0, but engines.node still accepts previously supported older runtimes".to_string())
        }
        _ => (Severity::Warning, "analysis-incomplete", "new resource-management syntax requires Node.js 24.0.0; missing or invalid engines.node prevents verifying runtime compatibility".to_string()),
    };
    Some(Finding {
        severity,
        rule,
        file: cause.strip_prefix(root).unwrap_or(cause).to_path_buf(),
        line: 1,
        col: 1,
        message,
    })
}

struct ModuleGraph<'a> {
    root: &'a Path,
    cache: HashMap<PathBuf, RequirementsResult>,
}

impl<'a> ModuleGraph<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            cache: HashMap::new(),
        }
    }

    fn analyze(
        &mut self,
        path: &Path,
        visited: &mut HashSet<PathBuf>,
        depth: usize,
    ) -> RequirementsResult {
        if depth >= 64 {
            return Err("module traversal depth limit reached".into());
        }
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(result) = self.cache.get(&path) {
            return result.clone();
        }
        if !visited.insert(path.clone()) {
            return Ok(ModuleRequirements::default());
        }
        let result = self.analyze_file(&path, visited, depth);
        // A cycle-dependent negative result must not hide await in another root.
        if match &result {
            Err(_) => true,
            Ok(requirements) => {
                requirements.asynchronous.is_some() && requirements.resources.is_some()
            }
        } {
            self.cache.insert(path, result.clone());
        }
        result
    }

    fn analyze_file(
        &mut self,
        path: &Path,
        visited: &mut HashSet<PathBuf>,
        depth: usize,
    ) -> RequirementsResult {
        if matches!(
            path.extension().and_then(|p| p.to_str()),
            Some("json" | "node")
        ) {
            return Ok(ModuleRequirements::default());
        }
        let source = std::fs::read(path).map_err(|e| e.to_string())?;
        if let Some(reason) = crate::core::source::classify(&source) {
            return Err(format!(
                "{} source skipped: {}",
                reason.label(),
                path.display()
            ));
        }
        let language = grammar::language_for_file(path)
            .ok_or_else(|| format!("unsupported module {}", path.display()))?;
        let tree = grammar::parse_source(&source, &language).map_err(|error| error.to_string())?;
        let mut dependencies = Vec::new();
        let mut requirements = ModuleRequirements {
            asynchronous: inspect(tree.root_node(), &source, &mut dependencies)
                .then(|| path.to_path_buf()),
            resources: contains_resource_management(tree.root_node()).then(|| path.to_path_buf()),
        };
        for (specifier, mode) in dependencies {
            let target = if specifier.starts_with('#') {
                resolver::parse_imports_map_for_pkg(self.root).and_then(|map| {
                    resolver::resolve_imports_subpath(&map, &specifier, self.root, Some(mode))
                })
            } else if specifier.starts_with('.') || specifier.starts_with('/') {
                resolve_runtime_dependency(path, &specifier, mode)
            } else {
                // External package execution is outside this packed module graph.
                // Its public types and dependency requirements are checked separately.
                continue;
            };
            let target = target
                .ok_or_else(|| format!("cannot resolve `{specifier}` from {}", path.display()))?;
            let dependency = self.analyze(&target, visited, depth + 1)?;
            requirements.asynchronous = requirements.asynchronous.or(dependency.asynchronous);
            requirements.resources = requirements.resources.or(dependency.resources);
        }
        Ok(requirements)
    }
}

fn contains_resource_management(root: tree_sitter::Node) -> bool {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.kind() == "using_declaration" {
            return true;
        }
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor));
    }
    false
}

pub(super) fn resolve_runtime_dependency(
    from: &Path,
    specifier: &str,
    mode: &str,
) -> Option<PathBuf> {
    let target = from.parent()?.join(specifier);
    if mode == "import" {
        return target.is_file().then_some(target);
    }
    fn file(target: &Path) -> Option<PathBuf> {
        if target.is_file() {
            return Some(target.to_path_buf());
        }
        ["js", "json", "node"]
            .iter()
            .map(|extension| {
                let mut path = target.as_os_str().to_os_string();
                path.push(format!(".{extension}"));
                PathBuf::from(path)
            })
            .find(|path| path.is_file())
    }
    if let Some(path) = file(&target) {
        return Some(path);
    }
    if let Some(main) = std::fs::read_to_string(target.join("package.json"))
        .ok()
        .and_then(|source| serde_json::from_str::<serde_json::Value>(&source).ok())
        .and_then(|manifest| {
            manifest
                .get("main")
                .and_then(|main| main.as_str())
                .map(str::to_owned)
        })
    {
        let main = target.join(main);
        if let Some(path) = file(&main).or_else(|| file(&main.join("index"))) {
            return Some(path);
        }
    }
    file(&target.join("index"))
}

fn inspect(
    node: tree_sitter::Node,
    source: &[u8],
    dependencies: &mut Vec<(String, &'static str)>,
) -> bool {
    if matches!(
        node.kind(),
        "function_declaration"
            | "function_expression"
            | "generator_function_declaration"
            | "generator_function"
            | "arrow_function"
            | "method_definition"
    ) {
        return false;
    }
    let mut asynchronous = node.kind() == "await_expression";
    if node.kind() == "using_declaration" {
        let mut cursor = node.walk();
        if node
            .children(&mut cursor)
            .any(|child| child.kind() == "await")
        {
            asynchronous = true;
        }
    }
    if node.kind() == "for_in_statement" {
        let mut cursor = node.walk();
        if node.children(&mut cursor).any(|n| n.kind() == "await") {
            asynchronous = true;
        }
    }
    if matches!(node.kind(), "import_statement" | "export_statement") {
        let text = node.utf8_text(source).unwrap_or("").trim_start();
        if text.starts_with("import type ") || text.starts_with("export type ") {
            return false;
        }
        if let Some(specifier) = node
            .child_by_field_name("source")
            .and_then(|n| n.utf8_text(source).ok())
        {
            dependencies.push((specifier.trim_matches(['\'', '"']).to_string(), "import"));
        }
    }
    if node.kind() == "call_expression"
        && node
            .child_by_field_name("function")
            .and_then(|n| n.utf8_text(source).ok())
            == Some("require")
    {
        if let Some(argument) = node
            .child_by_field_name("arguments")
            .and_then(|n| n.named_child(0))
            .filter(|n| n.kind() == "string")
        {
            dependencies.push((
                argument
                    .utf8_text(source)
                    .unwrap_or("")
                    .trim_matches(['\'', '"'])
                    .to_string(),
                "require",
            ));
        }
    }
    let mut cursor = node.walk();
    // Keep collecting dependencies after finding an await in an earlier child.
    for child in node.named_children(&mut cursor) {
        asynchronous |= inspect(child, source, dependencies);
    }
    asynchronous
}
