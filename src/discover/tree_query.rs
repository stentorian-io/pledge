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

/// Captures `export default <expr>` or `export default function/class`.
const DEFAULT_EXPORT_QUERY: &str = r#"
(export_statement
  "default"
  value: (_) @value)
"#;

/// Captures `export const/let/var foo`, `export function foo`, `export class Foo`.
const DECL_EXPORT_QUERY: &str = r#"
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

/// Captures `export * from './module'`.
const STAR_EXPORT_QUERY: &str = r#"
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

    // --- Default exports ------------------------------------------------------
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

    // --- Declaration exports --------------------------------------------------
    {
        let query = Query::new(&language, DECL_EXPORT_QUERY)
            .context("failed to compile DECL_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @name
                    let node = capture.node;
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

    // --- Star re-exports ------------------------------------------------------
    {
        let query = Query::new(&language, STAR_EXPORT_QUERY)
            .context("failed to compile STAR_EXPORT_QUERY")?;
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
                    exports.push(RawExport {
                        name: "*".to_string(),
                        kind: RawExportKind::StarReexport,
                        line: node.start_position().row as u32 + 1,
                        col: node.start_position().column as u32 + 1,
                        source: Some(module_spec.to_string()),
                    });
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
    let tree = parser.parse(source, None).context("parse failed in dedup")?;
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
