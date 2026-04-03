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

/// Captures `export { foo }` (no alias, no `from` clause).
///
/// We match `export_statement` nodes that contain an `export_clause` but no
/// `source` child — that distinguishes plain named exports from named re-exports.
/// For `export { foo as bar }` see `NAMED_EXPORT_ALIAS_QUERY`.
///
/// GAP-13-03 (inline type modifiers): `export { type Foo, Bar }` is handled
/// transparently by this query because the grammar's `export_specifier` matches
/// the `identifier` @name node regardless of a preceding per-specifier `"type"` token.
/// Both `Foo` and `Bar` appear exactly once as Named exports.
const NAMED_EXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @name)))
"#;

/// Captures `export { foo as bar }` (with alias, no `from` clause).
///
/// Capture index 0 = `@local_name` (the local binding, e.g. "foo").
/// Capture index 1 = `@alias_name` (the public export name, e.g. "bar").
/// We process this query first so the alias name is used instead of the local name.
const NAMED_EXPORT_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name)))
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
///
/// Capture index 0 = `@ns_name` (the namespace identifier, e.g. "utils").
/// Capture index 1 = `@source` (the module specifier string).
const NAMESPACE_EXPORT_QUERY: &str = r#"
(export_statement
  (namespace_export
    (identifier) @ns_name)
  source: (string) @source)
"#;

/// Captures `export { foo } from './module'` (no alias).
/// For `export { foo as bar } from './module'` see `NAMED_REEXPORT_ALIAS_QUERY`.
const NAMED_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @name))
  source: (string) @source)
"#;

/// Captures `export { foo as bar } from './module'` (with alias).
///
/// Capture index 0 = `@local_name` (the local name in the source module, e.g. "foo").
/// Capture index 1 = `@alias_name` (the public re-export name, e.g. "bar").
/// Capture index 2 = `@source` (the module specifier string).
/// We process this query first so the alias name is used as the exported name.
const NAMED_REEXPORT_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name))
  source: (string) @source)
"#;

/// Captures `export type { Foo }` (TS 3.8+ type-only named exports, no `from` clause).
///
/// The `"type"` keyword is an anonymous token child of `export_statement`.
/// Type-only exports are public API surface and must be tracked in the export map.
/// See Phase 12 RESEARCH Pattern 1.
const TYPE_ONLY_NAMED_EXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @name)))
"#;

/// Captures `export type { Foo } from './bar'` (TS 3.8+ type-only named re-exports).
///
/// Capture index 0 = `@name`, capture index 1 = `@source`.
const TYPE_ONLY_NAMED_REEXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @name))
  source: (string) @source)
"#;

/// Captures `export type { Foo as Bar }` (type-only with alias, no `from` clause).
///
/// Capture index 0 = `@local_name`, capture index 1 = `@alias_name`.
/// The alias_name is the public exported name.
const TYPE_ONLY_ALIAS_EXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name)))
"#;

/// Captures `export type { Foo as Bar } from './bar'` (type-only with alias and re-export).
///
/// Capture index 0 = `@local_name`, capture index 1 = `@alias_name`, capture index 2 = `@source`.
const TYPE_ONLY_ALIAS_REEXPORT_QUERY_TS: &str = r#"
(export_statement
  "type"
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name))
  source: (string) @source)
"#;

/// Captures `export const { foo, bar } = obj` — shorthand destructuring patterns.
///
/// Each `shorthand_property_identifier_pattern` is a separately exported name.
/// See Phase 12 RESEARCH Pattern 2.
const DESTRUCTURED_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (object_pattern
        (shorthand_property_identifier_pattern) @name))))
"#;

/// Captures `export const { foo: renamed } = obj` — pair-pattern destructuring.
///
/// The `value` (identifier) is the PUBLIC exported name (the local binding).
/// The `key` is the source property.
/// See Phase 12 RESEARCH Pattern 2 and BUG-13-03 fix.
const DESTRUCTURED_PAIR_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (object_pattern
        (pair_pattern
          value: (identifier) @name)))))
"#;

/// Captures `export = Foo` — CommonJS-style TypeScript module export.
///
/// This is a TypeScript-only syntax. The export is treated as a default export
/// with name "default" in interlint's model (equivalent to `module.exports = Foo`).
/// See Phase 12 RESEARCH Pattern 3.
const EXPORT_ASSIGNMENT_QUERY_TS: &str = r#"
(export_statement
  "="
  (_) @value)
"#;

/// Captures `export declare function/class/interface/...` — ambient declarations.
///
/// In `.d.ts` files (and `.ts` files with `declare` blocks), exported declarations
/// are wrapped in an `ambient_declaration` node. The existing `DECL_EXPORT_QUERY_TS`
/// does NOT match through this wrapper.
///
/// This is critical for packages that ship `.d.ts` type declaration files —
/// virtually all TypeScript packages. Their public API is expressed as
/// `declare` statements.
/// See Phase 12 RESEARCH Pattern 6 (Gap 8).
const AMBIENT_DECL_EXPORT_QUERY_TS: &str = r#"
(export_statement
  declaration: (ambient_declaration
    [
      (function_declaration name: (identifier) @name)
      (function_signature name: (identifier) @name)
      (class_declaration name: (type_identifier) @name)
      (abstract_class_declaration name: (type_identifier) @name)
      (type_alias_declaration name: (type_identifier) @name)
      (interface_declaration name: (type_identifier) @name)
      (enum_declaration name: (identifier) @name)
      (internal_module name: (identifier) @name)
      (lexical_declaration (variable_declarator name: (identifier) @name))
    ]))
