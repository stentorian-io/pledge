// Comparison engine: diff old vs new ExportMaps

pub mod behavior_diff;
pub mod class_diff;
pub mod enum_diff;
pub mod exports_diff;
pub mod generics_diff;
pub mod metadata;
pub mod ref_diff;
pub mod subtype;
pub mod type_diff;

use std::path::Path;

use crate::discover::tree_query;
use crate::types::{ClassShape, EnumShape, ExportMap, Finding, Severity, TypeParam};

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

    // New exports (DET-03, DET-04, OUT-05): compatible additions reported as Minor
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
    // Use first overload (primary signature) for type diff
    for (name, old_sym) in old {
        if let Some(new_sym) = new.get(name) {
            if let (Some(old_shape), Some(new_shape)) =
                (old_sym.overloads.first(), new_sym.overloads.first())
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

/// Orchestrates exports map diffing (EXP-01..13).
/// Reads package.json from both roots and delegates to exports_diff::compare_exports_diff.
/// Per D-03: findings merge in compare/mod.rs, not in main.rs.
pub fn compare_exports_map_diff(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let old_pkg_path = old_root.join("package.json");
    let new_pkg_path = new_root.join("package.json");

    let old_content = match std::fs::read_to_string(&old_pkg_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let new_content = match std::fs::read_to_string(&new_pkg_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let old_pkg: serde_json::Value = match serde_json::from_str(&old_content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let new_pkg: serde_json::Value = match serde_json::from_str(&new_content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    exports_diff::compare_exports_diff(&old_pkg, &new_pkg, old_root, new_root)
}

/// Returns true if any finding has Major severity (per CLI-02).
pub fn has_breaking_changes(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Major)
}

// ─── Structural diff orchestrator ────────────────────────────────────────────

/// Owned extraction result for a single declaration node.
///
/// Created by `extract_declaration` which parses the file and immediately extracts
/// all needed data into owned types, avoiding lifetime issues with tree-sitter nodes.
#[allow(dead_code)]
struct ExtractedDecl {
    is_class: bool,
    is_enum: bool,
    is_function: bool,
    is_interface: bool,
    class_shape: Option<ClassShape>,
    enum_shape: Option<EnumShape>,
    type_params: Vec<TypeParam>,
    // NEW for Phase 8 REF rules:
    index_signatures: Vec<tree_query::IndexSignature>,
    type_guard_type: Option<String>,
    overloads: Vec<crate::types::shape::TypeShape>,
    // NEW for Phase 9 BEH-01/BEH-02:
    is_async: bool,
    is_generator: bool,
}

/// Parse `source` with the given language, find the declaration for `name` at
/// approximately `hint_line` (1-indexed), extract class/enum/type-param data
/// into owned structures, and return them.
///
/// Returns `None` when the declaration cannot be located (e.g., unsupported
/// language, parse failure, name not found in tree).
fn extract_declaration(
    source: &[u8],
    lang: &tree_query::SourceLanguage,
    name: &str,
    hint_line: u32,
) -> Option<ExtractedDecl> {
    let ts_lang: tree_sitter::Language = match lang {
        tree_query::SourceLanguage::TypeScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        tree_query::SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    };

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    // Walk top-level children of the program node looking for a declaration
    // whose name field matches `name` and whose start line is within ±3 of hint_line.
    //
    // Supported kinds:
    //   class_declaration, abstract_class_declaration,
    //   enum_declaration,
    //   function_declaration, interface_declaration, type_alias_declaration,
    //   lexical_declaration (export const/let — the variable_declarator carries the name)
    let hint = hint_line.saturating_sub(1) as usize; // 0-indexed

    let decl_node = find_decl_in_node(root, source, name, hint)?;
    let kind = decl_node.kind();

    let is_class = matches!(kind, "class_declaration" | "abstract_class_declaration");
    let is_enum = kind == "enum_declaration";
    let is_function = matches!(
        kind,
        "function_declaration" | "function_signature" | "generator_function_declaration"
    );
    let is_interface = kind == "interface_declaration";

    let class_shape = if is_class {
        Some(tree_query::parse_class_shape(source, decl_node))
    } else {
        None
    };

    let enum_shape = if is_enum {
        Some(tree_query::parse_enum_shape(source, decl_node))
    } else {
        None
    };

    // Per D-06: extract type params from ALL declaration types.
    let type_params = tree_query::parse_type_params(source, decl_node);

    // Extract index signatures for interfaces (REF-07)
    let index_signatures = if is_interface {
        tree_query::find_interface_body(decl_node)
            .map(|body| tree_query::extract_index_signatures(body, source))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Extract type guard for functions (REF-03)
    let type_guard_type = if is_function {
        tree_query::extract_type_guard(decl_node, source)
    } else {
        None
    };

    // Build overload shape for function signatures (REF-01, REF-02)
    let overloads = if is_function {
        tree_query::build_function_shape_from_node(decl_node, source)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    // BEH-01/BEH-02: extract async/generator modifiers from tree-sitter node
    let is_async_fn = if is_function {
        tree_query::is_async_function(decl_node, source)
    } else {
        false
    };
    let is_generator_fn = if is_function {
        tree_query::is_generator_function(decl_node, source)
    } else {
        false
    };

    Some(ExtractedDecl {
        is_class,
        is_enum,
        is_function,
        is_interface,
        class_shape,
        enum_shape,
        type_params,
        index_signatures,
        type_guard_type,
        overloads,
        is_async: is_async_fn,
        is_generator: is_generator_fn,
    })
}

/// Recursively search `node` and its children for a declaration whose name
/// matches `name` and whose start row is within ±3 of `hint_row` (0-indexed).
///
/// Returns the declaration node (not the name node).
fn find_decl_in_node<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
    name: &str,
    hint_row: usize,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();

        let candidate = match kind {
            "class_declaration" | "abstract_class_declaration" => {
                // name field is `type_identifier`
                child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .filter(|n| *n == name)
                    .map(|_| child)
            }
            "function_declaration" | "generator_function_declaration" => {
                child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .filter(|n| *n == name)
                    .map(|_| child)
            }
            "interface_declaration" => {
                child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .filter(|n| *n == name)
                    .map(|_| child)
            }
            "type_alias_declaration" => {
                child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .filter(|n| *n == name)
                    .map(|_| child)
            }
            "enum_declaration" => {
                child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .filter(|n| *n == name)
                    .map(|_| child)
            }
            "function_signature" => {
                child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .filter(|n| *n == name)
                    .map(|_| child)
            }
            "internal_module" => {
                child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .filter(|n| *n == name)
                    .map(|_| child)
            }
            "export_statement" | "lexical_declaration" | "variable_declaration" => {
                // Recurse into export/variable wrappers
                find_decl_in_node(child, source, name, hint_row)
            }
            _ => None,
        };

        if let Some(found) = candidate {
            // Accept if the row is within ±3 lines of the hint
            let found_row = found.start_position().row;
            if found_row.abs_diff(hint_row) <= 3 {
                return Some(found);
            }
        }

        // Recurse one level deeper into export_statement wrappers
        if matches!(kind, "export_statement") {
            if let Some(found) = find_decl_in_node(child, source, name, hint_row) {
                return Some(found);
            }
        }
    }
    None
}

/// Collect all overload signatures for a given function name from source.
///
/// Scans for `function_signature` nodes with matching name at the top level
/// (exported overload declarations). Returns Vec of TypeShape::Function,
/// one per function_signature node found.
fn collect_overload_shapes(
    source: &[u8],
    lang: &tree_query::SourceLanguage,
    name: &str,
) -> Vec<crate::types::shape::TypeShape> {
    let ts_lang: tree_sitter::Language = match lang {
        tree_query::SourceLanguage::TypeScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        tree_query::SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&ts_lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let mut overloads = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        // Look for export_statement wrapping function_signature
        if child.kind() == "export_statement" {
            let mut ec = child.walk();
            for export_child in child.children(&mut ec) {
                if export_child.kind() == "function_signature" {
                    let matches_name = export_child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|n| n == name)
                        .unwrap_or(false);
                    if matches_name {
                        if let Some(shape) =
                            tree_query::build_function_shape_from_node(export_child, source)
                        {
                            overloads.push(shape);
                        }
                    }
                }
            }
        }
    }
    overloads
}

/// Orchestrates class, enum, and generic type parameter diff for all exported symbols.
///
/// For each exported symbol present in both old and new:
/// - Reads the source file from both package roots
/// - Parses with tree-sitter to find the declaration node
/// - Extracts ClassShape / EnumShape / TypeParams depending on declaration type
/// - Runs the appropriate diff function
///
/// Per D-06: generic params are extracted from ALL declaration types
/// (class, function, interface, type alias), not just classes.
pub fn compare_structural_diff(
    old_root: &Path,
    new_root: &Path,
    old_exports: &ExportMap,
    new_exports: &ExportMap,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    for (name, old_sym) in old_exports {
        let Some(new_sym) = new_exports.get(name) else {
            continue;
        };

        // Build absolute paths to the source files
        let old_path = old_root.join(&old_sym.file);
        let new_path = new_root.join(&new_sym.file);

        let Ok(old_source) = std::fs::read(&old_path) else {
            continue;
        };
        let Ok(new_source) = std::fs::read(&new_path) else {
            continue;
        };

        // Determine language from file extension
        let Some(old_lang) = tree_query::language_for_file(&old_sym.file) else {
            continue;
        };
        let Some(new_lang) = tree_query::language_for_file(&new_sym.file) else {
            continue;
        };

        // Extract all shapes from both versions
        let Some(old_decl) =
            extract_declaration(&old_source, &old_lang, name, old_sym.line)
        else {
            continue;
        };
        let Some(new_decl) =
            extract_declaration(&new_source, &new_lang, name, new_sym.line)
        else {
            continue;
        };

        // CLS-01..09: class diff
        if old_decl.is_class && new_decl.is_class {
            if let (Some(old_shape), Some(new_shape)) =
                (&old_decl.class_shape, &new_decl.class_shape)
            {
                findings.extend(class_diff::compare_class_shapes(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                ));
            }
        }

        // ENUM-01..05: enum diff
        if old_decl.is_enum && new_decl.is_enum {
            if let (Some(old_shape), Some(new_shape)) =
                (&old_decl.enum_shape, &new_decl.enum_shape)
            {
                findings.extend(enum_diff::compare_enum_shapes(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                ));
            }
        }

        // GEN-01..06: generic type param diff — applies to ALL constructs per D-06
        if !old_decl.type_params.is_empty() || !new_decl.type_params.is_empty() {
            findings.extend(generics_diff::compare_type_params(
                &old_decl.type_params,
                &new_decl.type_params,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // REF-01..05, REF-07, REF-08: signature & type refinement diff
        // Supplement ExportSymbol data with tree-sitter extraction results
        // (LSP may not have set these fields)
        let mut old_sym_augmented = old_sym.clone();
        let mut new_sym_augmented = new_sym.clone();

        if old_sym_augmented.type_guard_type.is_none() {
            old_sym_augmented.type_guard_type = old_decl.type_guard_type.clone();
        }
        if new_sym_augmented.type_guard_type.is_none() {
            new_sym_augmented.type_guard_type = new_decl.type_guard_type.clone();
        }

        // BEH-01/BEH-02: augment async/generator from tree-sitter extraction
        if old_decl.is_function {
            old_sym_augmented.is_async = old_decl.is_async;
            old_sym_augmented.is_generator = old_decl.is_generator;
        }
        if new_decl.is_function {
            new_sym_augmented.is_async = new_decl.is_async;
            new_sym_augmented.is_generator = new_decl.is_generator;
        }

        // Collect all overload shapes for functions (REF-01)
        if old_decl.is_function {
            let collected = collect_overload_shapes(&old_source, &old_lang, name);
            if !collected.is_empty() {
                old_sym_augmented.overloads = collected;
            } else if old_sym_augmented.overloads.is_empty() && !old_decl.overloads.is_empty() {
                old_sym_augmented.overloads = old_decl.overloads.clone();
            }
        }
        if new_decl.is_function {
            let collected = collect_overload_shapes(&new_source, &new_lang, name);
            if !collected.is_empty() {
                new_sym_augmented.overloads = collected;
            } else if new_sym_augmented.overloads.is_empty() && !new_decl.overloads.is_empty() {
                new_sym_augmented.overloads = new_decl.overloads.clone();
            }
        }

        findings.extend(ref_diff::compare_ref_changes(
            &old_sym_augmented,
            &new_sym_augmented,
            name,
            &new_sym.file,
            new_sym.line,
            new_sym.col,
            &old_decl.index_signatures,
            &new_decl.index_signatures,
        ));

        // BEH-01, BEH-02: behavioral modifier diff (async/generator)
        if old_decl.is_function && new_decl.is_function {
            findings.extend(behavior_diff::compare_behavior(
                &old_sym_augmented,
                &new_sym_augmented,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }
    }

    findings
}

/// Produce a synthetic list of findings for a first-publish package (PKG-03).
///
/// All current exports are treated as `export-added` minor findings.
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
