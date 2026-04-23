//! Phase 19: Supplementary comment-scan sidecar.
//!
//! Tree-sitter cannot see comments as AST nodes in the way consumers need.
//! This module builds a `CommentIndex` sidecar once per file parse, keyed
//! by byte offset. Three consumers subscribe to it:
//!
//!  1. META-06 release-tag post-process populate at resolver insertion
//!     (`release_tag_for_export_at`, used by `collect_exports`).
//!  2. INF-05 triple-slash directive extraction (`extract_directives`,
//!     body filled by Plan 02).
//!  3. CLS-10 decorator comment-fallback — NOT IN PHASE 19 SCOPE.
//!     Decorators are AST nodes in both grammars per RESEARCH §5 and
//!     CONTEXT.md post-research decisions. Plan 03 reads them via AST
//!     directly without consulting CommentIndex.

#![allow(dead_code)]

use std::collections::BTreeMap;

use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

use super::tree_query::{get_language, SourceLanguage};
use crate::types::ReleaseTag;

const COMMENT_QUERY: &str = r#"(comment) @c"#;

/// Internal metadata for a single comment captured from the tree.
#[derive(Debug, Clone)]
struct CommentMetadata {
    start_byte: usize,
    end_byte: usize,
    start_row: u32, // 1-indexed per project convention
    start_col: u32, // 1-indexed
}

/// INF-05 record (type shipped in Plan 01; body populated by Plan 02).
///
/// Represents a single triple-slash directive (e.g.,
/// `/// <reference path="./x.ts" />`). The attribute map is a BTreeMap so
/// attribute ordering is not significant for equality (per post-research
/// decision D-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveRecord {
    /// e.g., "reference", "amd-module", "amd-dependency"
    pub name: String,
    /// Ordered attribute map. BTreeMap for order-independent equality.
    pub attrs: BTreeMap<String, String>,
    /// Byte offset in source — used for ordering directives within a file
    /// (per post-research decision D-04: directive-reordered at Minor).
    pub start_byte: usize,
}

/// Phase 19 sidecar: enumerates every `(comment)` AST node in the parsed
/// tree and indexes them for location-based lookup. Built once per file.
pub struct CommentIndex<'a> {
    /// Borrowed reference to the tree so we can walk ancestors at lookup time.
    tree: &'a Tree,
    source: &'a [u8],
    /// Comments in document order.
    comments: Vec<CommentMetadata>,
}

impl<'a> CommentIndex<'a> {
    /// Build by running `(comment) @c` over an already-parsed tree.
    /// Returns an empty index if the query fails to compile (should not
    /// happen with canonical grammars; see RESEARCH §3 verification).
    pub fn build(tree: &'a Tree, source: &'a [u8], lang: SourceLanguage) -> Self {
        let language = get_language(&lang);
        let query = match Query::new(&language, COMMENT_QUERY) {
            Ok(q) => q,
            Err(_) => {
                return CommentIndex {
                    tree,
                    source,
                    comments: Vec::new(),
                };
            }
        };
        let mut cursor = QueryCursor::new();
        let root = tree.root_node();
        let mut matches = cursor.matches(&query, root, source);
        let mut comments = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let node = capture.node;
                comments.push(CommentMetadata {
                    start_byte: node.start_byte(),
                    end_byte: node.end_byte(),
                    start_row: node.start_position().row as u32 + 1,
                    start_col: node.start_position().column as u32 + 1,
                });
            }
        }
        CommentIndex {
            tree,
            source,
            comments,
        }
    }

    /// Read-only access to the list of collected comments (for Plan 02).
    pub(crate) fn comments_len(&self) -> usize {
        self.comments.len()
    }

    /// META-06 lookup: given the (1-indexed) `line` and `col` of a RawExport
    /// anchor node, find the preceding block-comment TSDoc release tag.
    ///
    /// Walks the AST from (line, col) to the nearest `export_statement` /
    /// `ambient_declaration` ancestor (matching `extract_release_tag`'s
    /// `find_statement_ancestor` behavior) and inspects its prev_named_sibling.
    /// Returns None if the sibling is not a `/** */` block comment.
    pub fn release_tag_for_export_at(&self, line: u32, col: u32) -> Option<ReleaseTag> {
        if line == 0 {
            return None;
        }
        let row = (line - 1) as usize;
        let column = col.saturating_sub(1) as usize;
        let point = tree_sitter::Point { row, column };
        let root = self.tree.root_node();
        let anchor = root.descendant_for_point_range(point, point)?;
        let statement = walk_to_statement_ancestor(anchor);
        let prev = statement.prev_named_sibling()?;
        if prev.kind() != "comment" {
            return None;
        }
        let text = prev.utf8_text(self.source).ok()?;
        if !text.starts_with("/**") {
            return None;
        }
        // Priority: @internal > @alpha > @beta > @deprecated > @public.
        // Mirrors tree_query.rs::extract_release_tag.
        if text.contains("@internal") {
            return Some(ReleaseTag::Internal);
        }
        if text.contains("@alpha") {
            return Some(ReleaseTag::Alpha);
        }
        if text.contains("@beta") {
            return Some(ReleaseTag::Beta);
        }
        if text.contains("@deprecated") {
            return Some(ReleaseTag::Deprecated);
        }
        if text.contains("@public") {
            return Some(ReleaseTag::Public);
        }
        None
    }

    /// INF-05 (consumed by Plan 02): return ordered directive records
    /// for triple-slash comments at the top of the file. Stub in Plan 01;
    /// Plan 02 fills in the body.
    pub fn extract_directives(&self) -> Vec<DirectiveRecord> {
        Vec::new()
    }
}