"#;

/// Captures `export { default as Named }` — keyword `default` as source with alias.
/// The `default` keyword is an unnamed node, not `identifier`, so existing queries miss it.
/// GAP-13-02.
const DEFAULT_SPECIFIER_ALIAS_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"
      alias: (identifier) @alias_name)))
"#;

/// Captures `export { default } from './foo'` — re-export of default.
/// GAP-13-02.
const DEFAULT_SPECIFIER_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"))
  source: (string) @source)
"#;

/// Captures `export { default as Named } from './foo'` — re-export default with alias.
/// GAP-13-02.
const DEFAULT_SPECIFIER_ALIAS_REEXPORT_QUERY: &str = r#"
(export_statement
  (export_clause
    (export_specifier
      name: "default"
      alias: (identifier) @alias_name))
  source: (string) @source)
"#;

/// Captures `export const [a, b] = arr` — array destructuring with named identifiers.
/// GAP-13-04.
const ARRAY_DESTRUCTURED_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (array_pattern
        (identifier) @name))))
"#;

/// Captures rest patterns in array destructuring: `export const [a, ...rest] = arr`.
/// GAP-13-04.
const ARRAY_DESTRUCTURED_REST_EXPORT_QUERY: &str = r#"
(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (array_pattern
        (rest_pattern (identifier) @name)))))
"#;

/// Captures `module.exports = { foo, bar }` — CJS bulk export with shorthand properties.
/// JavaScript/CJS only. Each shorthand_property_identifier is a named export.
/// GAP-13-01.
const CJS_MODULE_EXPORTS_SHORTHAND_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (object
      (shorthand_property_identifier) @name)))
"#;

/// Captures `module.exports = { key: value }` — CJS bulk export with explicit pairs.
/// The key (property_identifier) is the exported name.
/// GAP-13-01.
const CJS_MODULE_EXPORTS_PAIR_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (object
      (pair
        key: (property_identifier) @name))))
"#;

/// Captures `exports.foo = bar` — CJS property-by-property export.
/// The property identifier is the exported name.
/// GAP-13-01.
const CJS_EXPORTS_PROPERTY_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @name)))
"#;

