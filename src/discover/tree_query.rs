#![allow(dead_code)]

use std::path::Path;

use anyhow::Context;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::types::DeclarationKind;

// ---------------------------------------------------------------------------
// Language selection
// ---------------------------------------------------------------------------

/// Which grammar should be used to parse a source file.
#[allow(dead_code)]
pub enum SourceLanguage {
    TypeScript,
    JavaScript,
}

/// Determine which grammar to use based on the file extension.
///
/// Returns `None` for unsupported extensions so callers can emit a warning and
/// skip the file rather than hard-failing.
#[allow(dead_code)]
pub fn language_for_file(path: &Path) -> Option<SourceLanguage> {
    let ext = path.extension()?.to_str()?;
    // Check for compound extensions like `.d.ts`, `.d.mts`, `.d.cts`.
    let file_name = path.file_name()?.to_str().unwrap_or("");
    if file_name.ends_with(".d.ts")
        || file_name.ends_with(".d.mts")
        || file_name.ends_with(".d.cts")
    {
        return Some(SourceLanguage::TypeScript);
    }
    match ext {
        "ts" | "tsx" | "mts" | "cts" => Some(SourceLanguage::TypeScript),
        "js" | "mjs" | "cjs" | "jsx" => Some(SourceLanguage::JavaScript),
        _ => None,
    }
}

/// Return the tree-sitter `Language` handle for the given `SourceLanguage`.
///
/// Uses the non-deprecated `LANGUAGE_TYPESCRIPT` / `LANGUAGE` constants.
fn get_language(lang: &SourceLanguage) -> tree_sitter::Language {
    match lang {
        SourceLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    }
}

// ---------------------------------------------------------------------------
// Raw export representation
// ---------------------------------------------------------------------------

/// The kind of export as detected by tree-sitter — before semantic resolution.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawExportKind {
    /// `export { foo }` or `export { foo as bar }` (no `from` clause)
    Named,
    /// `export default …`
    Default,
    /// `export * from './module'`
    StarReexport,
    /// `export { foo } from './module'`
    NamedReexport,
}

/// A single export statement captured by tree-sitter before semantic enrichment.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RawExport {
    /// The exported name. For `Default` this is `"default"`. For `StarReexport`
    /// this is `"*"`.
    pub name: String,
    pub kind: RawExportKind,
    /// 1-indexed line number (per Pitfall 7: tree-sitter is 0-indexed).
    pub line: u32,
    /// 1-indexed column number.
    pub col: u32,
    /// The module specifier string for re-exports (e.g., `"./utils"`), with
    /// surrounding quotes stripped. `None` for non-re-export forms.
    pub source: Option<String>,
    /// The syntactic form of the declaration (function, class, interface, etc.).
    /// `None` for re-exports, default expression-form exports, and plain `export { foo }`.
    pub decl_kind: Option<DeclarationKind>,
}

// ---------------------------------------------------------------------------
// Tree-sitter query strings
// ---------------------------------------------------------------------------

/// Captures `export { foo }` and `export { foo as bar }` (no `from` clause).
///
/// We match `export_statement` nodes that contain an `export_clause` but no
/// `source` child — that distinguishes plain named exports from named re-exports.
const NAMED_EXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @name)))
"#;

/// Captures `export default <expr>` — the `value:` field covers expressions and
/// object/array literals. Named default exports (functions/classes with a name
/// that are declared as `declaration:`) are handled by `DEFAULT_DECL_EXPORT_QUERY`.
const DEFAULT_EXPORT_QUERY: &str = r#"
(export_statement
  "default"
  value: (_) @value)
"#;

/// Captures `export default function foo` and `export default class Foo` —
/// these appear as `declaration:` children, not `value:`, in both grammars.
/// We capture the keyword "default" position as the anchor since anonymous
/// default declarations don't have a name node.
const DEFAULT_DECL_EXPORT_QUERY: &str = r#"
(export_statement
  "default"
  declaration: (_) @decl)
"#;

/// Captures `export const/let/var foo`, `export function foo`, `export class Foo`
/// for **JavaScript** files. Class names use `identifier` in the JS grammar.
const DECL_EXPORT_QUERY_JS: &str = r#"
(export_statement
  declaration: [
    (lexical_declaration
      (variable_declarator name: (identifier) @name))
    (function_declaration name: (identifier) @name)
    (class_declaration name: (identifier) @name)
  ])
"#;

/// Captures `export const/let/var foo`, `export function foo`, `export class Foo`,
/// `export abstract class Foo`, `export type Foo`, `export interface Foo`, `export enum Foo`,
/// `export namespace Foo`, and overload `export function foo(...)` signatures
/// for **TypeScript** files. TypeScript uses `type_identifier` for class/type/interface names.
const DECL_EXPORT_QUERY_TS: &str = r#"
(export_statement
  declaration: [
    (lexical_declaration
      (variable_declarator name: (identifier) @name))
    (function_declaration name: (identifier) @name)
    (function_signature name: (identifier) @name)
    (class_declaration name: (type_identifier) @name)
    (abstract_class_declaration name: (type_identifier) @name)
    (type_alias_declaration name: (type_identifier) @name)
    (interface_declaration name: (type_identifier) @name)
    (enum_declaration name: (identifier) @name)
    (internal_module name: (identifier) @name)
  ])
"#;

/// Captures `export * from './module'` (plain star re-export).
///
/// There are two tree-sitter forms for star re-exports:
/// - `export * from './module'` → `(export_statement source: (string))`
///   with a bare `*` token — no `namespace_export` child node.
/// - `export * as name from './module'` → `(export_statement (namespace_export) source: (string))`
///
/// We use two separate queries to cover both forms.
const STAR_EXPORT_QUERY: &str = r#"
(export_statement
  "*"
  source: (string) @source)
"#;

/// Captures `export * as name from './module'` (namespace re-export).
const NAMESPACE_EXPORT_QUERY: &str = r#"
(export_statement
  (namespace_export)
  source: (string) @source)
"#;

