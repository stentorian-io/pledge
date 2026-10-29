//! Extract public declarations and dispatch their language-specific comparisons.

use crate::core::findings::{Finding, Severity};
use crate::languages::javascript::compare::{behavior as behavior_diff, classes as class_diff};
use crate::languages::javascript::model::{ClassShape, ExportMap};
use crate::languages::javascript::syntax::{
    classes as class_syntax, functions as function_syntax, grammar,
};
use crate::languages::javascript::{compare as javascript_compare, model as js_model};
use crate::languages::typescript::compare::{
    enums as enum_diff, generics as generic_diff, interfaces as interface_diff, refinements,
    rules as type_rules, types as type_diff,
};
use crate::languages::typescript::model::{ConstructSignature, EnumShape, ReleaseTag, TypeParam};
use crate::languages::typescript::syntax::{
    enums as enum_syntax, functions as typed_functions, generics as generic_syntax,
    interfaces as interface_syntax,
};
use crate::languages::typescript::types::{parser as type_parser, shape as type_shapes};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Owned extraction result for a single declaration node.
///
/// Created by `extract_declaration` which parses the file and immediately extracts
/// all needed data into owned types, avoiding lifetime issues with tree-sitter nodes.
#[allow(dead_code)]
pub(crate) struct ExtractedDecl {
    is_class: bool,
    is_enum: bool,
    is_function: bool,
    is_interface: bool,
    /// TYPE-02: true iff the declaration is a `type X = <type>`
    /// alias. Used by `compare_structural_diff` to route type-alias RHS
    /// pairs through `diff_types` (the base `compare_exports_core` path
    /// only fires for functions because resolver-side `overloads` is
    /// empty for type aliases without LSP).
    is_type_alias: bool,
    class_shape: Option<ClassShape>,
    enum_shape: Option<EnumShape>,
    type_params: Vec<TypeParam>,
    // REF rules:
    index_signatures: Vec<interface_syntax::IndexSignature>,
    type_guard_type: Option<String>,
    overloads: Vec<type_shapes::TypeShape>,
    // BEH-01/BEH-02:
    is_async: bool,
    is_generator: bool,
    // TLF rules:
    construct_signatures: Vec<ConstructSignature>,
    this_param_type: Option<String>,
    assertion_target: Option<String>,
}

/// Check if an arrow_function node has the `async` keyword.
///
/// Arrow functions have no `"function"` keyword — instead, `"async"` appears
/// before the `"parameters"` or `"identifier"` children.
pub(crate) fn is_async_arrow(arrow_node: tree_sitter::Node, source: &[u8]) -> bool {
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
/// Parse `source` once with the language-specific tree-sitter grammar.
/// Hot-path parses can be cached by the caller and passed to
/// `extract_declaration_from_tree` to avoid re-parsing the same.d.ts file
/// for every exported symbol it declares (#OOM-prisma).
pub(crate) fn parse_source_for_extract(
    source: &[u8],
    lang: &grammar::SourceLanguage,
) -> Option<tree_sitter::Tree> {
    // INF-01: TypeScriptTsx routes to LANGUAGE_TSX — strict superset
    // of LANGUAGE_TYPESCRIPT so every TS query still parses identically.
    let ts_lang: tree_sitter::Language = match lang {
        grammar::SourceLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        grammar::SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        grammar::SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    };

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang).ok()?;
    parser.parse(source, None)
}

/// Read + parse a source file with caching keyed by absolute path.
///
/// The cache stores `Option<(Vec<u8>, Tree)>` so that "unparseable" outcomes
/// (read failure, oversized, minified, grammar load failure) are also memoised
/// every subsequent symbol referencing the same file returns immediately
/// instead of re-stating the same fs read or re-attempting the same parse.
///
/// This is the hot-path mitigation for #OOM-prisma where a 100 KB+.d.ts can
/// be home to 50+ exported symbols and re-parsing per symbol blows resident
/// memory past 8 GB.
pub(crate) fn get_or_parse<'a>(
    cache: &'a mut HashMap<PathBuf, Option<(Vec<u8>, tree_sitter::Tree)>>,
    path: &Path,
    lang: &grammar::SourceLanguage,
) -> Option<&'a (Vec<u8>, tree_sitter::Tree)> {
    if !cache.contains_key(path) {
        let parsed = std::fs::read(path).ok().and_then(|bytes| {
            if crate::core::source::classify(&bytes).is_some() {
                return None;
            }
            let tree = parse_source_for_extract(&bytes, lang)?;
            Some((bytes, tree))
        });
        cache.insert(path.to_path_buf(), parsed);
    }
    cache.get(path).and_then(|opt| opt.as_ref())
}

