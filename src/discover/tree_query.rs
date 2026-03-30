#![allow(dead_code)]

use std::path::Path;

use anyhow::Context;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

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
/// `export type Foo`, `export interface Foo`, `export enum Foo`
/// for **TypeScript** files. TypeScript uses `type_identifier` for class/type/interface names.
const DECL_EXPORT_QUERY_TS: &str = r#"
(export_statement
  declaration: [
    (lexical_declaration
      (variable_declarator name: (identifier) @name))
    (function_declaration name: (identifier) @name)
    (class_declaration name: (type_identifier) @name)
    (type_alias_declaration name: (type_identifier) @name)
    (interface_declaration name: (type_identifier) @name)
    (enum_declaration name: (identifier) @name)
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
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::Named,
                        line: node.start_position().row as u32 + 1,
                        col: node.start_position().column as u32 + 1,
                        source: None,
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
                });
            }
        }
    }

    Ok(exports)
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