/// Captures `module.exports = SomeIdentifier` — CJS single default-like export.
/// GAP-13-01.
const CJS_MODULE_EXPORTS_DEFAULT_QUERY: &str = r#"
(expression_statement
  (assignment_expression
    left: (member_expression
      object: (identifier) @obj
      property: (property_identifier) @prop)
    right: (identifier) @value))
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

    // Tracks (line, col) positions of names captured by type-only re-export queries
    // so the later NAMED_REEXPORT_QUERY section can skip them (avoiding double-emit).
    let mut type_only_reexport_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();

    // --- `export { default as Named }` forms (GAP-13-02) ----------------------
    // Process BEFORE regular named export queries to set positions for dedup.
    // The `default` keyword is an unnamed node in the grammar, so existing
    // identifier-based queries miss it.

    // 1. `export { default as Named } from './foo'` — re-export with alias
    if let Ok(query) = Query::new(&language, DEFAULT_SPECIFIER_ALIAS_REEXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            let mut alias_opt: Option<(String, u32, u32)> = None;
            let mut src_opt: Option<String> = None;
            for capture in m.captures {
                let node = capture.node;
                if capture.index == 0 {
                    // @alias_name
                    if let Ok(text) = node.utf8_text(source) {
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        alias_opt = Some((text.to_string(), line, col));
                    }
                } else if capture.index == 1 {
                    // @source
                    if let Ok(text) = node.utf8_text(source) {
                        src_opt = Some(strip_quotes(text).to_string());
                    }
                }
            }
            if let (Some((name, line, col)), Some(src)) = (alias_opt, src_opt) {
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

    // 2. `export { default } from './foo'` — re-export default (no alias)
    if let Ok(query) = Query::new(&language, DEFAULT_SPECIFIER_REEXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @source
                    let node = capture.node;
                    if let Ok(text) = node.utf8_text(source) {
                        let src = strip_quotes(text).to_string();
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        let already = exports
                            .iter()
                            .any(|e| e.name == "default" && e.source.as_deref() == Some(&src));
                        if !already {
                            exports.push(RawExport {
                                name: "default".to_string(),
                                kind: RawExportKind::NamedReexport,
                                line,
                                col,
                                source: Some(src),
                                decl_kind: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // 3. `export { default as Named }` — local default aliased to Named (no from clause)
    if let Ok(query) = Query::new(&language, DEFAULT_SPECIFIER_ALIAS_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @alias_name
                    let node = capture.node;
                    if let Ok(text) = node.utf8_text(source) {
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        exports.push(RawExport {
                            name: text.to_string(),
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
    }

    // Process aliased named exports first: `export { foo as bar }`.
    // We track their local-name positions so the plain query below skips them.
    let mut alias_local_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();
    {
        let query = Query::new(&language, NAMED_EXPORT_ALIAS_QUERY)
            .context("failed to compile NAMED_EXPORT_ALIAS_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // index 0 = @local_name, index 1 = @alias_name
            let mut local_pos: Option<(u32, u32)> = None;
            let mut alias_name_opt: Option<(String, u32, u32)> = None;
            for capture in m.captures {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    local_pos = Some((line, col));
                } else if capture.index == 1 {
                    // alias_name is the PUBLIC exported name
                    if !reexport_positions.contains(&(line, col)) {
                        let name = node
                            .utf8_text(source)
                            .context("UTF-8 decode failed on alias name")?
                            .to_string();
                        alias_name_opt = Some((name, line, col));
                    }
                }
            }
            if let (Some(lp), Some((name, line, col))) = (local_pos, alias_name_opt) {
                alias_local_positions.insert(lp);
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

    // Process non-aliased named exports: `export { foo }`.
    // Skip positions already handled by the alias query or re-export query.
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
                    // Skip if already handled by the alias query above.
                    if alias_local_positions.contains(&(line, col)) {
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

    // --- Type-only exports (TypeScript only: `export type { Foo }`) ------------
    // TS 3.8+ type-only export forms. The `"type"` keyword is an anonymous token.
    // These are tracked as Named/NamedReexport — same kind as regular named exports.
    // Processing order: alias forms first (to set type_only_alias_local_positions),
    // then non-alias forms (skipping positions handled by alias queries).
    // We also skip positions already in reexport_positions to avoid double-counting.
    if matches!(lang, SourceLanguage::TypeScript) {
        let mut type_only_alias_local_positions: std::collections::HashSet<(u32, u32)> =
            std::collections::HashSet::new();

        // Process `export type { Foo as Bar }` (no from clause)
        if let Ok(query) = Query::new(&language, TYPE_ONLY_ALIAS_EXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut local_pos: Option<(u32, u32)> = None;
                let mut alias_name_opt: Option<(String, u32, u32)> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        // @local_name
                        local_pos = Some((line, col));
                    } else if capture.index == 1 {
                        // @alias_name — public exported name; skip if it's a re-export
                        if !reexport_positions.contains(&(line, col)) {
                            if let Ok(name) = node.utf8_text(source) {
                                alias_name_opt = Some((name.to_string(), line, col));
                            }
                        }
                    }
                }
                if let (Some(lp), Some((name, line, col))) = (local_pos, alias_name_opt) {
                    type_only_alias_local_positions.insert(lp);
                    // Skip if already captured by the non-type-only alias query
                    if !alias_local_positions.contains(&lp) {
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

        // Process `export type { Foo as Bar } from './bar'` (with source)
        if let Ok(query) = Query::new(&language, TYPE_ONLY_ALIAS_REEXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut local_pos: Option<(u32, u32)> = None;
                let mut alias_name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        local_pos = Some((line, col));
                    } else if capture.index == 1 {
                        if let Ok(name) = node.utf8_text(source) {
                            alias_name_opt = Some((name.to_string(), line, col));
                        }
                    } else if capture.index == 2 {
                        if let Ok(text) = node.utf8_text(source) {
                            src_opt = Some(strip_quotes(text).to_string());
                        }
                    }
                }
                if let (Some(lp), Some((name, line, col)), Some(src)) =
                    (local_pos, alias_name_opt, src_opt)
                {
                    type_only_alias_local_positions.insert(lp);
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

        // Process `export type { Foo }` (no alias, no from clause)
        if let Ok(query) = Query::new(&language, TYPE_ONLY_NAMED_EXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        let node = capture.node;
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        // Skip if already a re-export position
                        if reexport_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if handled by alias query above
                        if type_only_alias_local_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if already handled by the non-type-only alias query
                        if alias_local_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if already captured by the non-type-only named export query
                        // (the existing NAMED_EXPORT_QUERY also matches these nodes)
                        let already = exports.iter().any(|e| {
                            e.kind == RawExportKind::Named && e.line == line && e.col == col
                        });
                        if !already {
                            if let Ok(name) = node.utf8_text(source) {
                                exports.push(RawExport {
                                    name: name.to_string(),
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
            }
        }

        // Process `export type { Foo } from './bar'` (no alias, with source)
        if let Ok(query) = Query::new(&language, TYPE_ONLY_NAMED_REEXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        // @name
                        if type_only_alias_local_positions.contains(&(line, col)) {
                            break; // alias variant already handled
                        }
                        if let Ok(name) = node.utf8_text(source) {
                            name_opt = Some((name.to_string(), line, col));
                        }
                    } else if capture.index == 1 {
                        if let Ok(text) = node.utf8_text(source) {
                            src_opt = Some(strip_quotes(text).to_string());
                        }
                    }
                }
                if let (Some((name, line, col)), Some(src)) = (name_opt, src_opt) {
                    // Track this position so the later NAMED_REEXPORT_QUERY section
                    // skips it and avoids emitting a duplicate NamedReexport entry.
                    type_only_reexport_positions.insert((line, col));
                    // Skip if already captured by non-type-only named re-export query
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::NamedReexport
                            && e.line == line
                            && e.col == col
                    });
                    if !already {
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

    // --- Export assignment (`export = Foo`, TypeScript only) -----------------
    // CommonJS-style TypeScript default export. Produces a "default" entry.
    // Only emit if no default export already exists at this position.
    if matches!(lang, SourceLanguage::TypeScript) {
        if let Ok(query) = Query::new(&language, EXPORT_ASSIGNMENT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                if let Some(capture) = m.captures.first() {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    // Only add if no default export has already been emitted
                    let already_emitted = exports
                        .iter()
                        .any(|e| e.kind == RawExportKind::Default && e.line == line);
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

    // --- Ambient declaration exports (`export declare ...`, TypeScript only) ---
    // In .d.ts files and `declare` blocks, declarations are wrapped in
    // `ambient_declaration`. The existing DECL_EXPORT_QUERY_TS doesn't match
    // through this wrapper.
    if matches!(lang, SourceLanguage::TypeScript) {
        if let Ok(query) = Query::new(&language, AMBIENT_DECL_EXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        let node = capture.node;
                        let name =
                            match node.utf8_text(source) {
                                Ok(n) => n.to_string(),
                                Err(_) => continue,
                            };

                        // Skip if already captured (ambient + non-ambient declarations
                        // on the same symbol would be redundant).
                        let already = exports.iter().any(|e| e.name == name);
                        if already {
                            continue;
                        }

                        // The captured @name node's parent is the inner declaration
                        // (e.g., function_declaration) OR variable_declarator (for lexical_declaration).
                        // For lexical_declaration: @name → variable_declarator → lexical_declaration.
                        // We need to check the correct level for decl_kind.
                        let decl_kind = node.parent().map(|immediate_parent| {
                            let parent_kind = immediate_parent.kind();
                            if parent_kind == "variable_declarator" {
                                // Navigate up one more level to lexical_declaration
                                immediate_parent
                                    .parent()
                                    .map(|ld| decl_kind_from_node_kind(ld.kind()))
                                    .unwrap_or(crate::types::DeclarationKind::Variable)
                            } else {
                                decl_kind_from_node_kind(parent_kind)
                            }
                        });

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
    }

    // --- Destructured variable exports (`export const { foo, bar } = obj`) ---
    // These are variable exports where the name field is an object_pattern instead of
    // an identifier. The existing DECL_EXPORT_QUERY misses these because it only
    // handles `variable_declarator name: (identifier)`.
    // Works for both JS and TS.

    // Shorthand patterns: `export const { foo, bar } = obj`
    if let Ok(query) = Query::new(&language, DESTRUCTURED_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(crate::types::DeclarationKind::Variable),
                        });
                    }
                }
            }
        }
    }

    // Pair patterns: `export const { foo: renamed } = obj` — key is the public name
    if let Ok(query) = Query::new(&language, DESTRUCTURED_PAIR_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(crate::types::DeclarationKind::Variable),
                        });
                    }
                }
            }
        }
    }

    // --- Star re-exports (plain `export * from '...'`) ------------------------
    // The JS/TS grammar represents `export * from '...'` with a bare `*` token
    // directly inside the export_statement (no namespace_export child node).
    if let Ok(query) = Query::new(&language, STAR_EXPORT_QUERY) {
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

    // --- Namespace re-exports (`export * as name from '...'`) -----------------
    // NAMESPACE_EXPORT_QUERY captures: index 0 = @ns_name, index 1 = @source.
    {
        if let Ok(query) = Query::new(&language, NAMESPACE_EXPORT_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut ns_name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let text = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on namespace re-export")?;
                    if capture.index == 0 {
                        // @ns_name — the public namespace identifier
                        ns_name_opt = Some((
                            text.to_string(),
                            node.start_position().row as u32 + 1,
                            node.start_position().column as u32 + 1,
                        ));
                    } else if capture.index == 1 {
                        // @source — the module specifier
                        src_opt = Some(strip_quotes(text).to_string());
                    }
                }
                if let (Some((ns_name, line, col)), Some(src)) = (ns_name_opt, src_opt) {
                    // BUG-13-02: Include the name in the dedup check so that a plain star
                    // re-export (name="*") and a namespace re-export (name="utils") from the
                    // same source are NOT considered duplicates and can coexist.
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::StarReexport
                            && e.source.as_deref() == Some(src.as_str())
                            && e.name == ns_name
                    });
                    if !already {
                        exports.push(RawExport {
                            name: ns_name,
                            kind: RawExportKind::StarReexport,
                            line,
                            col,
                            source: Some(src),
                            decl_kind: None,
                        });
                    }
                }
            }
        }
    }

    // --- Named re-exports -----------------------------------------------------
    // Process aliased re-exports first (`export { foo as bar } from './mod'`),
    // then non-aliased re-exports (`export { foo } from './mod'`).
    // Track local-name positions from the alias query to skip them in the plain query.
    let mut reexport_alias_local_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();
    {
        let query = Query::new(&language, NAMED_REEXPORT_ALIAS_QUERY)
            .context("failed to compile NAMED_REEXPORT_ALIAS_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // index 0 = @local_name, index 1 = @alias_name, index 2 = @source
            let mut local_pos: Option<(u32, u32)> = None;
            let mut alias_name_opt: Option<(String, u32, u32)> = None;
            let mut src_opt: Option<String> = None;
            for capture in m.captures {
                let node = capture.node;
                let text = node
                    .utf8_text(source)
                    .context("UTF-8 decode failed on named re-export alias")?;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    local_pos = Some((line, col));
                } else if capture.index == 1 {
                    alias_name_opt = Some((text.to_string(), line, col));
                } else if capture.index == 2 {
                    src_opt = Some(strip_quotes(text).to_string());
                }
            }
            if let (Some(lp), Some((name, line, col)), Some(src)) =
                (local_pos, alias_name_opt, src_opt)
            {
                reexport_alias_local_positions.insert(lp);
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
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    name_opt = Some((text.to_string(), line, col));
                } else if capture.index == 1 {
                    source_opt = Some(strip_quotes(text).to_string());
                }
            }

            if let (Some((name, line, col)), Some(src)) = (name_opt, source_opt) {
                // Skip if already handled by the alias query above.
                if reexport_alias_local_positions.contains(&(line, col)) {
                    continue;
                }
                // Skip if already handled by the type-only re-export query
                // (TypeScript `export type { Foo } from '...'` — avoids double-emit).
                if type_only_reexport_positions.contains(&(line, col)) {
                    continue;
                }
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

    // --- Array destructuring: `export const [a, b] = arr` (GAP-13-04) ----------
    if let Ok(query) = Query::new(&language, ARRAY_DESTRUCTURED_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(DeclarationKind::Variable),
                        });
                    }
                }
            }
        }
    }

    // Rest patterns: `export const [a, ...rest] = arr`
    if let Ok(query) = Query::new(&language, ARRAY_DESTRUCTURED_REST_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(DeclarationKind::Variable),
                        });
                    }
                }
            }
        }
    }

    // --- CJS module.exports / exports.foo detection (GAP-13-01) ---------------
    // Only for JavaScript files — TypeScript uses `export =` instead.
    // Validation (checking "module"/"exports" text) is done in Rust after capture
    // since #eq? predicates are not automatically applied by cursor.matches().
    if matches!(lang, SourceLanguage::JavaScript) {
        // 1. module.exports = { foo, bar } — shorthand properties
        if let Ok(query) = Query::new(&language, CJS_MODULE_EXPORTS_SHORTHAND_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Validate: capture 0 (@obj) must be "module", capture 1 (@prop) must be "exports"
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            // @name — shorthand property identifier
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. module.exports = { key: value } — explicit pair properties
        if let Ok(query) = Query::new(&language, CJS_MODULE_EXPORTS_PAIR_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. exports.foo = bar — property-by-property
        if let Ok(query) = Query::new(&language, CJS_EXPORTS_PROPERTY_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Validate: capture 0 (@obj) must be "exports" (not "module" or anything else)
                let obj_text = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                if obj_text == Some("exports") {
                    for capture in m.captures {
                        if capture.index == 1 {
                            // @name — the property being assigned
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. module.exports = SomeIdentifier — default-like export
        if let Ok(query) = Query::new(&language, CJS_MODULE_EXPORTS_DEFAULT_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            // @value — the identifier being assigned as default
                            let node = capture.node;
                            let line = node.start_position().row as u32 + 1;
                            let col = node.start_position().column as u32 + 1;
                            let already = exports.iter().any(|e| e.name == "default");
                            if !already {
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

                // Detect getter/setter by scanning anonymous children for "get"/"set" keywords.
                // Modifiers appear before the name field; stop at named nodes other than
                // accessibility_modifier to avoid false positives.
                let mut is_getter = false;
                let mut is_setter = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.is_named() && kw.kind() != "accessibility_modifier" {
                            break;
                        }
                        match kw.kind() {
                            "get" => is_getter = true,
                            "set" => is_setter = true,
                            _ => {}
                        }
                    }
                }

                let kind = if name == "constructor" {
                    ClassMemberKind::Constructor
                } else if is_getter {
                    ClassMemberKind::Getter
                } else if is_setter {
                    ClassMemberKind::Setter
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

                // ES private fields (#field) are truly runtime-private — not public API.
                // Exclude them to avoid false positives when they are added/removed.
                if name.starts_with('#') {
                    continue;
                }

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
            // BUG-13-01: .d.ts ambient class members use method_signature /
            // property_signature instead of method_definition / public_field_definition.
            "method_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let visibility = extract_visibility(child, source);
                let param_count = count_required_params(child);
                let mut is_static = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.kind() == "static" {
                            is_static = true;
                        }
                    }
                }
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Method,
                    visibility,
                    is_static,
                    is_abstract: false,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                });
            }
            "property_signature" => {
                let name_node = child.child_by_field_name("name");
                let name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() || name.starts_with('#') {
                    continue;
                }
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

/// Extract members from an `interface_body` node.
///
/// Interface members are `method_signature` and `property_signature` nodes
/// (same node kinds as .d.ts ambient class members, handled in Phase 13).
/// All interface members are public (no visibility modifiers in interfaces).
/// Reuses ClassMember type for consistency with class_diff pipeline.
pub fn parse_interface_members(source: &[u8], interface_body: tree_sitter::Node) -> Vec<ClassMember> {
    let mut members = Vec::new();
    let mut cursor = interface_body.walk();
    for child in interface_body.children(&mut cursor) {
        match child.kind() {
            "method_signature" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let param_count = count_required_params(child);
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Method,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_readonly: false,
                    param_count,
                    type_signature: None,
                });
            }
            "property_signature" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                // Detect readonly modifier
                let mut is_readonly = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if kw.kind() == "readonly" {
                            is_readonly = true;
                        }
                    }
                }
                // Detect optional (?) — property_signature with "?" anonymous child
                // Optional properties are non-breaking to add; required are breaking
                let mut is_optional = false;
                for j in 0..child.child_count() as u32 {
                    if let Some(kw) = child.child(j) {
                        if !kw.is_named() && kw.utf8_text(source).ok() == Some("?") {
                            is_optional = true;
                            break;
                        }
                    }
                }
                members.push(ClassMember {
                    name,
                    kind: ClassMemberKind::Property,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_readonly,
                    param_count: 0,
                    // Use type_signature to signal optional status for interface_diff
                    type_signature: if is_optional { Some("optional".to_string()) } else { None },
                });
            }
            "call_signature" => {
                // Call signatures: interface with () callable syntax
                // Treat as a method with a synthetic name "(call)"
                let param_count = count_required_params(child);
                members.push(ClassMember {
                    name: "(call)".to_string(),
                    kind: ClassMemberKind::Method,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
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

/// Collect (line, col) positions of local names captured by re-export queries.
/// Used to exclude those positions from the plain named export results.
///
/// Handles both `export { foo } from '...'` and `export { foo as bar } from '...'`.
/// For the alias form, the local-name (`foo`) position is collected so the plain
/// export query doesn't double-count it.
fn collect_reexport_positions(
    source: &[u8],
    language: &tree_sitter::Language,
) -> anyhow::Result<std::collections::HashSet<(u32, u32)>> {
    let mut positions = std::collections::HashSet::new();

    let mut parser = Parser::new();
    parser
        .set_language(language)
        .context("failed to set language for dedup parser")?;
    let tree = parser
        .parse(source, None)
        .context("parse failed in dedup")?;
    let root = tree.root_node();

    // Collect positions from the plain re-export query (@name = index 0).
    let query = Query::new(language, NAMED_REEXPORT_QUERY)
        .context("failed to compile NAMED_REEXPORT_QUERY for dedup")?;
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @name — the local name node
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    positions.insert((line, col));
                }
            }
        }
    }

    // Also collect local-name positions from the alias re-export query (@local_name = index 0).
    if let Ok(alias_query) = Query::new(language, NAMED_REEXPORT_ALIAS_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&alias_query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @local_name
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    positions.insert((line, col));
                }
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

    #[test]
    fn test_parse_class_members_getter() {
        let src = "class A { get foo(): number { return 1; } }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let getter = members.iter().find(|m| m.name == "foo").unwrap();
        assert_eq!(getter.kind, ClassMemberKind::Getter, "getter should have kind Getter");
    }

    #[test]
    fn test_parse_class_members_setter() {
        let src = "class A { set foo(v: number) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let setter = members.iter().find(|m| m.name == "foo").unwrap();
        assert_eq!(setter.kind, ClassMemberKind::Setter, "setter should have kind Setter");
    }

    #[test]
    fn test_parse_class_members_private_field_excluded() {
        let src = "class A { #secret: string; visible: string; }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        assert_eq!(members.len(), 1, "only 'visible' should be present, not '#secret'");
        assert_eq!(members[0].name, "visible");
    }

    #[test]
    fn test_parse_class_members_getter_setter_pair() {
        let src = "class A { get x(): number { return 0; } set x(v: number) {} }";
        let (tree, bytes) = parse_ts(src);
        let class_node = first_node_of_kind(tree.root_node(), "class_declaration").unwrap();
        let members = parse_class_members(&bytes, class_node);
        let getter = members.iter().find(|m| m.name == "x" && m.kind == ClassMemberKind::Getter);
        let setter = members.iter().find(|m| m.name == "x" && m.kind == ClassMemberKind::Setter);
        assert!(getter.is_some(), "should have a Getter for x");
        assert!(setter.is_some(), "should have a Setter for x");
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

    // ── Export alias and namespace re-export tests (Phase 11 bug fixes) ────────

    #[test]
    fn test_export_alias_uses_alias_name() {
        // `export { foo as bar }` — public API name is "bar", not "foo".
        let source = b"const foo = 1;\nexport { foo as bar };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "bar"),
            "exported alias name 'bar' should appear in exports; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(
            !exports.iter().any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "local name 'foo' should NOT appear as a Named export; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_export_no_alias_uses_local_name() {
        // Regression guard: `export { foo }` with no alias should still use "foo".
        let source = b"const foo = 1;\nexport { foo };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "non-aliased export should use local name 'foo'; got: {:?}",
            exports.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_reexport_alias_uses_alias_name() {
        // `export { foo as bar } from './mod'` — public re-export name is "bar".
        let source = b"export { foo as bar } from './mod';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "bar" && e.kind == RawExportKind::NamedReexport),
            "aliased re-export 'bar' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            !exports
                .iter()
                .any(|e| e.name == "foo" && e.kind == RawExportKind::NamedReexport),
            "local name 'foo' should NOT appear as a NamedReexport; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_namespace_reexport_captures_name() {
        // `export * as utils from './mod'` — exported name is "utils", not "*".
        let source = b"export * as utils from './mod';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "utils" && e.kind == RawExportKind::StarReexport),
            "namespace re-export should use name 'utils'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            !exports
                .iter()
                .any(|e| e.name == "*" && e.source.as_deref() == Some("./mod")),
            "name '*' should not appear for a named namespace re-export; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_plain_star_reexport_still_works() {
        // Regression guard: `export * from './mod'` should still produce name="*".
        let source = b"export * from './mod';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "*" && e.kind == RawExportKind::StarReexport),
            "plain star re-export should still produce name='*'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    // ── Type-only export tests (Task 1 — Phase 12) ────────────────────────────

    #[test]
    fn test_type_only_named_export() {
        // `export type { Foo }` — Foo is a Named export with no source
        let source = b"export type { Foo };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "Foo" && e.kind == RawExportKind::Named),
            "export type {{ Foo }} should produce Named export 'Foo'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        let foo = exports.iter().find(|e| e.name == "Foo").unwrap();
        assert!(foo.decl_kind.is_none(), "type-only export should have no decl_kind");
    }

    #[test]
    fn test_type_only_reexport() {
        // `export type { Foo } from './bar'` — NamedReexport with source
        let source = b"export type { Foo } from './bar';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "Foo" && e.kind == RawExportKind::NamedReexport),
            "export type {{ Foo }} from './bar' should produce NamedReexport 'Foo'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        let foo = exports
            .iter()
            .find(|e| e.name == "Foo" && e.kind == RawExportKind::NamedReexport)
            .unwrap();
        assert_eq!(
            foo.source.as_deref(),
            Some("./bar"),
            "source should be './bar'"
        );
    }

    #[test]
    fn test_type_only_alias_export() {
        // `export type { Foo as Bar }` — exported public name is "Bar"
        let source = b"export type { Foo as Bar };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "Bar" && e.kind == RawExportKind::Named),
            "export type {{ Foo as Bar }} should produce Named export 'Bar'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            !exports.iter().any(|e| e.name == "Foo" && e.kind == RawExportKind::Named),
            "local name 'Foo' should NOT appear as Named; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_destructured_export() {
        // `export const { foo, bar } = obj` — two Named exports
        let source = b"export const { foo, bar } = obj;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "destructured export 'foo' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            exports.iter().any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
            "destructured export 'bar' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        // Both should have Variable decl_kind
        let foo = exports
            .iter()
            .find(|e| e.name == "foo" && e.kind == RawExportKind::Named)
            .unwrap();
        assert_eq!(
            foo.decl_kind,
            Some(crate::types::DeclarationKind::Variable),
            "destructured export should have Variable decl_kind"
        );
    }

    #[test]
    fn test_destructured_pair_export() {
        // `export const { foo: renamed } = obj` — public name is "renamed" (the value/local binding)
        let source = b"export const { foo: renamed } = obj;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "renamed" && e.kind == RawExportKind::Named),
            "pair-pattern destructured export should export 'renamed' (local binding); got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            !exports.iter().any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "source property key 'foo' should NOT appear as Named export; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    // ── BUG-13-02: Namespace/star dedup fix ──────────────────────────────────

    #[test]
    fn test_namespace_and_star_from_same_source() {
        let source = b"export * from './mod';\nexport * as utils from './mod';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let star = exports
            .iter()
            .filter(|e| e.name == "*" && e.source.as_deref() == Some("./mod"))
            .count();
        let ns = exports
            .iter()
            .filter(|e| e.name == "utils" && e.source.as_deref() == Some("./mod"))
            .count();
        assert_eq!(star, 1, "plain star should appear once; got {star}");
        assert_eq!(ns, 1, "namespace 'utils' should appear once; got {ns}");
    }

    // ── GAP-13-03: Inline type modifier exports ───────────────────────────────

    #[test]
    fn test_inline_type_modifier_exports() {
        // TS 4.5: per-specifier `type` prefix — `export { type Foo, Bar }`
        // Both should appear exactly once as Named exports
        let source = b"export { type Foo, Bar };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let foo_count = exports.iter().filter(|e| e.name == "Foo").count();
        let bar_count = exports.iter().filter(|e| e.name == "Bar").count();
        assert_eq!(foo_count, 1, "Foo should appear exactly once; got {foo_count}");
        assert_eq!(bar_count, 1, "Bar should appear exactly once; got {bar_count}");
    }

    // ── Export assignment and ambient declaration tests (Task 2 — Phase 12) ───

    #[test]
    fn test_export_assignment() {
        // `export = MyClass;` — CJS-style TS default export, becomes name="default"
        let source = b"export = MyClass;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "default" && e.kind == RawExportKind::Default),
            "export = MyClass should produce Default export 'default'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_ambient_function_declaration() {
        // `export declare function foo(): void;` — ambient declaration in .d.ts
        let source = b"export declare function foo(): void;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let foo = exports
            .iter()
            .find(|e| e.name == "foo" && e.kind == RawExportKind::Named);
        assert!(
            foo.is_some(),
            "export declare function foo should produce Named export 'foo'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert_eq!(
            foo.unwrap().decl_kind,
            Some(crate::types::DeclarationKind::Function),
            "ambient function should have Function decl_kind"
        );
    }

    #[test]
    fn test_ambient_class_declaration() {
        // `export declare class Bar {}` — ambient class declaration
        let source = b"export declare class Bar {}";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let bar = exports
            .iter()
            .find(|e| e.name == "Bar" && e.kind == RawExportKind::Named);
        assert!(
            bar.is_some(),
            "export declare class Bar should produce Named export 'Bar'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert_eq!(
            bar.unwrap().decl_kind,
            Some(crate::types::DeclarationKind::Class),
            "ambient class should have Class decl_kind"
        );
    }

    #[test]
    fn test_ambient_interface_declaration() {
        // `export declare interface Baz { x: number }` — ambient interface
        let source = b"export declare interface Baz { x: number }";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let baz = exports
            .iter()
            .find(|e| e.name == "Baz" && e.kind == RawExportKind::Named);
        assert!(
            baz.is_some(),
            "export declare interface Baz should produce Named export 'Baz'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert_eq!(
            baz.unwrap().decl_kind,
            Some(crate::types::DeclarationKind::Interface),
            "ambient interface should have Interface decl_kind"
        );
    }

    #[test]
    fn test_ambient_variable_declaration() {
        // `export declare const VERSION: string;` — ambient variable
        let source = b"export declare const VERSION: string;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        let version = exports
            .iter()
            .find(|e| e.name == "VERSION" && e.kind == RawExportKind::Named);
        assert!(
            version.is_some(),
            "export declare const VERSION should produce Named export 'VERSION'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert_eq!(
            version.unwrap().decl_kind,
            Some(crate::types::DeclarationKind::Variable),
            "ambient variable should have Variable decl_kind"
        );
    }

    // ── GAP-13-01: CJS module.exports detection ───────────────────────────────

    #[test]
    fn test_cjs_module_exports_shorthand() {
        let source = b"module.exports = { foo, bar };";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "CJS shorthand export 'foo' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            exports.iter().any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
            "CJS shorthand export 'bar' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_module_exports_pair() {
        let source = b"module.exports = { foo: myFoo, bar: myBar };";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "CJS pair export 'foo' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            exports.iter().any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
            "CJS pair export 'bar' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_exports_property() {
        let source = b"exports.foo = function() {};\nexports.bar = 42;";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "foo" && e.kind == RawExportKind::Named),
            "CJS exports.foo should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            exports.iter().any(|e| e.name == "bar" && e.kind == RawExportKind::Named),
            "CJS exports.bar should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_module_exports_default() {
        let source = b"module.exports = MyClass;";
        let exports = parse_exports(source, SourceLanguage::JavaScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "default" && e.kind == RawExportKind::Default),
            "CJS module.exports = MyClass should produce Default 'default'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_not_triggered_in_typescript() {
        // CJS patterns should NOT be detected in TypeScript files
        let source = b"module.exports = { foo };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            !exports.iter().any(|e| e.name == "foo"),
            "CJS should not match in TS; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    // ── GAP-13-02: export { default as Named } detection ─────────────────────

    #[test]
    fn test_export_default_as_named() {
        let source = b"export { default as Named };";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "Named" && e.kind == RawExportKind::Named),
            "export {{ default as Named }} should export 'Named'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_export_default_reexport() {
        let source = b"export { default } from './foo';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "default" && e.kind == RawExportKind::NamedReexport),
            "export {{ default }} from './foo' should produce NamedReexport 'default'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_export_default_as_named_reexport() {
        let source = b"export { default as Named } from './foo';";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "Named" && e.kind == RawExportKind::NamedReexport),
            "export {{ default as Named }} from './foo' should export 'Named'; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    // ── GAP-13-04: Array destructuring exports ────────────────────────────────

    #[test]
    fn test_array_destructured_export() {
        let source = b"export const [a, b] = arr;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "a" && e.kind == RawExportKind::Named),
            "array destructured export 'a' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            exports.iter().any(|e| e.name == "b" && e.kind == RawExportKind::Named),
            "array destructured export 'b' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_array_destructured_rest_export() {
        let source = b"export const [first, ...rest] = arr;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports
                .iter()
                .any(|e| e.name == "first" && e.kind == RawExportKind::Named),
            "array destructured 'first' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            exports.iter().any(|e| e.name == "rest" && e.kind == RawExportKind::Named),
            "array destructured rest 'rest' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_array_destructured_with_holes() {
        let source = b"export const [a, , c] = arr;";
        let exports = parse_exports(source, SourceLanguage::TypeScript).unwrap();
        assert!(
            exports.iter().any(|e| e.name == "a" && e.kind == RawExportKind::Named),
            "array destructured 'a' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert!(
            exports.iter().any(|e| e.name == "c" && e.kind == RawExportKind::Named),
            "array destructured 'c' should appear; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
        assert_eq!(
            exports
                .iter()
                .filter(|e| e.kind == RawExportKind::Named)
                .count(),
            2,
            "holes should not produce Named exports; got: {:?}",
            exports.iter().map(|e| (&e.name, &e.kind)).collect::<Vec<_>>()
        );
    }
}