/// Captures `export { foo } from './module'` and `export { foo as bar } from './module'`.
const NAMED_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @name))
  source: (string) @source)
"#;

// ---------------------------------------------------------------------------
// Declaration kind mapping
// ---------------------------------------------------------------------------

/// Map a tree-sitter node kind string to a DeclarationKind.
/// Per D-04 and 08-RESEARCH Pattern 1.
fn decl_kind_from_node_kind(kind: &str) -> DeclarationKind {
    match kind {
        "function_declaration" | "function_signature" => DeclarationKind::Function,
        "class_declaration" | "abstract_class_declaration" => DeclarationKind::Class,
        "interface_declaration" => DeclarationKind::Interface,
        "type_alias_declaration" => DeclarationKind::TypeAlias,
        "enum_declaration" => DeclarationKind::Enum,
        "internal_module" => DeclarationKind::Namespace,
        "lexical_declaration" | "variable_declaration" => DeclarationKind::Variable,
        _ => DeclarationKind::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Public parsing API
// ---------------------------------------------------------------------------

/// Parse `source` bytes with the given grammar and return all detected exports.
///
/// Line and column numbers in the returned `RawExport`s are **1-indexed**
/// (tree-sitter provides 0-indexed positions; this function adds 1 to both).
#[allow(dead_code)]
pub fn parse_exports(source: &[u8], lang: SourceLanguage) -> anyhow::Result<Vec<RawExport>> {
    let language = get_language(&lang);

    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .context("failed to set tree-sitter language")?;

    let tree = parser
        .parse(source, None)
        .context("tree-sitter returned None — source bytes may be invalid UTF-8 or too large")?;

    let root = tree.root_node();
    let mut exports: Vec<RawExport> = Vec::new();

    // --- Named exports (no `from` clause) ------------------------------------
    // We run the named re-export query first to collect (name, source) pairs,
    // then use that set to exclude them from the plain named export results.
    let reexport_positions = collect_reexport_positions(source, &language)?;

    {
        let query = Query::new(&language, NAMED_EXPORT_QUERY)
            .context("failed to compile NAMED_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @name
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;

                    // Skip if this position belongs to a named re-export.
                    if reexport_positions.contains(&(line, col)) {
                        continue;
                    }

                    let name = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on export name")?
                        .to_string();
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::Named,
                        line,
                        col,
                        source: None,
                        decl_kind: None,
                    });
                }
            }
        }
    }

    // --- Default exports (expression form) ------------------------------------
    // Captures `export default <expr>` where the default value is an expression
    // (e.g. `export default 42`, `export default obj`, `export default function() {}`).
    // Named default declarations (`export default class Foo {}`) are handled below.
    {
        let query = Query::new(&language, DEFAULT_EXPORT_QUERY)
            .context("failed to compile DEFAULT_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // @value gives us the position of the default export expression.
            if let Some(capture) = m.captures.first() {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                exports.push(RawExport {
                    name: "default".to_string(),
                    kind: RawExportKind::Default,
                    line,
                    col,
                    source: None,
                    decl_kind: None,
                });
            }
        }
    }

    // --- Default exports (declaration form) -----------------------------------
    // Captures `export default function foo() {}` and `export default class Foo {}`
    // which appear as `declaration:` children rather than `value:` children.
    // We only emit a "default" entry if one hasn't already been added above.
    {
        let query = Query::new(&language, DEFAULT_DECL_EXPORT_QUERY)
            .context("failed to compile DEFAULT_DECL_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            if let Some(capture) = m.captures.first() {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                // Only add if no default has been emitted for this position yet.
                let already_emitted = exports
                    .iter()
                    .any(|e| e.kind == RawExportKind::Default && e.line == line && e.col == col);
                if !already_emitted {
                    exports.push(RawExport {
                        name: "default".to_string(),
                        kind: RawExportKind::Default,
                        line,
                        col,
                        source: None,
                        decl_kind: None,
                    });
                }
            }
        }
    }

    // --- Declaration exports --------------------------------------------------
    // Use grammar-specific queries: TypeScript uses `type_identifier` for class/
    // interface/type names; JavaScript uses `identifier` for all names.
    //
    // We skip any declaration that is under `export default` (which has a
    // "default" keyword child in the parent export_statement node) — those are
    // handled by the default export queries above.
    {
        let decl_query_str = match lang {
            SourceLanguage::TypeScript => DECL_EXPORT_QUERY_TS,
            SourceLanguage::JavaScript => DECL_EXPORT_QUERY_JS,
        };
        let query =
            Query::new(&language, decl_query_str).context("failed to compile DECL_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @name
                    let node = capture.node;

                    // Skip if this declaration is under `export default …`.
                    // The export_statement parent will have a child with kind "default".
                    let is_default_export = node
                        .parent() // declaration node
                        .and_then(|decl| decl.parent()) // export_statement
                        .map(|stmt| {
                            (0..stmt.child_count() as u32).any(|i| {
                                stmt.child(i)
                                    .map(|c| c.kind() == "default")
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false);

                    if is_default_export {
                        continue;
                    }

                    let name = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on declaration name")?
                        .to_string();
                    // Get the declaration node kind from the parent of the captured @name node.
                    // The @name node's parent is the declaration node (function_declaration, etc.).
                    let decl_kind = node
                        .parent()
                        .map(|parent| decl_kind_from_node_kind(parent.kind()));
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::Named,
                        line: node.start_position().row as u32 + 1,
                        col: node.start_position().column as u32 + 1,
                        source: None,
                        decl_kind,
                    });
                }
            }
        }
    }

    // --- Star re-exports (plain `export * from '...'`) ------------------------
    // The JS/TS grammar represents `export * from '...'` with a bare `*` token
    // directly inside the export_statement (no namespace_export child node).
    for star_query_str in &[STAR_EXPORT_QUERY, NAMESPACE_EXPORT_QUERY] {
        // NAMESPACE_EXPORT_QUERY may not compile for some grammar versions if
        // the namespace_export node is absent — skip gracefully.
        let query = match Query::new(&language, star_query_str) {
            Ok(q) => q,
            Err(_) => continue,
        };
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @source
                    let node = capture.node;
                    let raw_source = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on star re-export source")?;
                    let module_spec = strip_quotes(raw_source);
                    let spec_str = module_spec.to_string();
                    // Deduplicate: avoid adding the same source twice if both
                    // queries match (shouldn't happen with correct grammar but
                    // prevents accidental double recursion).
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::StarReexport
                            && e.source.as_deref() == Some(&spec_str)
                    });
                    if !already {
                        exports.push(RawExport {
                            name: "*".to_string(),
                            kind: RawExportKind::StarReexport,
                            line: node.start_position().row as u32 + 1,
                            col: node.start_position().column as u32 + 1,
                            source: Some(spec_str),
                            decl_kind: None,
                        });
                    }
                }
            }
        }
    }

    // --- Named re-exports -----------------------------------------------------
    {
        let query = Query::new(&language, NAMED_REEXPORT_QUERY)
            .context("failed to compile NAMED_REEXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // Match has two captures: @name (index 0) and @source (index 1).
            let mut name_opt: Option<(String, u32, u32)> = None;
            let mut source_opt: Option<String> = None;

            for capture in m.captures {
                let node = capture.node;
                let text = node
                    .utf8_text(source)
                    .context("UTF-8 decode failed on named re-export")?;
                if capture.index == 0 {
                    name_opt = Some((
                        text.to_string(),
                        node.start_position().row as u32 + 1,
                        node.start_position().column as u32 + 1,
                    ));
                } else if capture.index == 1 {
                    source_opt = Some(strip_quotes(text).to_string());
                }
            }

            if let (Some((name, line, col)), Some(src)) = (name_opt, source_opt) {
                exports.push(RawExport {
                    name,
                    kind: RawExportKind::NamedReexport,
                    line,
                    col,
                    source: Some(src),
                    decl_kind: None,
                });
            }
        }
    }

    Ok(exports)
}