/// Walk up the tree to the nearest `export_statement` / `ambient_declaration`
/// ancestor. Mirrors `tree_query.rs::find_statement_ancestor` (which is
/// private to that module — duplicated here rather than extending its API).
fn walk_to_statement_ancestor(mut node: Node<'_>) -> Node<'_> {
    loop {
        let parent = match node.parent() {
            Some(p) => p,
            None => return node,
        };
        let parent_kind = parent.kind();
        if parent_kind == "program" {
            return node;
        }
        if parent_kind == "export_statement" || parent_kind == "ambient_declaration" {
            return parent;
        }
        node = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_ts(source: &[u8]) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&get_language(&SourceLanguage::TypeScript))
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn meta_06_beta_tag_looked_up_by_location() {
        let source = b"/** @beta */\nexport function previewFoo() {}\n";
        let tree = parse_ts(source);
        let idx = CommentIndex::build(&tree, source, SourceLanguage::TypeScript);
        // `export_statement` starts at line 2, col 1 (1-indexed).
        assert_eq!(
            idx.release_tag_for_export_at(2, 1),
            Some(ReleaseTag::Beta),
            "META-06: @beta lookup by (line, col) must return Beta"
        );
    }

    #[test]
    fn meta_06_internal_tag_looked_up_by_location() {
        let source = b"/** @internal */\nexport function priv() {}\n";
        let tree = parse_ts(source);
        let idx = CommentIndex::build(&tree, source, SourceLanguage::TypeScript);
        assert_eq!(
            idx.release_tag_for_export_at(2, 1),
            Some(ReleaseTag::Internal)
        );
    }

    #[test]
    fn meta_06_deprecated_tag_looked_up_by_location() {
        let source = b"/** @deprecated use newFoo */\nexport function oldFoo() {}\n";
        let tree = parse_ts(source);
        let idx = CommentIndex::build(&tree, source, SourceLanguage::TypeScript);
        assert_eq!(
            idx.release_tag_for_export_at(2, 1),
            Some(ReleaseTag::Deprecated)
        );
    }

    #[test]
    fn meta_06_no_preceding_comment_returns_none() {
        let source = b"export function plain() {}\n";
        let tree = parse_ts(source);
        let idx = CommentIndex::build(&tree, source, SourceLanguage::TypeScript);
        assert_eq!(idx.release_tag_for_export_at(1, 1), None);
    }

    #[test]
    fn meta_06_line_comment_form_ignored() {
        // TSDoc requires /** — // @deprecated is explicitly NOT a TSDoc tag.
        let source = b"// @deprecated\nexport function foo() {}\n";
        let tree = parse_ts(source);
        let idx = CommentIndex::build(&tree, source, SourceLanguage::TypeScript);
        assert_eq!(idx.release_tag_for_export_at(2, 1), None);
    }

    #[test]
    fn inf_05_extract_directives_stub_returns_empty() {
        // Plan 01 stub body returns empty Vec; Plan 02 fills in.
        let source = b"/// <reference path=\"./x.ts\" />\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = CommentIndex::build(&tree, source, SourceLanguage::TypeScript);
        assert_eq!(idx.extract_directives().len(), 0);
    }

    #[test]
    fn comment_index_collects_all_comments() {
        // Smoke test: verify the (comment) @c query captures both /** */ and //.
        let source = b"// line\n/** block */\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = CommentIndex::build(&tree, source, SourceLanguage::TypeScript);
        assert_eq!(idx.comments_len(), 2);
    }
}
