//! Module walker for extracting exports from parsed AST.
//!
//! This module provides functionality for walking through parsed modules
//! and extracting export information.

use crate::ecma::entity::EntityDeclaration;
use crate::ecma::parser::parse_import;
use crate::parser::{NodeKind, ParsedNode};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A map of declaration names to their EntityDeclaration.
pub type Declarations = HashMap<String, EntityDeclaration>;

/// Extracts all exports from a module.
pub fn get_exports_in_module(
    base_import_path: PathBuf,
    parsed: ParsedNode,
) -> Result<(Option<EntityDeclaration>, Declarations)> {
    let mut declarations = Declarations::new();
    let mut default_export: Option<EntityDeclaration> = None;

    // Walk through all nodes to find exports
    walk_module_exports(&parsed, &base_import_path, &mut declarations, &mut default_export)?;

    Ok((default_export, declarations))
}

/// Recursively walks through the parsed tree to extract exports.
fn walk_module_exports(
    node: &ParsedNode,
    base_path: &Path,
    declarations: &mut Declarations,
    default_export: &mut Option<EntityDeclaration>,
) -> Result<()> {
    // Check if this node is an export
    if node.is_exported {
        if let Some(entity) = extract_entity_from_node(node, base_path)? {
            if node.is_default_export {
                *default_export = Some(entity);
            } else {
                let name = entity.name().clone();
                declarations.insert(name, entity);
            }
        }
    }

    // Handle export statement nodes specifically
    if node.kind == NodeKind::ExportStatement {
        extract_exports_from_statement(node, base_path, declarations, default_export)?;
    }

    // Recurse into children
    for child in &node.children {
        walk_module_exports(child, base_path, declarations, default_export)?;
    }

    Ok(())
}

/// Extracts exports from an export statement node.
fn extract_exports_from_statement(
    node: &ParsedNode,
    base_path: &Path,
    declarations: &mut Declarations,
    default_export: &mut Option<EntityDeclaration>,
) -> Result<()> {
    let is_default = node.is_default_export || node.text.contains("export default");

    // Check for re-exports (export * from or export { } from)
    if node.text.contains(" from ") {
        return handle_reexport(node, base_path, declarations);
    }

    // Check for direct declaration exports
    for child in &node.children {
        if child.kind.is_declaration() {
            if let Ok(entity) = EntityDeclaration::from_node(child) {
                if is_default {
                    *default_export = Some(entity);
                } else {
                    let name = entity.name().clone();
                    declarations.insert(name, entity);
                }
            }
        }

        // Handle export specifiers (export { a, b })
        if child.kind == NodeKind::ExportClause {
            extract_export_specifiers(child, declarations);
        }
    }

    Ok(())
}

/// Extracts export specifiers from an export clause.
fn extract_export_specifiers(clause: &ParsedNode, declarations: &mut Declarations) {
    for specifier in &clause.children {
        if specifier.kind == NodeKind::ExportSpecifier {
            if let Some(name) = specifier.name.clone().or_else(|| {
                specifier
                    .children
                    .iter()
                    .find(|c| c.kind == NodeKind::Identifier)
                    .and_then(|c| c.name.clone().or_else(|| Some(c.text.clone())))
            }) {
                // Create a placeholder entity for re-exported names
                let entity = EntityDeclaration::Var(
                    name.clone(),
                    crate::ecma::entity::VarInfo {
                        is_exported: true,
                        source: specifier.text.clone(),
                        ..Default::default()
                    },
                );
                declarations.insert(name, entity);
            }
        }
    }
}