// ---------------------------------------------------------------------------
// Class / Enum / Generic extraction
// ---------------------------------------------------------------------------

use crate::types::{
    ClassMember, ClassMemberKind, ClassShape, EnumMember, EnumShape, EnumValueKind, TypeParam,
    Visibility,
};

/// Extract the `Visibility` for a method or field node by iterating its children
/// looking for an `accessibility_modifier` child.
///
/// Per Pitfall 3 in RESEARCH.md: no modifier = `Visibility::Public`.
fn extract_visibility(node: tree_sitter::Node, source: &[u8]) -> Visibility {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == "accessibility_modifier" {
                if let Ok(text) = child.utf8_text(source) {
                    return match text {
                        "protected" => Visibility::Protected,
                        "private" => Visibility::Private,
                        _ => Visibility::Public, // "public" explicit
                    };
                }
            }
        }
    }
    Visibility::Public // No modifier = public (TypeScript default)
}

/// Count required (non-optional) parameters on a method or constructor node.
///
/// Finds the `formal_parameters` child and counts children with kind
/// `required_parameter` (not `optional_parameter`).
fn count_required_params(method_node: tree_sitter::Node) -> usize {
    for i in 0..method_node.child_count() as u32 {
        if let Some(params_node) = method_node.child(i) {
            if params_node.kind() == "formal_parameters" {
                let mut count = 0;
                let mut cursor = params_node.walk();
                for child in params_node.children(&mut cursor) {
                    if child.kind() == "required_parameter" {
                        count += 1;
                    }
                }
                return count;
            }
        }
    }
    0
}

/// Extract all class members (methods, constructors, properties, abstract methods)
/// from a `class_declaration` or `abstract_class_declaration` node.
///
/// Per Pitfall 7: line/col are 1-indexed (tree-sitter provides 0-indexed).
pub fn parse_class_members(source: &[u8], class_node: tree_sitter::Node) -> Vec<ClassMember> {
    let mut members = Vec::new();

    // Find class_body child
    let class_body = {
        let mut body = None;
        for i in 0..class_node.child_count() as u32 {
            if let Some(child) = class_node.child(i) {
                if child.kind() == "class_body" {
                    body = Some(child);
                    break;
                }
            }
        }
        match body {
            Some(b) => b,
            None => return members,
        }
    };

    let mut cursor = class_body.walk();
    for child in class_body.children(&mut cursor) {
        match child.kind() {
            "method_definition" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();

                let kind = if name == "constructor" {
                    ClassMemberKind::Constructor
                } else {
                    ClassMemberKind::Method
                };

                let visibility = extract_visibility(child, source);

                // Check for static, readonly, abstract keyword children
                let mut is_static = false;
                let mut is_abstract = false;
                let mut is_readonly = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        match kw.kind() {
                            "static" => is_static = true,
                            "abstract" => is_abstract = true,
                            "readonly" => is_readonly = true,
                            _ => {}
                        }
                    }
                }

                let param_count = count_required_params(child);

                members.push(ClassMember {
                    name,
                    kind,
                    visibility,
                    is_static,
                    is_abstract,
                    is_readonly,
                    param_count,
                    type_signature: None,
                });
            }
            "public_field_definition" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();

                let visibility = extract_visibility(child, source);

                let mut is_static = false;
                let mut is_readonly = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        match kw.kind() {
                            "static" => is_static = true,
                            "readonly" => is_readonly = true,
                            _ => {}
                        }
                    }
                }

                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Property,
                    visibility,
                    is_static,
                    is_abstract: false,
                    is_readonly,
                    param_count: 0,
                    type_signature: None,
                });
            }
            "abstract_method_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();

                let visibility = extract_visibility(child, source);
                let param_count = count_required_params(child);

                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Method,
                    visibility,
                    is_static: false,
                    is_abstract: true,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                });
            }
            _ => {}
        }
    }

    members
}