/// Test-only convenience wrapper that parses the source and extracts in a
/// single call. Production code uses `parse_source_for_extract` +
/// `extract_declaration_from_tree` separately so the parsed tree can be cached
/// across many symbols sharing the same file (#OOM-prisma).
#[cfg(test)]
pub(crate) fn extract_declaration(
    source: &[u8],
    lang: &grammar::SourceLanguage,
    name: &str,
    hint_line: u32,
) -> Option<ExtractedDecl> {
    let tree = parse_source_for_extract(source, lang)?;
    extract_declaration_from_tree(&tree, source, name, hint_line)
}

/// Extract a declaration from a pre-parsed tree.
///
/// Caller-owned tree avoids re-parsing the same source bytes once per
/// exported symbol when many symbols share a single.d.ts file (the
/// `compare_structural_diff` hot path on packages like `@prisma/client`).
pub(crate) fn extract_declaration_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
    name: &str,
    hint_line: u32,
) -> Option<ExtractedDecl> {
    let root = tree.root_node();

    // Walk top-level children of the program node looking for a declaration
    // whose name field matches `name` and whose start line is within ±3 of hint_line.
    //
    // Supported kinds:
    // class_declaration, abstract_class_declaration,
    // enum_declaration,
    // function_declaration, interface_declaration, type_alias_declaration,
    // lexical_declaration (export const/let — the variable_declarator carries the name)
    let hint = hint_line.saturating_sub(1) as usize; // 0-indexed

    let decl_node = find_decl_in_node(root, source, name, hint)?;
    let kind = decl_node.kind();

    // arrow_function and function_expression in variables
    // function_expression and generator_function_expression as anonymous defaults
    // Note: tree-sitter uses "function_expression" for `function() {}` values (not "function").
    let is_arrow_function = kind == "arrow_function";
    let is_anonymous_function = matches!(
        kind,
        "function_expression" | "generator_function_expression"
    );
    let is_anonymous_class = kind == "class";

    // Treat arrow functions and anonymous functions as function declarations
    let is_function = matches!(
        kind,
        "function_declaration" | "function_signature" | "generator_function_declaration"
    ) || is_arrow_function
        || is_anonymous_function;

    // anonymous class nodes (export default class {}) treated as classes
    let is_class =
        matches!(kind, "class_declaration" | "abstract_class_declaration") || is_anonymous_class;
    let is_enum = kind == "enum_declaration";
    let is_interface = kind == "interface_declaration";
    // TYPE-02: recognize type aliases so compare_structural_diff
    // can route RHS pairs through diff_types.
    let is_type_alias = kind == "type_alias_declaration";

    // parse_class_shape handles both class_declaration and class (anonymous) nodes
    // since both have a class_body child
    let class_shape = if is_class {
        Some(class_syntax::parse_class_shape(source, decl_node))
    } else {
        None
    };

    let enum_shape = if is_enum {
        Some(enum_syntax::parse_enum_shape(source, decl_node))
    } else {
        None
    };

    // extract type params from ALL declaration types.
    let type_params = generic_syntax::parse_type_params(source, decl_node);

    // Extract index signatures for interfaces (REF-07)
    let index_signatures = if is_interface {
        interface_syntax::find_interface_body(decl_node)
            .map(|body| interface_syntax::extract_index_signatures(body, source))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Extract type guard for functions (REF-03)
    // is_function now includes arrow functions, so extract_type_guard works automatically
    let type_guard_type = if is_function {
        typed_functions::extract_type_guard(decl_node, source)
    } else {
        None
    };

    // Build overload shape for function signatures (REF-01, REF-02)
    // build_function_shape_from_node uses child_by_field_name("parameters") which
    // works on arrow_function nodes (field name is also "parameters")
    let overloads = if is_function {
        function_syntax::build_function_shape_from_node(decl_node, source)
            .into_iter()
            .collect()
    } else if kind == "type_alias_declaration" {
        // TYPE-02: route the RHS of `type X = <type>` through
        // `parse_type_string` into `overloads[0]` so the existing
        // `diff_types` pipeline can compare the shape. Without this,
        // non-LSP runs (where `type_signature` stays None) have no path
        // from a type-alias source to `TypeShape`, and the
        // (InferExtends, InferExtends) diff arm never fires.
        // Limited to a single TypeShape (no true overloads for type aliases).
        decl_node
            .child_by_field_name("value")
            .and_then(|value_node| value_node.utf8_text(source).ok())
            .map(type_parser::parse_type_string)
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
        function_syntax::is_async_function(decl_node, source)
    } else {
        false
    };
    // Arrow functions cannot be generators (JS/TS spec)
    let is_generator_fn = if is_arrow_function {
        false
    } else if is_function {
        function_syntax::is_generator_function(decl_node, source)
    } else {
        false
    };

    // TLF-01: extract construct signatures from interface/class body
    let construct_signatures = if is_interface {
        interface_syntax::find_interface_body(decl_node)
            .map(|body| interface_syntax::extract_construct_signatures(body, source))
            .unwrap_or_default()
    } else if is_class {
        decl_node
            .child_by_field_name("body")
            .map(|body| interface_syntax::extract_construct_signatures(body, source))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // TLF-03: extract this parameter type from function declarations
    let this_param_type = if is_function {
        typed_functions::extract_this_param_type(decl_node, source)
    } else {
        None
    };

    // TLF-04: extract assertion target from function declarations
    let assertion_target = if is_function {
        typed_functions::extract_assertion_target(decl_node, source)
    } else {
        None
    };

    Some(ExtractedDecl {
        is_class,
        is_enum,
        is_function,
        is_interface,
        is_type_alias,
        class_shape,
        enum_shape,
        type_params,
        index_signatures,
        type_guard_type,
        overloads,
        is_async: is_async_fn,
        is_generator: is_generator_fn,
        construct_signatures,
        this_param_type,
        assertion_target,
    })
}

/// Recursively search `node` and its children for a declaration whose name
/// matches `name`. Returns the declaration node closest to `hint_row`
/// (0-indexed).
///
/// Uses a closest-match strategy: collects all candidates matching by name,
/// then returns the one nearest to `hint_row`. This handles file moves
/// between versions and ambient declarations in.d.ts files
///.
pub(crate) fn find_decl_in_node<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
    name: &str,
    hint_row: usize,
) -> Option<tree_sitter::Node<'a>> {
    // Handle dotted names (namespace members) by finding the namespace
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
            "class_declaration" | "abstract_class_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "function_declaration" | "generator_function_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "interface_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "type_alias_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "enum_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "function_signature" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "internal_module" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "lexical_declaration" | "variable_declaration" => {
                // Look for variable_declarator whose name matches,
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
                            if let Some(found) = find_decl_in_node(ld_child, source, name, hint_row)
                            {
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
                // Anonymous function expression — only matches when name == "default"
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
                // Anonymous class — only matches when name == "default"
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
                // CJS assignment patterns — module.exports = class/fn or exports.NAME = fn
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
/// Used in the compare_structural_diff hot path; the tree is parsed once
/// per file instead of once per overloaded symbol (#OOM-prisma).
pub(crate) fn collect_overload_shapes_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
    name: &str,
) -> Vec<type_shapes::TypeShape> {
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
                            function_syntax::build_function_shape_from_node(export_child, source)
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
/// Handles declaration merging: when two `interface Foo` declarations
/// exist in the same file, their members are unioned. This matches TypeScript
/// compiler behavior for merged interfaces.
#[cfg(test)]
pub(crate) fn collect_interface_members(
    source: &[u8],
    lang: &grammar::SourceLanguage,
    name: &str,
) -> Vec<js_model::ClassMember> {
    let Some(tree) = parse_source_for_extract(source, lang) else {
        return Vec::new();
    };
    collect_interface_members_from_tree(&tree, source, name)
}

/// Pre-parsed-tree variant of `collect_interface_members`. Used in the
/// compare_structural_diff hot path so the tree is parsed once per file
/// instead of once per merged-interface symbol (#OOM-prisma).
pub(crate) fn collect_interface_members_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
    name: &str,
) -> Vec<js_model::ClassMember> {
    let root = tree.root_node();
    let mut all_members: Vec<js_model::ClassMember> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Walk top-level nodes (including through export_statement and ambient_declaration wrappers)
    fn scan_for_interfaces(
        node: tree_sitter::Node<'_>,
        source: &[u8],
        name: &str,
        all_members: &mut Vec<js_model::ClassMember>,
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
                        if let Some(body) = interface_syntax::find_interface_body(child) {
                            let members = interface_syntax::parse_interface_members(source, body);
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
/// generic params are extracted from ALL declaration types
/// (class, function, interface, type alias), not just classes.
pub fn compare_structural_diff(
    old_root: &Path,
    new_root: &Path,
    old_exports: &ExportMap,
    new_exports: &ExportMap,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    // SEM-08: Build exported-names set ONCE per comparison for rollup suffix normalization.
    // The union of old and new exported names ensures any symbol visible in either version
    // can be used as a normalization anchor. This set is passed to normalize_rollup_suffix
    // to gate $N stripping on names that are actually exported.
    //
    // keys are ExportKey composites; we project to `name` for the
    // rollup-suffix matching code which operates on bare identifier strings.
    let exported_names: HashSet<String> = old_exports
        .keys()
        .chain(new_exports.keys())
        .map(|k| k.name.clone())
        .collect();

    // Cached parses (#OOM-prisma): on real-world packages like @prisma/client,
    // a single 100 KB+.d.ts is the home of 50+ exported symbols. The previous
    // implementation re-read and re-parsed the same file once per symbol,
    // accumulating ~80 MB of transient parse-tree allocations per call and
    // pushing resident memory past 8 GB before the OS killed the process.
    // Cache parsed Trees by file path so each file is parsed exactly once
    // per compare call. The Option layer caches "this path is unparseable"
    // (unreadable, oversized, minified, or grammar load failed) so subsequent
    // symbols sharing the same file don't redo the failed work.
    let mut old_parse_cache: HashMap<PathBuf, Option<(Vec<u8>, tree_sitter::Tree)>> =
        HashMap::new();
    let mut new_parse_cache: HashMap<PathBuf, Option<(Vec<u8>, tree_sitter::Tree)>> =
        HashMap::new();

    for (key, old_sym) in old_exports {
        let name = key.name.as_str();
        let Some(new_sym) = new_exports.get(key) else {
            continue;
        };
        if [old_sym, new_sym].iter().any(|symbol| {
            matches!(
                symbol.release_tag,
                Some(ReleaseTag::Internal | ReleaseTag::Alpha | ReleaseTag::Beta)
            )
        }) {
            continue;
        }

        // Build absolute paths to the source files
        let old_path = old_root.join(&old_sym.file);
        let new_path = new_root.join(&new_sym.file);

        // Determine language from file extension
        let Some(old_lang) = grammar::language_for_file(&old_sym.file) else {
            continue;
        };
        let Some(new_lang) = grammar::language_for_file(&new_sym.file) else {
            continue;
        };

        let Some((old_source, old_tree)) = get_or_parse(&mut old_parse_cache, &old_path, &old_lang)
        else {
            continue;
        };
        let Some((new_source, new_tree)) = get_or_parse(&mut new_parse_cache, &new_path, &new_lang)
        else {
            continue;
        };

        // Extract all shapes from both versions
        let Some(old_decl) = extract_declaration_from_tree(
            old_tree,
            old_source,
            old_sym.local_name.as_deref().unwrap_or(name),
            old_sym.line,
        ) else {
            continue;
        };
        let Some(new_decl) = extract_declaration_from_tree(
            new_tree,
            new_source,
            new_sym.local_name.as_deref().unwrap_or(name),
            new_sym.line,
        ) else {
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

        // Interface member structural diff (degraded mode)
        // Declaration merging — collect members from ALL interface declarations
        if old_decl.is_interface && new_decl.is_interface {
            let old_members = collect_interface_members_from_tree(
                old_tree,
                old_source,
                old_sym.local_name.as_deref().unwrap_or(name),
            );
            let new_members = collect_interface_members_from_tree(
                new_tree,
                new_source,
                new_sym.local_name.as_deref().unwrap_or(name),
            );
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

            // Interface index signature diff (interface-level)
            findings.extend(interface_diff::compare_interface_index_signatures(
                &old_decl.index_signatures,
                &new_decl.index_signatures,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // ENUM-01..05: enum diff
        if old_decl.is_enum && new_decl.is_enum {
            if let (Some(old_shape), Some(new_shape)) = (&old_decl.enum_shape, &new_decl.enum_shape)
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

        // GEN-01..06: generic type parameter comparison applies to all constructs.
        let has_resolved_parameters = [old_sym, new_sym].iter().any(|symbol| {
            symbol
                .overloads
                .iter()
                .any(|shape| matches!(shape, type_shapes::TypeShape::GenericDeclaration { .. }))
        });
        if (!old_decl.type_params.is_empty() || !new_decl.type_params.is_empty())
            && !has_resolved_parameters
        {
            findings.extend(generic_diff::compare_type_params(
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
            let collected = collect_overload_shapes_from_tree(old_tree, old_source, name);
            if old_sym_augmented.overloads.is_empty() && !collected.is_empty() {
                old_sym_augmented.overloads = collected;
            } else if old_sym_augmented.overloads.is_empty() && !old_decl.overloads.is_empty() {
                old_sym_augmented.overloads = old_decl.overloads.clone();
            }
        }
        if new_decl.is_function {
            let collected = collect_overload_shapes_from_tree(new_tree, new_source, name);
            if new_sym_augmented.overloads.is_empty() && !collected.is_empty() {
                new_sym_augmented.overloads = collected;
            } else if new_sym_augmented.overloads.is_empty() && !new_decl.overloads.is_empty() {
                new_sym_augmented.overloads = new_decl.overloads.clone();
            }
        }

        // SEM-08: Normalize rollup-plugin-dts $N suffixes on type_signature fields
        // before they flow into ref_diff and type_diff comparisons. We use local
        // normalized copies to avoid mutating the ExportMap.
        if let Some(sig) = &old_sym_augmented.type_signature {
            old_sym_augmented.type_signature = Some(javascript_compare::normalize_rollup_suffix(
                sig,
                &exported_names,
            ));
        }
        if let Some(sig) = &new_sym_augmented.type_signature {
            new_sym_augmented.type_signature = Some(javascript_compare::normalize_rollup_suffix(
                sig,
                &exported_names,
            ));
        }

        findings.extend(refinements::compare_ref_changes(
            &old_sym_augmented,
            &new_sym_augmented,
            name,
            &new_sym.file,
            new_sym.line,
            new_sym.col,
            &old_decl.index_signatures,
            &new_decl.index_signatures,
        ));

        // GEN-07: NoInfer<T> wrapper removed from a function-
        // parameter position. Walks paired Function overloads from the
        // augmented symbols (which carry the tree-sitter-extracted shapes
        // even in non-LSP runs). Falls through Opaque param-text via
        // parse_type_string so the GEN-07 wrapper canonicalization (in
        // parse_generic) surfaces here as TypeShape::NoInferWrap. Out of
        // scope: return-type position.
        if old_decl.is_function && new_decl.is_function {
            findings.extend(type_diff::diff_function_param_noinfer_removed(
                &old_sym_augmented.overloads,
                &new_sym_augmented.overloads,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

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

        // TLF-01: construct signature removed (interfaces and classes)
        let is_class_or_interface = (old_decl.is_interface && new_decl.is_interface)
            || (old_decl.is_class && new_decl.is_class);
        if is_class_or_interface
            && (!old_decl.construct_signatures.is_empty()
                || !new_decl.construct_signatures.is_empty())
        {
            findings.extend(interface_diff::compare_construct_signatures(
                &old_decl.construct_signatures,
                &new_decl.construct_signatures,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // TLF-03: this parameter type changed (functions)
        let resolved_receiver = [old_sym, new_sym].iter().any(|symbol| {
            symbol.overloads.iter().any(|shape| {
                shape
                    .function_contract()
                    .is_some_and(|contract| contract.receiver.is_some())
            })
        });
        if old_decl.is_function && new_decl.is_function && !resolved_receiver {
            findings.extend(refinements::compare_this_param(
                old_decl.this_param_type.as_deref(),
                new_decl.this_param_type.as_deref(),
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // TLF-04: assertion target widened (functions)
        if old_decl.is_function && new_decl.is_function {
            findings.extend(refinements::compare_assertion(
                old_decl.assertion_target.as_deref(),
                new_decl.assertion_target.as_deref(),
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // Source-only fallback; resolved shapes were already compared by the
        // primary type pass, so do not emit that same finding twice.
        if old_decl.is_type_alias
            && new_decl.is_type_alias
            && (old_sym.overloads.is_empty() || new_sym.overloads.is_empty())
        {
            if let (Some(old_shape), Some(new_shape)) =
                (old_decl.overloads.first(), new_decl.overloads.first())
            {
                findings.extend(type_diff::diff_types(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                ));
            }
        }

        // TLF-07: type-only re-export flag changed
        // Going from a value re-export to type-only re-export (or vice versa) changes
        // the consumer's ability to use the import as a value.
        if old_sym.is_reexport
            && new_sym.is_reexport
            && old_sym.is_type_only != new_sym.is_type_only
        {
            let change = if new_sym.is_type_only {
                "became type-only"
            } else {
                "became value re-export"
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: type_rules::RULE_TYPE_ONLY_REEXPORT_CHANGED,
                file: new_sym.file.clone(),
                line: new_sym.line,
                col: new_sym.col,
                message: format!(
                    "re-export of `{}` {} — consumers relying on the {} may break",
                    name,
                    change,
                    if new_sym.is_type_only {
                        "value"
                    } else {
                        "type-only constraint"
                    },
                ),
            });
        }
    }

    findings
}

#[cfg(test)]
mod tests;