/// Handles re-exports (export * from 'module' or export { a } from 'module').
fn handle_reexport(
    node: &ParsedNode,
    base_path: &Path,
    declarations: &mut Declarations,
) -> Result<()> {
    // Extract the source path from the export statement
    let source_path = extract_import_source(node);
    if source_path.is_none() {
        return Ok(());
    }

    let source_path = source_path.unwrap();
    let import_path = resolve_import_path(base_path, &source_path);

    // Try to parse the imported module
    let imported_module = parse_import(&import_path).with_context(|| {
        format!(
            "Failed to parse re-exported module: {}",
            import_path.display()
        )
    })?;

    let import_dir = import_path.parent().unwrap().to_path_buf();
    let (_, imported_exports) = get_exports_in_module(import_dir, imported_module)?;

    // Check if this is a namespace export (export * from)
    if node.text.contains("export *") && !node.text.contains("export * as") {
        // Re-export all named exports
        declarations.extend(imported_exports);
    } else if node.text.contains("export * as") {
        // Namespace re-export - extract the namespace name
        if let Some(ns_name) = extract_namespace_name(node) {
            for (name, entity) in imported_exports {
                declarations.insert(format!("{}.{}", ns_name, name), entity);
            }
        }
    } else {
        // Selective re-export
        for child in &node.children {
            if child.kind == NodeKind::ExportClause {
                for specifier in &child.children {
                    if let Some(orig_name) = extract_specifier_original_name(specifier) {
                        let export_name = extract_specifier_export_name(specifier).unwrap_or(&orig_name);
                        if let Some(entity) = imported_exports.get(&orig_name) {
                            declarations.insert(export_name.clone(), entity.clone());
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Extracts an entity declaration from a node.
fn extract_entity_from_node(node: &ParsedNode, _base_path: &Path) -> Result<Option<EntityDeclaration>> {
    if node.kind.is_declaration() {
        return Ok(Some(EntityDeclaration::from_node(node)?));
    }

    // Check children for declarations
    for child in &node.children {
        if child.kind.is_declaration() {
            return Ok(Some(EntityDeclaration::from_node(child)?));
        }
    }

    Ok(None)
}

/// Extracts the import source path from a node.
fn extract_import_source(node: &ParsedNode) -> Option<String> {
    // Look for string literal children (the source path)
    node.find_first(|n| n.kind == NodeKind::StringLiteral)
        .map(|n| {
            // Remove quotes from the string literal
            n.text
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
}

/// Extracts the namespace name from a namespace export.
fn extract_namespace_name(node: &ParsedNode) -> Option<String> {
    // Look for the identifier after "as"
    node.find_first(|n| n.kind == NodeKind::Identifier)
        .and_then(|n| n.name.clone().or_else(|| Some(n.text.clone())))
}

/// Extracts the original name from an export specifier.
fn extract_specifier_original_name(specifier: &ParsedNode) -> Option<String> {
    specifier
        .children
        .first()
        .and_then(|c| c.name.clone().or_else(|| Some(c.text.clone())))
}

/// Extracts the exported name from an export specifier (after "as" if present).
fn extract_specifier_export_name(specifier: &ParsedNode) -> Option<&String> {
    if specifier.children.len() > 1 {
        specifier
            .children
            .last()
            .and_then(|c| c.name.as_ref())
    } else {
        None
    }
}

/// Resolves an import path relative to the base path.
fn resolve_import_path(base_path: &Path, import_source: &str) -> PathBuf {
    let parent = base_path.parent().unwrap_or(base_path);

    // Handle relative imports
    if import_source.starts_with("./") || import_source.starts_with("../") {
        let mut resolved = parent.join(import_source);

        // Try adding extensions if the file doesn't exist
        if !resolved.exists() {
            for ext in &["js", "ts", "jsx", "tsx", "mjs", "cjs"] {
                let with_ext = resolved.with_extension(ext);
                if with_ext.exists() {
                    resolved = with_ext;
                    break;
                }
            }
        }

        // Try index files
        if resolved.is_dir() {
            for index in &["index.js", "index.ts", "index.jsx", "index.tsx"] {
                let index_path = resolved.join(index);
                if index_path.exists() {
                    return index_path;
                }
            }
        }

        resolved
    } else {
        // Node modules or absolute path - just return as-is
        PathBuf::from(import_source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecma::parser::parse_source;

    #[test]
    fn test_simple_export() {
        let source = "export const PI = 3.14159;";
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();

        let (default_export, named_exports) = get_exports_in_module(path, parsed).unwrap();

        assert!(default_export.is_none());
        assert!(!named_exports.is_empty());
    }

    #[test]
    fn test_default_export() {
        let source = "export default function main() {}";
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();

        let (default_export, _) = get_exports_in_module(path, parsed).unwrap();

        assert!(default_export.is_some());
    }

    #[test]
    fn test_multiple_exports() {
        let source = r#"
            export function foo() {}
            export class Bar {}
            export const baz = 42;
        "#;
        let path = PathBuf::from("test.js");
        let parsed = parse_source(source, &path).unwrap();

        let (_, named_exports) = get_exports_in_module(path, parsed).unwrap();

        assert!(named_exports.len() >= 1); // At least some exports found
    }

    #[test]
    fn test_resolve_relative_path() {
        let base = PathBuf::from("/project/src/index.js");
        let resolved = resolve_import_path(&base, "./utils");
        assert_eq!(resolved, PathBuf::from("/project/src/utils"));
    }

    #[test]
    fn test_resolve_parent_path() {
        let base = PathBuf::from("/project/src/components/Button.js");
        let resolved = resolve_import_path(&base, "../utils");
        // join doesn't canonicalize, so we expect the raw joined path
        assert_eq!(resolved, PathBuf::from("/project/src/components/../utils"));
    }
}