/// Extract a full `ClassShape` from a `class_declaration` or
/// `abstract_class_declaration` node.
pub fn parse_class_shape(source: &[u8], decl_node: tree_sitter::Node) -> ClassShape {
    let is_abstract = decl_node.kind() == "abstract_class_declaration";

    // Find class_heritage child (contains extends_clause and implements_clause)
    let mut extends_text: Option<String> = None;
    let mut implements_list: Vec<String> = Vec::new();

    for i in 0..decl_node.child_count() as u32 {
        if let Some(child) = decl_node.child(i) {
            if child.kind() == "class_heritage" {
                // Look for extends_clause and implements_clause
                for j in 0..child.child_count() as u32 {
                    if let Some(heritage_child) = child.child(j) {
                        match heritage_child.kind() {
                            "extends_clause" => {
                                if let Ok(text) = heritage_child.utf8_text(source) {
                                    // Strip the leading "extends " keyword
                                    let text = text.trim();
                                    let text = text.strip_prefix("extends ").unwrap_or(text);
                                    extends_text = Some(text.to_string());
                                }
                            }
                            "implements_clause" => {
                                // Iterate named children (type references)
                                let mut cursor = heritage_child.walk();
                                for impl_child in heritage_child.children(&mut cursor) {
                                    // Skip the "implements" keyword itself and commas
                                    if impl_child.is_named() {
                                        if let Ok(text) = impl_child.utf8_text(source) {
                                            implements_list.push(text.to_string());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                break;
            }
        }
    }

    let type_params = parse_type_params(source, decl_node);
    let members = parse_class_members(source, decl_node);

    ClassShape {
        members,
        is_abstract,
        extends_text,
        implements_list,
        type_params,
    }
}

/// Returns true if the given `enum_declaration` node has a `const` anonymous
/// keyword child (i.e., it is a `const enum`).
///
/// Per Pitfall 1 in RESEARCH.md: `const` is an anonymous grammar token,
/// not a named field. Must iterate children and check text.
pub fn is_const_enum(node: tree_sitter::Node, source: &[u8]) -> bool {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == "enum" {
                break; // Past modifiers
            }
            if !child.is_named() {
                if let Ok(text) = child.utf8_text(source) {
                    if text == "const" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if a function_declaration node has the `async` modifier.
///
/// The `async` keyword is an anonymous token that appears BEFORE the `function`
/// keyword in the AST child order. We stop iterating at the `function` keyword
/// to avoid false positives. Per Phase 9 RESEARCH Pitfall 2.
pub fn is_async_function(fn_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "function" {
            break;
        }
        if !child.is_named() && child.utf8_text(source).ok() == Some("async") {
            return true;
        }
    }
    false
}

/// Check if a function_declaration or generator_function_declaration node has
/// the `*` generator marker.
///
/// The `*` token appears between the `function` keyword and the
/// `formal_parameters` node. We stop at `formal_parameters` to avoid
/// matching `*` in other contexts. Per Phase 9 RESEARCH Pitfall 3.
///
/// Note: tree-sitter-typescript parses `function* foo() {}` as
/// `generator_function_declaration`, not `function_declaration`.
pub fn is_generator_function(fn_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "formal_parameters" {
            break;
        }
        if !child.is_named() && child.utf8_text(source).ok() == Some("*") {
            return true;
        }
    }
    false
}

/// Compute auto-incremented numeric values for enum members.
///
/// Per D-04 and RESEARCH.md: maintains running counter starting at 0.
/// - No explicit value: assign computed_value = Some(counter), increment.
/// - Numeric explicit value: assign computed_value = Some(n), set counter = n + 1.
/// - Non-numeric initializer: set computed_value = None, still increment counter.
pub fn compute_enum_values(members: &mut [EnumMember]) {
    let mut next_value: i64 = 0;
    for member in members.iter_mut() {
        match &member.raw_value {
            None => {
                member.computed_value = Some(next_value);
                next_value += 1;
            }
            Some(text) => {
                if let Ok(n) = text.trim().parse::<i64>() {
                    member.computed_value = Some(n);
                    next_value = n + 1;
                } else {
                    member.computed_value = None;
                    next_value += 1; // TypeScript behavior: still increment
                }
            }
        }
    }
}

/// Extract all members from an `enum_declaration` node.
///
/// Handles both explicit (`enum_assignment`) and implicit (bare identifier) members.
/// Calls `compute_enum_values` to fill in computed_value fields.
pub fn parse_enum_members(source: &[u8], enum_node: tree_sitter::Node) -> Vec<EnumMember> {
    let mut members = Vec::new();

    // Find enum_body child
    let enum_body = {
        let mut body = None;
        for i in 0..enum_node.child_count() as u32 {
            if let Some(child) = enum_node.child(i) {
                if child.kind() == "enum_body" {
                    body = Some(child);
                    break;
                }
            }
        }
        match body {
            Some(b) => b,
            None => return members,
        }
    };

    let mut cursor = enum_body.walk();
    for child in enum_body.children(&mut cursor) {
        match child.kind() {
            "enum_assignment" => {
                // Has explicit value: name: (_) @name value: (_) @value
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                let value_text = child
                    .child_by_field_name("value")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.to_string());

                let value_kind = if let Some(ref text) = value_text {
                    let trimmed = text.trim();
                    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
                        EnumValueKind::StringLiteral
                    } else if trimmed.parse::<i64>().is_ok() {
                        EnumValueKind::Numeric
                    } else {
                        EnumValueKind::OtherExpr
                    }
                } else {
                    EnumValueKind::Numeric // implicit
                };

                members.push(EnumMember {
                    name,
                    raw_value: value_text,
                    computed_value: None, // filled in by compute_enum_values
                    value_kind,
                });
            }
            "property_identifier" | "identifier" => {
                // Bare member with no explicit value (implicit numeric)
                let name = child
                    .utf8_text(source)
                    .unwrap_or("")
                    .to_string();
                members.push(EnumMember {
                    name,
                    raw_value: None,
                    computed_value: None,
                    value_kind: EnumValueKind::Numeric, // implicit numeric
                });
            }
            _ => {}
        }
    }

    compute_enum_values(&mut members);
    members
}

/// Extract a full `EnumShape` from an `enum_declaration` node.
pub fn parse_enum_shape(source: &[u8], enum_node: tree_sitter::Node) -> EnumShape {
    let is_const = is_const_enum(enum_node, source);
    let members = parse_enum_members(source, enum_node);
    EnumShape { members, is_const }
}

/// Extract generic type parameters from ANY declaration node that has a
/// `type_parameters` field (class, function, interface, type_alias).
///
/// Per D-06: applies to ALL exported constructs.
/// Returns empty Vec if no `type_parameters` field.
pub fn parse_type_params(source: &[u8], decl_node: tree_sitter::Node) -> Vec<TypeParam> {
    let mut params = Vec::new();

    let tp_node = match decl_node.child_by_field_name("type_parameters") {
        Some(n) => n,
        None => return params,
    };

    let mut cursor = tp_node.walk();
    for child in tp_node.children(&mut cursor) {
        if child.kind() == "type_parameter" {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("")
                .to_string();

            let constraint_text = child
                .child_by_field_name("constraint")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| {
                    // Strip the leading "extends " keyword if present
                    let s = s.trim();
                    s.strip_prefix("extends ").unwrap_or(s).to_string()
                });

            let default_text = child
                .child_by_field_name("value")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| {
                    let s = s.trim();
                    // Strip leading "= " prefix if present (tree-sitter may include it)
                    s.strip_prefix("= ").unwrap_or(s).trim().to_string()
                });

            params.push(TypeParam {
                name,
                constraint_text,
                default_text,
                constraint_shape: None, // LSP fills in later for GEN-04
            });
        }
    }

    params
}

// ---------------------------------------------------------------------------
// Phase 8 extraction functions (REF-02..REF-07)
// ---------------------------------------------------------------------------

/// Check if a required_parameter node represents a rest parameter.
/// Per 08-RESEARCH: rest params are `required_parameter` with a `rest_pattern` child.
/// NOT `rest_parameter` — that node kind does not exist in tree-sitter-typescript 0.23.2.
pub fn is_rest_param(param_node: tree_sitter::Node) -> bool {
    let mut cursor = param_node.walk();
    for child in param_node.children(&mut cursor) {
        if child.kind() == "rest_pattern" {
            return true;
        }
    }
    false
}

/// Extract the type guard type from a function declaration/signature node.
/// Returns `Some("Foo")` for `function isStr(x: unknown): x is Foo`.
/// Returns `None` if the function does not have a type predicate return.
///
/// Per 08-RESEARCH: type guard returns use `type_predicate_annotation` (not `type_annotation`).
/// The `type_predicate` child contains the parameter name and guarded type.
pub fn extract_type_guard(fn_node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = fn_node.walk();
    for child in fn_node.children(&mut cursor) {
        if child.kind() == "type_predicate_annotation" {
            // Find the type_predicate child
            let mut inner_cursor = child.walk();
            for inner_child in child.children(&mut inner_cursor) {
                if inner_child.kind() == "type_predicate" {
                    // The guarded type is the last named child (type node)
                    // Structure: identifier "is" type
                    let type_node = inner_child.child_by_field_name("type").or_else(|| {
                        // Fallback: last named child that is a type (not the identifier)
                        let mut tc = inner_child.walk();
                        inner_child
                            .children(&mut tc)
                            .filter(|c| c.is_named() && c.kind() != "identifier")
                            .last()
                    });
                    if let Some(tn) = type_node {
                        return tn.utf8_text(source).ok().map(|s| s.to_string());
                    }
                }
            }
        }
    }
    None
}

/// An index signature extracted from an interface or object type.
/// E.g., `[key: string]: number` has key_type_text="string", value_type_text="number".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSignature {
    pub key_type_text: String,
    pub value_type_text: String,
}

/// Extract all index signatures from an interface_body or object_type node.
/// Returns empty Vec if node has no index_signature children.
pub fn extract_index_signatures(
    body_node: tree_sitter::Node,
    source: &[u8],
) -> Vec<IndexSignature> {
    let mut sigs = Vec::new();
    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        if child.kind() == "index_signature" {
            let mut key_type = None;
            let mut value_type = None;
            let mut in_brackets = false;
            let mut ic = child.walk();
            for c in child.children(&mut ic) {
                match c.kind() {
                    "[" => in_brackets = true,
                    "]" => in_brackets = false,
                    "type_annotation" if !in_brackets => {
                        // Value type is the type_annotation after the brackets
                        // Iterate children with for loop to avoid cursor borrow issues
                        let mut tc = c.walk();
                        for type_child in c.children(&mut tc) {
                            if type_child.is_named() {
                                value_type =
                                    type_child.utf8_text(source).ok().map(|s| s.to_string());
                                break;
                            }
                        }
                    }
                    _ if in_brackets && c.is_named() && c.kind() != "identifier" => {
                        // Key type is the named non-identifier child inside brackets
                        // (predefined_type like "string" or "number")
                        if key_type.is_none() {
                            key_type = c.utf8_text(source).ok().map(|s| s.to_string());
                        }
                    }
                    _ => {}
                }
            }
            if let (Some(kt), Some(vt)) = (key_type, value_type) {
                sigs.push(IndexSignature {
                    key_type_text: kt,
                    value_type_text: vt,
                });
            }
        }
    }
    sigs
}

/// Find the interface_body child of an interface_declaration node.
/// Returns None if the declaration has no body (forward declaration).
pub fn find_interface_body(decl_node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = decl_node.walk();
    // Use a local result variable so the cursor is dropped before function return
    let result = decl_node
        .children(&mut cursor)
        .find(|child| child.kind() == "interface_body" || child.kind() == "object_type");
    result
}

/// Extract the text of the first named child of a type_annotation node.
/// Returns None if no named child is found.
fn first_named_child_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            return child.utf8_text(source).ok();
        }
    }
    None
}

