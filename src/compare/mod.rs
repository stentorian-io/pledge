// Comparison engine: diff old vs new ExportMaps

pub mod asset_diff;
pub mod behavior_diff;
pub mod class_diff;
pub mod css_diff;
pub mod enum_diff;
pub mod exports_diff;
pub mod generics_diff;
pub mod interface_diff;
pub mod json_diff;
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

/// Orchestrates asset & schema comparison (AST-01..06).
/// Runs JSON diff, CSS diff, and asset file diff, merging all findings.
pub fn compare_asset_diff(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // AST-01, AST-02, AST-06: JSON field diff
    findings.extend(json_diff::compare_json_files(old_root, new_root));

    // AST-03, AST-04: CSS custom property and class name diff
    findings.extend(css_diff::compare_css_files(old_root, new_root));

    // AST-05: Asset file presence diff
    findings.extend(asset_diff::compare_asset_files(old_root, new_root));

    findings
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

/// Check if an arrow_function node has the `async` keyword.
///
/// Arrow functions have no `"function"` keyword — instead, `"async"` appears
/// before the `"parameters"` or `"identifier"` children. Per Phase 14 Research Pitfall 7.
fn is_async_arrow(arrow_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = arrow_node.walk();
    for child in arrow_node.children(&mut cursor) {
        // Past the point where "async" would appear
        if child.kind() == "formal_parameters" || child.kind() == "identifier" {
            break;
        }
        if !child.is_named() && child.utf8_text(source).ok() == Some("async") {
            return true;
        }
    }
    false
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

    // REQ-14-01: arrow_function and function_expression in variables
    // REQ-14-02: function_expression and generator_function_expression as anonymous defaults
    // Note: tree-sitter uses "function_expression" for `function() {}` values (not "function").
    let is_arrow_function = kind == "arrow_function";
    let is_anonymous_function =
        matches!(kind, "function_expression" | "generator_function_expression");
    let is_anonymous_class = kind == "class";

    // Treat arrow functions and anonymous functions as function declarations
    let is_function = matches!(
        kind,
        "function_declaration" | "function_signature" | "generator_function_declaration"
    ) || is_arrow_function
        || is_anonymous_function;

    // REQ-14-02: anonymous class nodes (export default class {}) treated as classes
    let is_class =
        matches!(kind, "class_declaration" | "abstract_class_declaration") || is_anonymous_class;
    let is_enum = kind == "enum_declaration";
    let is_interface = kind == "interface_declaration";

    // parse_class_shape handles both class_declaration and class (anonymous) nodes
    // since both have a class_body child
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
    // is_function now includes arrow functions, so extract_type_guard works automatically
    let type_guard_type = if is_function {
        tree_query::extract_type_guard(decl_node, source)
    } else {
        None
    };

    // Build overload shape for function signatures (REF-01, REF-02)
    // build_function_shape_from_node uses child_by_field_name("parameters") which
    // works on arrow_function nodes (field name is also "parameters")
    let overloads = if is_function {
        tree_query::build_function_shape_from_node(decl_node, source)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    // BEH-01/BEH-02: extract async/generator modifiers from tree-sitter node
    // Arrow functions use is_async_arrow because they lack the "function" keyword
    let is_async_fn = if is_arrow_function {
        is_async_arrow(decl_node, source)
    } else if is_function {
        tree_query::is_async_function(decl_node, source)
    } else {
        false
    };
    // Arrow functions cannot be generators (JS/TS spec)
    let is_generator_fn = if is_arrow_function {
        false
    } else if is_function {
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
/// matches `name`. Returns the declaration node closest to `hint_row`
/// (0-indexed).
///
/// Uses a closest-match strategy: collects all candidates matching by name,
/// then returns the one nearest to `hint_row`. This handles file moves
/// between versions (GAP-13-05) and ambient declarations in .d.ts files
/// (BUG-13-01).
fn find_decl_in_node<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
    name: &str,
    hint_row: usize,
) -> Option<tree_sitter::Node<'a>> {
    // REQ-14-03: Handle dotted names (namespace members) by finding the namespace
    // first, then searching inside its body for the member.
    if name.contains('.') {
        let parts: Vec<&str> = name.splitn(2, '.').collect();
        let ns_name = parts[0];
        let member_name = parts[1];
        // Find the internal_module node for the namespace by scanning children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "internal_module" => {
                    let matches_ns = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|n| n == ns_name)
                        .unwrap_or(false);
                    if matches_ns {
                        if let Some(body) = child.child_by_field_name("body") {
                            return find_decl_in_node(body, source, member_name, hint_row);
                        }
                    }
                }
                "export_statement" | "ambient_declaration" => {
                    // Recurse into wrappers to find internal_module
                    if let Some(found) = find_decl_in_node(child, source, name, hint_row) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        return None;
    }

    let mut candidates: Vec<(usize, tree_sitter::Node<'a>)> = Vec::new(); // (distance, node)

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();

        let candidate = match kind {
            "class_declaration" | "abstract_class_declaration" => {
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
            "lexical_declaration" | "variable_declaration" => {
                // REQ-14-01: Look for variable_declarator whose name matches,
                // then return the value node (arrow_function, function_expression, etc.)
                // Note: tree-sitter uses "function_expression" for `function(){}` values,
                // and "arrow_function" for `() => {}` values.
                let mut ld_cursor = child.walk();
                for ld_child in child.children(&mut ld_cursor) {
                    if ld_child.kind() == "variable_declarator" {
                        let matches_name = ld_child
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok())
                            .map(|n| n == name)
                            .unwrap_or(false);
                        if matches_name {
                            // Return the value node (arrow_function, function_expression, etc.)
                            if let Some(value) = ld_child.child_by_field_name("value") {
                                let value_kind = value.kind();
                                if matches!(
                                    value_kind,
                                    "arrow_function"
                                        | "function_expression"
                                        | "generator_function_expression"
                                ) {
                                    let found_row = value.start_position().row;
                                    let dist = found_row.abs_diff(hint_row);
                                    candidates.push((dist, value));
                                }
                            }
                            // Also recurse for nested declarations
                            if let Some(found) = find_decl_in_node(ld_child, source, name, hint_row) {
                                let found_row = found.start_position().row;
                                let dist = found_row.abs_diff(hint_row);
                                candidates.push((dist, found));
                            }
                        }
                    }
                }
                None
            }
            "function_expression" | "generator_function_expression" => {
                // REQ-14-02: Anonymous function expression — only matches when name == "default"
                // (export default function() {})
                // Note: tree-sitter uses "function_expression" (not "function") for anonymous functions.
                if name == "default" {
                    let found_row = child.start_position().row;
                    let dist = found_row.abs_diff(hint_row);
                    candidates.push((dist, child));
                }
                None
            }
            "class" => {
                // REQ-14-02: Anonymous class — only matches when name == "default"
                // (export default class {})
                if name == "default" {
                    let found_row = child.start_position().row;
                    let dist = found_row.abs_diff(hint_row);
                    candidates.push((dist, child));
                }
                None
            }
            "export_statement" | "ambient_declaration" => {
                // Recurse into wrappers
                find_decl_in_node(child, source, name, hint_row)
            }
            "expression_statement" => {
                // REQ-15-01: CJS assignment patterns — module.exports = class/fn or exports.NAME = fn
                let mut es_cursor = child.walk();
                for es_child in child.children(&mut es_cursor) {
                    if es_child.kind() == "assignment_expression" {
                        let left = es_child.child_by_field_name("left");
                        let right = es_child.child_by_field_name("right");
                        if let (Some(left_node), Some(right_node)) = (left, right) {
                            if left_node.kind() == "member_expression" {
                                let obj = left_node
                                    .child_by_field_name("object")
                                    .and_then(|n| n.utf8_text(source).ok());
                                let prop = left_node
                                    .child_by_field_name("property")
                                    .and_then(|n| n.utf8_text(source).ok());

                                match (obj, prop) {
                                    (Some("module"), Some("exports")) if name == "default" => {
                                        // module.exports = class/function/arrow
                                        let rhs_kind = right_node.kind();
                                        if matches!(
                                            rhs_kind,
                                            "class"
                                                | "function_expression"
                                                | "generator_function"
                                                | "arrow_function"
                                        ) {
                                            let found_row = right_node.start_position().row;
                                            let dist = found_row.abs_diff(hint_row);
                                            candidates.push((dist, right_node));
                                        }
                                    }
                                    (Some("exports"), Some(prop_name)) if prop_name == name => {
                                        // exports.NAME = function/arrow
                                        let rhs_kind = right_node.kind();
                                        if matches!(
                                            rhs_kind,
                                            "function_expression"
                                                | "generator_function"
                                                | "arrow_function"
                                        ) {
                                            let found_row = right_node.start_position().row;
                                            let dist = found_row.abs_diff(hint_row);
                                            candidates.push((dist, right_node));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        };

        if let Some(found) = candidate {
            let found_row = found.start_position().row;
            let dist = found_row.abs_diff(hint_row);
            candidates.push((dist, found));
        }

        // Also recurse into export_statement and ambient_declaration wrappers
        // for deeper nesting
        if matches!(kind, "export_statement" | "ambient_declaration") {
            if let Some(found) = find_decl_in_node(child, source, name, hint_row) {
                let found_row = found.start_position().row;
                let dist = found_row.abs_diff(hint_row);
                candidates.push((dist, found));
            }
        }
    }

    // Return the candidate closest to hint_row
    candidates.sort_by_key(|(dist, _)| *dist);
    candidates.into_iter().next().map(|(_, n)| n)
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

/// Collect members from ALL interface_declaration nodes with matching name.
///
/// Handles declaration merging (REQ-14-04): when two `interface Foo` declarations
/// exist in the same file, their members are unioned. This matches TypeScript
/// compiler behavior for merged interfaces.
fn collect_interface_members(
    source: &[u8],
    lang: &tree_query::SourceLanguage,
    name: &str,
) -> Vec<crate::types::ClassMember> {
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
    let mut all_members: Vec<crate::types::ClassMember> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Walk top-level nodes (including through export_statement and ambient_declaration wrappers)
    fn scan_for_interfaces(
        node: tree_sitter::Node<'_>,
        source: &[u8],
        name: &str,
        all_members: &mut Vec<crate::types::ClassMember>,
        seen_names: &mut std::collections::HashSet<String>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "interface_declaration" => {
                    let matches = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|n| n == name)
                        .unwrap_or(false);
                    if matches {
                        if let Some(body) = tree_query::find_interface_body(child) {
                            let members = tree_query::parse_interface_members(source, body);
                            for member in members {
                                // Dedup by name — first declaration wins for same member
                                if seen_names.insert(member.name.clone()) {
                                    all_members.push(member);
                                }
                            }
                        }
                    }
                }
                "export_statement" | "ambient_declaration" => {
                    scan_for_interfaces(child, source, name, all_members, seen_names);
                }
                _ => {}
            }
        }
    }

    scan_for_interfaces(root, source, name, &mut all_members, &mut seen_names);
    all_members
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

        // REQ-14-05: Interface member structural diff (degraded mode)
        // REQ-14-04: Declaration merging — collect members from ALL interface declarations
        if old_decl.is_interface && new_decl.is_interface {
            let old_members = collect_interface_members(&old_source, &old_lang, name);
            let new_members = collect_interface_members(&new_source, &new_lang, name);
            if !old_members.is_empty() || !new_members.is_empty() {
                findings.extend(interface_diff::compare_interface_members(
                    &old_members,
                    &new_members,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::tree_query::SourceLanguage;

    #[test]
    fn test_ambient_class_declaration_found() {
        // BUG-13-01: export declare class Foo should be found by extract_declaration
        let source = b"export declare class Foo { bar(): void; }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
        assert!(result.is_some(), "extract_declaration should find ambient class Foo");
        let decl = result.unwrap();
        assert!(decl.is_class, "declaration should be identified as a class");
        assert!(decl.class_shape.is_some(), "class shape should be extracted");
    }

    #[test]
    fn test_ambient_function_declaration_found() {
        // BUG-13-01: export declare function foo should be found
        let source = b"export declare function foo(x: number): string;";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "foo", 1);
        assert!(result.is_some(), "extract_declaration should find ambient function foo");
        let decl = result.unwrap();
        assert!(decl.is_function, "declaration should be identified as a function");
    }

    #[test]
    fn test_ambient_enum_declaration_found() {
        // BUG-13-01: export declare enum Status should be found
        let source = b"export declare enum Status { Active, Inactive }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Status", 1);
        assert!(result.is_some(), "extract_declaration should find ambient enum Status");
        let decl = result.unwrap();
        assert!(decl.is_enum, "declaration should be identified as an enum");
        assert!(decl.enum_shape.is_some(), "enum shape should be extracted");
    }

    #[test]
    fn test_ambient_interface_declaration_found() {
        // BUG-13-01: export declare interface Opts should be found
        let source = b"export declare interface Opts { debug: boolean; }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Opts", 1);
        assert!(result.is_some(), "extract_declaration should find ambient interface Opts");
        let decl = result.unwrap();
        assert!(decl.is_interface, "declaration should be identified as an interface");
    }

    #[test]
    fn test_find_decl_tolerates_line_distance() {
        // GAP-13-05: declaration far from hint_line should still be found
        // Simulates a file move where line numbers shift dramatically
        let source = b"\n\n\n\n\n\n\n\n\n\nclass Foo { bar() {} }"; // Foo at line 11
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
        assert!(result.is_some(), "should find Foo even when hint_line is 1 but actual line is 11");
    }

    #[test]
    fn test_find_decl_closest_match_with_multiple_same_name() {
        // GAP-13-05: when multiple declarations have the same name (overloads),
        // the closest to hint_row should be picked
        let source = b"function foo(x: string): void;\nfunction foo(x: number): void;\nfunction foo(x: any): void {}";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "foo", 2);
        assert!(result.is_some(), "should find function foo");
        // We can't easily check which overload was picked, but the function should not panic
    }

    #[test]
    fn test_arrow_function_extract() {
        // REQ-14-01: arrow function exports should be found by extract_declaration
        let source = b"export const add = (x: number, y: number): number => x + y;";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "add", 1);
        assert!(result.is_some(), "extract_declaration should find arrow function 'add'");
        let decl = result.unwrap();
        assert!(decl.is_function, "arrow function should be identified as a function");
        assert!(!decl.is_async, "non-async arrow should not be async");
    }

    #[test]
    fn test_async_arrow_function_extract() {
        // REQ-14-01: async arrow function export
        let source = b"export const fetchData = async (url: string): Promise<string> => { return ''; };";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "fetchData", 1);
        assert!(result.is_some(), "should find async arrow function");
        let decl = result.unwrap();
        assert!(decl.is_function, "async arrow should be a function");
        assert!(decl.is_async, "async arrow should have is_async=true");
    }

    #[test]
    fn test_anonymous_default_function_extract() {
        // REQ-14-02: anonymous default function should be found
        let source = b"export default function(x: number): string { return ''; }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "default", 1);
        assert!(result.is_some(), "should find anonymous default function");
        let decl = result.unwrap();
        assert!(decl.is_function, "anonymous function should be identified as function");
    }

    #[test]
    fn test_anonymous_default_class_extract() {
        // REQ-14-02: anonymous default class should be found
        let source = b"export default class { foo(): void {} }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "default", 1);
        assert!(result.is_some(), "should find anonymous default class");
        let decl = result.unwrap();
        assert!(decl.is_class, "anonymous class should be identified as class");
        assert!(decl.class_shape.is_some(), "class shape should be extracted");
    }

    #[test]
    fn test_function_expression_extract() {
        // REQ-14-01: function expression in variable should be found
        let source = b"export const handler = function(req: Request): Response { return new Response(); };";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "handler", 1);
        assert!(result.is_some(), "should find function expression 'handler'");
        let decl = result.unwrap();
        assert!(decl.is_function, "function expression should be identified as function");
    }

    #[test]
    fn test_arrow_function_overloads_extracted() {
        // REQ-14-01: Arrow functions should have overloads extracted via build_function_shape_from_node
        let source = b"export const greet = (name: string): string => `Hello ${name}`;";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "greet", 1);
        assert!(result.is_some());
        let decl = result.unwrap();
        assert!(!decl.overloads.is_empty(), "arrow function should have overload shape extracted");
    }

    #[test]
    fn test_interface_extract_found() {
        // REQ-14-05: interface should be found and classified
        let source = b"export interface Foo { bar(): void; baz: string; }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
        assert!(result.is_some());
        let decl = result.unwrap();
        assert!(decl.is_interface);
    }

    #[test]
    fn test_collect_interface_members_merged() {
        // REQ-14-04: two interface declarations with same name should union members
        let source = b"export interface Foo { a(): void; }\nexport interface Foo { b(): void; }";
        let members = collect_interface_members(source, &SourceLanguage::TypeScript, "Foo");
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"a"), "member 'a' from first declaration should be present");
        assert!(names.contains(&"b"), "member 'b' from second declaration should be present");
        assert_eq!(members.len(), 2, "should have exactly 2 members from merged declarations");
    }

    #[test]
    fn test_collect_interface_members_dedup() {
        // Same member name in both declarations — first wins
        let source = b"export interface Foo { x(): void; }\nexport interface Foo { x(): void; y: number; }";
        let members = collect_interface_members(source, &SourceLanguage::TypeScript, "Foo");
        let x_count = members.iter().filter(|m| m.name == "x").count();
        assert_eq!(x_count, 1, "duplicate member 'x' should appear only once");
    }

    // ─── REQ-14-03: find_decl_in_node dotted name (namespace member) ─────────

    #[test]
    fn test_find_decl_dotted_name_in_namespace() {
        // REQ-14-03: find_decl_in_node should find member inside namespace body
        let source = b"export namespace Foo { export function bar(x: number): void {} }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo.bar", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find 'Foo.bar' inside namespace"
        );
        let decl = result.unwrap();
        assert!(decl.is_function, "Foo.bar should be identified as a function");
    }

    #[test]
    fn test_find_decl_dotted_name_class_in_namespace() {
        // REQ-14-03: class member inside namespace
        let source = b"export namespace NS { export class MyClass { foo(): void {} } }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "NS.MyClass", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find 'NS.MyClass' inside namespace"
        );
        let decl = result.unwrap();
        assert!(decl.is_class, "NS.MyClass should be identified as a class");
    }

    // ─── REQ-15-01: CJS inline structural diff ──────────────────────────────

    #[test]
    fn test_cjs_module_exports_class_extract() {
        // REQ-15-01: module.exports = class Foo { method() {} } should be found as class
        let source = b"module.exports = class Foo { method() {} }";
        let result = extract_declaration(source, &SourceLanguage::JavaScript, "default", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find CJS inline class as 'default'"
        );
        let decl = result.unwrap();
        assert!(decl.is_class, "CJS inline class should be identified as class");
        let shape = decl.class_shape.as_ref().expect("class_shape should be extracted");
        assert!(
            shape.members.iter().any(|m| m.name == "method"),
            "class shape should contain 'method'; got: {:?}",
            shape.members.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_module_exports_function_extract() {
        // REQ-15-01: module.exports = function(a, b) {} should be found as function
        let source = b"module.exports = function(a, b) { return a + b; }";
        let result = extract_declaration(source, &SourceLanguage::JavaScript, "default", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find CJS inline function as 'default'"
        );
        let decl = result.unwrap();
        assert!(decl.is_function, "CJS inline function should be identified as function");
    }

    #[test]
    fn test_cjs_exports_property_function_extract() {
        // REQ-15-01: exports.handler = async (req, res) => {} should be found
        let source = b"exports.handler = async (req, res) => { return 'ok'; }";
        let result = extract_declaration(source, &SourceLanguage::JavaScript, "handler", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find CJS exports.handler arrow function"
        );
        let decl = result.unwrap();
        assert!(decl.is_function, "CJS exports.handler arrow should be identified as function");
        assert!(decl.is_async, "async arrow should have is_async=true");
    }

    #[test]
    fn test_cjs_inline_class_structural_diff() {
        // REQ-15-01: module.exports = class { method() {} } with method removed
        // should produce class-method-removed finding via compare_structural_diff
        use crate::types::{DeclarationKind, ExportKind, ExportSymbol};
        use std::collections::HashMap;

        let tmp = tempfile::tempdir().expect("tempdir");
        let old_dir = tmp.path().join("old");
        let new_dir = tmp.path().join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        std::fs::write(
            old_dir.join("index.js"),
            b"module.exports = class Foo { method() {} }",
        )
        .unwrap();
        std::fs::write(
            new_dir.join("index.js"),
            b"module.exports = class Foo { }",
        )
        .unwrap();

        let file = std::path::PathBuf::from("index.js");
        let sym = ExportSymbol {
            name: "default".to_string(),
            kind: ExportKind::Default,
            file: file.clone(),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kind: Some(DeclarationKind::Class),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
        };

        let mut old_exports: ExportMap = HashMap::new();
        old_exports.insert("default".to_string(), sym.clone());
        let mut new_exports: ExportMap = HashMap::new();
        new_exports.insert("default".to_string(), sym);

        let findings =
            compare_structural_diff(&old_dir, &new_dir, &old_exports, &new_exports);
        assert!(
            findings.iter().any(|f| f.rule == "class-method-removed"),
            "removing a method from CJS inline class should produce class-method-removed; got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_inline_function_async_diff() {
        // REQ-15-01: module.exports = function(a, b) {} changing to async function
        // should produce async-to-sync / sync-to-async finding
        use crate::types::{DeclarationKind, ExportKind, ExportSymbol};
        use std::collections::HashMap;

        let tmp = tempfile::tempdir().expect("tempdir");
        let old_dir = tmp.path().join("old");
        let new_dir = tmp.path().join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        std::fs::write(
            old_dir.join("index.js"),
            b"module.exports = function(a, b) { return a + b; }",
        )
        .unwrap();
        std::fs::write(
            new_dir.join("index.js"),
            b"module.exports = async function(a, b) { return a + b; }",
        )
        .unwrap();

        let file = std::path::PathBuf::from("index.js");
        let sym = ExportSymbol {
            name: "default".to_string(),
            kind: ExportKind::Default,
            file: file.clone(),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kind: Some(DeclarationKind::Function),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
        };

        let mut old_exports: ExportMap = HashMap::new();
        old_exports.insert("default".to_string(), sym.clone());
        let mut new_exports_map: ExportMap = HashMap::new();
        new_exports_map.insert("default".to_string(), sym);

        let findings =
            compare_structural_diff(&old_dir, &new_dir, &old_exports, &new_exports_map);
        assert!(
            findings.iter().any(|f| f.rule == "sync-to-async"),
            "changing CJS function from sync to async should produce sync-to-async; got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
    }
}
