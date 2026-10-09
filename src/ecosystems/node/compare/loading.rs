//! Detect synchronous module graphs that become asynchronous.

use crate::core::findings::{Finding, Severity};
use crate::ecosystems::node::discovery::{entry_points, resolver};
use crate::languages::javascript::syntax::grammar;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

type AsyncResult = Result<Option<PathBuf>, String>;

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
    let mut findings = Vec::new();
    for old in old_entries.iter().filter(|entry| require_accessible(entry)) {
        let Some(new) = new_entries
            .iter()
            .find(|entry| entry.subpath == old.subpath && entry.condition == old.condition)
        else {
            continue;
        };
        let before = old_graph.analyze(&old.path, &mut HashSet::new(), 0);
        let after = new_graph.analyze(&new.path, &mut HashSet::new(), 0);
        match (before, after) {
            (Ok(None), Ok(Some(cause))) => findings.push(Finding {
                severity: Severity::Major, rule: "module-became-async", file: new.path.strip_prefix(new_root).unwrap_or(&new.path).to_path_buf(), line: 1, col: 1,
                message: format!("subpath `{}` can no longer be loaded synchronously with require(): top-level await in `{}`", old.subpath, cause.strip_prefix(new_root).unwrap_or(&cause).display()),
            }),
            (Err(cause), _) | (_, Err(cause)) => findings.push(Finding {
                severity: Severity::Warning, rule: "analysis-incomplete", file: "package.json".into(), line: 1, col: 1,
                message: format!("cannot verify synchronous loading of `{}`: {cause}", old.subpath),
            }),
            _ => {}
        }
    }
    findings.sort_by(|a, b| a.message.cmp(&b.message));
    findings.dedup_by(|a, b| a.rule == b.rule && a.message == b.message);
    findings
}

fn require_accessible(entry: &entry_points::EntryPoint) -> bool {
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
            .any(|c| {
                c == "import" || c == "types" || c.starts_with("types@") || c == "typesVersions"
            })
}

struct ModuleGraph<'a> {
    root: &'a Path,
    cache: HashMap<PathBuf, AsyncResult>,
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
    ) -> AsyncResult {
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
            return Ok(None);
        }
        let result = self.analyze_file(&path, visited, depth);
        // A cycle-dependent negative result must not hide await in another root.
        if !matches!(result, Ok(None)) {
            self.cache.insert(path, result.clone());
        }
        result
    }

    fn analyze_file(
        &mut self,
        path: &Path,
        visited: &mut HashSet<PathBuf>,
        depth: usize,
    ) -> AsyncResult {
        if matches!(
            path.extension().and_then(|p| p.to_str()),
            Some("json" | "node")
        ) {
            return Ok(None);
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
        let mut parser = tree_sitter::Parser::new();
        let language = match language {
            grammar::SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            grammar::SourceLanguage::TypeScript => {
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
            }
            grammar::SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        };
        parser.set_language(&language).map_err(|e| e.to_string())?;
        let tree = parser.parse(&source, None).ok_or("module parse failed")?;
        let mut dependencies = Vec::new();
        if inspect(tree.root_node(), &source, &mut dependencies) {
            return Ok(Some(path.to_path_buf()));
        }
        for (specifier, mode) in dependencies {
            let target = if specifier.starts_with('#') {
                resolver::parse_imports_map_for_pkg(self.root).and_then(|map| {
                    resolver::resolve_imports_subpath(&map, &specifier, self.root, Some(mode))
                })
            } else if specifier.starts_with('.') || specifier.starts_with('/') {
                resolver::resolve_module_path(path, &specifier)
                    .into_iter()
                    .find(|p| p.is_file())
            } else {
                // External package execution is outside this packed module graph.
                // Its public types and dependency requirements are checked separately.
                continue;
            };
            let target = target
                .ok_or_else(|| format!("cannot resolve `{specifier}` from {}", path.display()))?;
            if let Some(cause) = self.analyze(&target, visited, depth + 1)? {
                return Ok(Some(cause));
            }
        }
        Ok(None)
    }
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
    if node.kind() == "await_expression" {
        return true;
    }
    if node.kind() == "for_in_statement" {
        let mut cursor = node.walk();
        if node.children(&mut cursor).any(|n| n.kind() == "await") {
            return true;
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
    let asynchronous = node
        .named_children(&mut cursor)
        .any(|child| inspect(child, source, dependencies));
    asynchronous
}