/// Find the `rest_pattern` child of a `required_parameter` node, then return the
/// `identifier` text inside that rest_pattern (the param name).
fn rest_param_name<'a>(param: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut pc = param.walk();
    for child in param.children(&mut pc) {
        if child.kind() == "rest_pattern" {
            let mut rc = child.walk();
            for inner in child.children(&mut rc) {
                if inner.kind() == "identifier" {
                    return inner.utf8_text(source).ok();
                }
            }
        }
    }
    None
}

/// Build a TypeShape::Function from a function_signature or function_declaration node.
/// Extracts parameter names, types, optionality, and rest flags.
/// Used for overload signature extraction.
pub fn build_function_shape_from_node(
    fn_node: tree_sitter::Node,
    source: &[u8],
) -> Option<crate::types::shape::TypeShape> {
    use crate::types::shape::{ParamShape, PrimitiveKind, TypeShape};

    // Find formal_parameters child
    let params_node = fn_node.child_by_field_name("parameters")?;
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for param in params_node.children(&mut cursor) {
        match param.kind() {
            "required_parameter" => {
                let rest = is_rest_param(param);
                let name = if rest {
                    // For rest params, the identifier is inside rest_pattern
                    rest_param_name(param, source).unwrap_or("_").to_string()
                } else {
                    param
                        .child_by_field_name("pattern")
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("_")
                        .to_string()
                };
                let type_text = param
                    .child_by_field_name("type")
                    .and_then(|ta| first_named_child_text(ta, source))
                    .unwrap_or("unknown");
                params.push(ParamShape {
                    name,
                    optional: false,
                    rest,
                    shape: TypeShape::Opaque(type_text.to_string()),
                });
            }
            "optional_parameter" => {
                let name = param
                    .child_by_field_name("pattern")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("_")
                    .to_string();
                let type_text = param
                    .child_by_field_name("type")
                    .and_then(|ta| first_named_child_text(ta, source))
                    .unwrap_or("unknown");
                params.push(ParamShape {
                    name,
                    optional: true,
                    rest: false,
                    shape: TypeShape::Opaque(type_text.to_string()),
                });
            }
            _ => {} // skip commas, other tokens
        }
    }

    // Extract return type
    let return_type = fn_node
        .child_by_field_name("return_type")
        .and_then(|ta| first_named_child_text(ta, source))
        .map(|s| TypeShape::Opaque(s.to_string()))
        .unwrap_or(TypeShape::Primitive(PrimitiveKind::Void));

    Some(TypeShape::Function {
        params,
        return_type: Box::new(return_type),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect (line, col) positions of names captured by the named re-export query.
/// Used to exclude those positions from the plain named export results.
fn collect_reexport_positions(
    source: &[u8],
    language: &tree_sitter::Language,
) -> anyhow::Result<std::collections::HashSet<(u32, u32)>> {
    let mut positions = std::collections::HashSet::new();

    let query = Query::new(language, NAMED_REEXPORT_QUERY)
        .context("failed to compile NAMED_REEXPORT_QUERY for dedup")?;
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .context("failed to set language for dedup parser")?;
    let tree = parser
        .parse(source, None)
        .context("parse failed in dedup")?;
    let root = tree.root_node();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source);
    while let Some(m) = matches.next() {
        for capture in m.captures {
            if capture.index == 0 {
                // @name
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                positions.insert((line, col));
            }
        }
    }

    Ok(positions)
}

/// Strip leading and trailing quote characters from a string literal as returned
/// by tree-sitter (e.g., `"'./utils'"` -> `"./utils"`).
fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
        || (s.starts_with('`') && s.ends_with('`'))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ClassMemberKind, EnumValueKind, Visibility};
    use tree_sitter::Parser;

    /// Parse a TypeScript source string and return the root node and source bytes.
    fn parse_ts(source: &str) -> (tree_sitter::Tree, Vec<u8>) {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&lang).unwrap();
        let bytes = source.as_bytes().to_vec();
        let tree = parser.parse(&bytes, None).expect("parse failed");
        (tree, bytes)
    }

    /// Get the first child of root with the given kind.
    fn first_node_of_kind<'a>(
        root: tree_sitter::Node<'a>,
        kind: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() == kind {
                return Some(child);
            }
        }
        None
    }

    // ── parse_class_members tests ─────────────────────────────────────────────

    #[test]
    fn test_parse_class_members_extracts_method_names() {
        let src = "class Foo { doWork(x: string): void {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        assert!(
            members.iter().any(|m| m.name == "doWork"),
            "should extract doWork method"
        );
    }

    #[test]
    fn test_parse_class_members_no_modifier_is_public() {
        let src = "class Foo { doWork(): void {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let method = members.iter().find(|m| m.name == "doWork").unwrap();
        assert_eq!(method.visibility, Visibility::Public, "no modifier = Public");
    }

    #[test]
    fn test_parse_class_members_protected_modifier() {
        let src = "class Foo { protected doWork(): void {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let method = members.iter().find(|m| m.name == "doWork").unwrap();
        assert_eq!(method.visibility, Visibility::Protected);
    }

    #[test]
    fn test_parse_class_members_constructor_kind() {
        let src = "class Foo { constructor(x: string) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let ctor = members.iter().find(|m| m.name == "constructor").unwrap();
        assert_eq!(ctor.kind, ClassMemberKind::Constructor);
    }

    #[test]
    fn test_parse_class_members_constructor_required_params() {
        let src = "class Foo { constructor(x: string, y: number) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let ctor = members.iter().find(|m| m.name == "constructor").unwrap();
        assert_eq!(ctor.param_count, 2, "two required params");
    }

    #[test]
    fn test_parse_class_members_static_flag() {
        let src = "class Foo { static create(): Foo { return new Foo(); } }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let method = members.iter().find(|m| m.name == "create").unwrap();
        assert!(method.is_static, "create should be static");
    }

    #[test]
    fn test_parse_class_members_property_kind() {
        let src = "class Foo { name: string = ''; }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let prop = members.iter().find(|m| m.name == "name").unwrap();
        assert_eq!(prop.kind, ClassMemberKind::Property);
    }

    // ── abstract class tests ──────────────────────────────────────────────────

    #[test]
    fn test_abstract_class_detection_via_node_kind() {
        let src = "abstract class Base { abstract doWork(): void; }";
        let (tree, bytes) = parse_ts(src);
        let abs_node =
            first_node_of_kind(tree.root_node(), "abstract_class_declaration").unwrap();
        assert_eq!(abs_node.kind(), "abstract_class_declaration");
        let shape = parse_class_shape(&bytes, abs_node);
        assert!(shape.is_abstract, "should detect as abstract");
    }

    #[test]
    fn test_concrete_class_not_abstract() {
        let src = "class Foo {}";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let shape = parse_class_shape(&bytes, class_node);
        assert!(!shape.is_abstract, "concrete class should not be abstract");
    }

    // ── DECL_EXPORT_QUERY_TS abstract class test ──────────────────────────────

    #[test]
    fn test_decl_export_query_captures_abstract_class() {
        let src = "export abstract class MyAbstractClass { abstract doWork(): void; }";
        let exports = parse_exports(src.as_bytes(), SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "MyAbstractClass"),
            "exported abstract class should be captured"
        );
    }

    // ── parse_enum_members tests ──────────────────────────────────────────────

    #[test]
    fn test_parse_enum_members_extracts_names() {
        let src = "enum Color { Red, Green, Blue }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        let members = parse_enum_members(&bytes, enum_node);
        assert_eq!(members.len(), 3);
        assert_eq!(members[0].name, "Red");
        assert_eq!(members[1].name, "Green");
        assert_eq!(members[2].name, "Blue");
    }

    #[test]
    fn test_parse_enum_members_no_explicit_value() {
        let src = "enum Dir { Up, Down }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        let members = parse_enum_members(&bytes, enum_node);
        assert!(
            members.iter().all(|m| m.raw_value.is_none()),
            "implicit members should have no raw_value"
        );
    }

    #[test]
    fn test_parse_enum_members_with_explicit_numeric_value() {
        let src = "enum Status { Active = 1, Inactive = 2 }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        let members = parse_enum_members(&bytes, enum_node);
        let active = members.iter().find(|m| m.name == "Active").unwrap();
        assert_eq!(active.raw_value.as_deref(), Some("1"));
        assert_eq!(active.computed_value, Some(1));
    }

    // ── is_const_enum tests ───────────────────────────────────────────────────

    #[test]
    fn test_is_const_enum_true_for_const_enum() {
        let src = "const enum Direction { Up, Down }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        assert!(is_const_enum(enum_node, &bytes), "should detect const enum");
    }

    #[test]
    fn test_is_const_enum_false_for_regular_enum() {
        let src = "enum Direction { Up, Down }";
        let (tree, bytes) = parse_ts(src);
        let enum_node = first_node_of_kind(tree.root_node(), "enum_declaration").unwrap();
        assert!(
            !is_const_enum(enum_node, &bytes),
            "regular enum should not be const"
        );
    }

    // ── compute_enum_values tests ─────────────────────────────────────────────

    #[test]
    fn test_compute_enum_values_auto_increment_from_zero() {
        let mut members = vec![
            EnumMember {
                name: "A".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
            EnumMember {
                name: "B".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
            EnumMember {
                name: "C".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
        ];
        compute_enum_values(&mut members);
        assert_eq!(members[0].computed_value, Some(0));
        assert_eq!(members[1].computed_value, Some(1));
        assert_eq!(members[2].computed_value, Some(2));
    }

    #[test]
    fn test_compute_enum_values_resets_after_explicit_numeric() {
        let mut members = vec![
            EnumMember {
                name: "A".to_string(),
                raw_value: Some("5".to_string()),
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
            EnumMember {
                name: "B".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
        ];
        compute_enum_values(&mut members);
        assert_eq!(members[0].computed_value, Some(5));
        assert_eq!(members[1].computed_value, Some(6), "should reset to 5+1=6");
    }

    #[test]
    fn test_compute_enum_values_non_numeric_sets_none() {
        let mut members = vec![
            EnumMember {
                name: "A".to_string(),
                raw_value: Some("SomeExpr + 1".to_string()),
                computed_value: None,
                value_kind: EnumValueKind::OtherExpr,
            },
            EnumMember {
                name: "B".to_string(),
                raw_value: None,
                computed_value: None,
                value_kind: EnumValueKind::Numeric,
            },
        ];
        compute_enum_values(&mut members);
        assert_eq!(
            members[0].computed_value, None,
            "non-numeric initializer = None"
        );
        // Counter still increments for B
        assert_eq!(members[1].computed_value, Some(1));
    }

    // ── parse_type_params tests ───────────────────────────────────────────────

    #[test]
    fn test_parse_type_params_extracts_name_constraint_default() {
        let src = "interface Foo<T extends string = never> {}";
        let (tree, bytes) = parse_ts(src);
        let iface_node =
            first_node_of_kind(tree.root_node(), "interface_declaration").unwrap();
        let params = parse_type_params(&bytes, iface_node);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "T");
        assert_eq!(params[0].constraint_text.as_deref(), Some("string"));
        assert_eq!(params[0].default_text.as_deref(), Some("never"));
    }

    #[test]
    fn test_parse_type_params_empty_for_no_type_params() {
        let src = "class Foo {}";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let params = parse_type_params(&bytes, class_node);
        assert!(params.is_empty(), "no type parameters = empty Vec");
    }

    #[test]
    fn test_parse_type_params_multiple_params() {
        let src = "type Pair<K, V> = [K, V];";
        let (tree, bytes) = parse_ts(src);
        let alias_node =
            first_node_of_kind(tree.root_node(), "type_alias_declaration").unwrap();
        let params = parse_type_params(&bytes, alias_node);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "K");
        assert_eq!(params[1].name, "V");
        assert!(params[0].constraint_text.is_none());
        assert!(params[0].default_text.is_none());
    }

    #[test]
    fn test_parse_type_params_constraint_shape_is_none() {
        let src = "function foo<T extends string>() {}";
        let (tree, bytes) = parse_ts(src);
        let fn_node = first_node_of_kind(tree.root_node(), "function_declaration").unwrap();
        let params = parse_type_params(&bytes, fn_node);
        assert_eq!(params.len(), 1);
        assert!(
            params[0].constraint_shape.is_none(),
            "constraint_shape is None (LSP fills in later)"
        );
    }

    // ── DeclarationKind propagation tests (Task 1) ────────────────────────────

    #[test]
    fn test_decl_export_query_captures_namespace() {
        let source = b"export namespace MyNS { export function helper(): void {} }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "MyNS"),
            "namespace export should be captured"
        );
        let ns = exports.iter().find(|e| e.name == "MyNS").unwrap();
        assert_eq!(ns.decl_kind, Some(crate::types::DeclarationKind::Namespace));
    }

    #[test]
    fn test_decl_kind_propagated_for_function() {
        let source = b"export function greet(name: string): string { return name; }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let exp = exports.iter().find(|e| e.name == "greet").unwrap();
        assert_eq!(
            exp.decl_kind,
            Some(crate::types::DeclarationKind::Function)
        );
    }

    #[test]
    fn test_decl_kind_propagated_for_interface() {
        let source = b"export interface Config { timeout: number; }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let exp = exports.iter().find(|e| e.name == "Config").unwrap();
        assert_eq!(
            exp.decl_kind,
            Some(crate::types::DeclarationKind::Interface)
        );
    }

    #[test]
    fn test_decl_kind_propagated_for_type_alias() {
        let source = b"export type ID = string;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let exp = exports.iter().find(|e| e.name == "ID").unwrap();
        assert_eq!(
            exp.decl_kind,
            Some(crate::types::DeclarationKind::TypeAlias)
        );
    }

    #[test]
    fn test_function_signature_captured_as_overload() {
        let source = b"export function foo(x: string): string;\nexport function foo(x: number): number;\nexport function foo(x: any): any { return x; }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let foos: Vec<_> = exports.iter().filter(|e| e.name == "foo").collect();
        // function_signature nodes (overloads) + function_declaration (implementation)
        assert!(
            foos.len() >= 3,
            "should capture overload signatures and implementation; got {}",
            foos.len()
        );
    }

    // ── Rest param, type guard, index sig, function shape tests (Task 2) ─────

    #[test]
    fn test_is_rest_param_detects_spread() {
        let source = b"function foo(...args: string[]): void {}";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        let params = fn_node.child_by_field_name("parameters").unwrap();
        let mut cursor = params.walk();
        let first_param = params
            .children(&mut cursor)
            .find(|c| c.kind() == "required_parameter")
            .unwrap();
        assert!(is_rest_param(first_param), "...args should be detected as rest param");
    }

    #[test]
    fn test_is_rest_param_false_for_normal() {
        let source = b"function foo(x: string): void {}";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        let params = fn_node.child_by_field_name("parameters").unwrap();
        let mut cursor = params.walk();
        let first_param = params
            .children(&mut cursor)
            .find(|c| c.kind() == "required_parameter")
            .unwrap();
        assert!(!is_rest_param(first_param), "normal param should not be rest");
    }

    #[test]
    fn test_extract_type_guard_returns_guarded_type() {
        let source = b"function isStr(x: unknown): x is string { return typeof x === 'string'; }";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let fn_node = tree.root_node().child(0).unwrap();
        let guard = extract_type_guard(fn_node, source);
        assert_eq!(guard, Some("string".to_string()));
    }

    #[test]
    fn test_extract_type_guard_none_for_regular_return() {
        let source = b"function foo(x: string): boolean { return true; }";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let fn_node = tree.root_node().child(0).unwrap();
        let guard = extract_type_guard(fn_node, source);
        assert!(guard.is_none(), "regular boolean return should not be a type guard");
    }

    #[test]
    fn test_extract_index_signatures() {
        let source = b"interface Dict { [key: string]: number; name: string; }";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let iface = root.child(0).unwrap(); // interface_declaration
        let body = find_interface_body(iface).unwrap();
        let sigs = extract_index_signatures(body, source);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].key_type_text, "string");
        assert_eq!(sigs[0].value_type_text, "number");
    }

    #[test]
    fn test_build_function_shape_from_signature_node() {
        let source =
            b"export function foo(x: string): number;\nexport function foo(x: any): any { return x; }";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        // First child is export_statement containing function_signature
        let first_export = root.child(0).unwrap();
        let mut cursor = first_export.walk();
        let fn_sig = first_export
            .children(&mut cursor)
            .find(|c| c.kind() == "function_signature")
            .unwrap();
        let shape = build_function_shape_from_node(fn_sig, source);
        assert!(shape.is_some(), "should build TypeShape from function_signature");
        if let Some(crate::types::shape::TypeShape::Function { params, .. }) = shape {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
            assert!(!params[0].rest);
        } else {
            panic!("expected TypeShape::Function");
        }
    }

    #[test]
    fn test_build_function_shape_with_rest_param() {
        let source = b"function bar(...args: string[]): void {}";
        let mut parser = Parser::new();
        let ts_lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let fn_node = tree.root_node().child(0).unwrap();
        let shape = build_function_shape_from_node(fn_node, source);
        assert!(shape.is_some());
        if let Some(crate::types::shape::TypeShape::Function { params, .. }) = shape {
            assert_eq!(params.len(), 1);
            assert!(params[0].rest, "rest param should have rest=true");
        } else {
            panic!("expected TypeShape::Function");
        }
    }

    // ── is_async_function / is_generator_function tests (BEH-01, BEH-02) ─────

    #[test]
    fn test_is_async_function_true() {
        let source = b"async function foo() {}";
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        assert!(is_async_function(fn_node, source));
    }

    #[test]
    fn test_is_async_function_false() {
        let source = b"function foo() {}";
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        assert!(!is_async_function(fn_node, source));
    }

    #[test]
    fn test_is_generator_function_true() {
        let source = b"function* foo() {}";
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        assert!(is_generator_function(fn_node, source));
    }

    #[test]
    fn test_is_generator_function_false() {
        let source = b"function foo() {}";
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let fn_node = root.child(0).unwrap();
        assert!(!is_generator_function(fn_node, source));
    }
}
