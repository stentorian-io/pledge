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

    /// INF-05 (Plan 02): return ordered directive records for triple-slash
    /// comments at the top of the file.
    ///
    /// "Top of file" honors the TypeScript compiler position rule: a
    /// `/// <X ... />` comment is a directive ONLY if all preceding
    /// top-level children of the `program` node are comments. The moment a
    /// non-comment statement appears at program level, ALL subsequent
    /// `///` comments are treated as plain comments and produce no
    /// directive records.
    pub fn extract_directives(&self) -> Vec<DirectiveRecord> {
        let root = self.tree.root_node();
        let mut directives: Vec<DirectiveRecord> = Vec::new();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() == "comment" {
                let text = match child.utf8_text(self.source) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let trimmed = text.trim_start();
                if !trimmed.starts_with("///") {
                    // Plain comment (`// ...` or `/* ... */`) — does not break
                    // the directive window, just skip.
                    continue;
                }
                // Strip the leading /// and any whitespace after it.
                let body = trimmed[3..].trim_start();
                if !body.starts_with('<') {
                    // `/// some free-form comment` — not a directive.
                    continue;
                }
                if let Some(record) = parse_directive_body(body, child.start_byte()) {
                    directives.push(record);
                }
                // Continue scanning: more /// directives may follow before
                // the first non-comment statement.
                continue;
            }
            // First non-comment named child ends the directive window.
            break;
        }
        directives
    }
}

/// Parse the body of a triple-slash directive — the text inside a `///`
/// comment AFTER the leading `///` has been stripped.
///
/// Example body: `<reference path="./x.ts" preserve="true" />`
///
/// Returns `None` if the body does not look like `<name attr="v" .../>`.
/// Malformed attributes (e.g., unterminated quote) cause the directive to
/// be silently skipped — this matches the TypeScript compiler's permissive
/// handling and prevents a single malformed comment from producing a
/// misleading finding.
fn parse_directive_body(body: &str, start_byte: usize) -> Option<DirectiveRecord> {
    let body = body.trim();
    if !body.starts_with('<') {
        return None;
    }
    let inner = body.strip_prefix('<')?;
    let end = inner.find('>')?;
    // Content between < and > is "name attr=\"v\" attr=\"v\" /" — strip the
    // trailing '/' that precedes '>' in self-closing form, then trim.
    let content = inner[..end].trim_end_matches('/').trim();
    // Split on first whitespace to separate name from attrs.
    let (name, attrs_str) = match content.find(char::is_whitespace) {
        Some(i) => (content[..i].to_string(), content[i..].trim()),
        None => (content.to_string(), ""),
    };
    // Validate the name looks like a directive (letters/digits/hyphen).
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let mut attrs: BTreeMap<String, String> = BTreeMap::new();
    // Iterate `key="value"` pairs.
    let mut remaining = attrs_str;
    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }
        // Read key: letters/digits/hyphen until '=' or whitespace.
        let key_end = remaining
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .unwrap_or(remaining.len());
        if key_end == 0 {
            break;
        }
        let key = remaining[..key_end].to_string();
        remaining = remaining[key_end..].trim_start();
        if !remaining.starts_with('=') {
            break;
        }
        remaining = remaining[1..].trim_start();
        if !remaining.starts_with('"') {
            break;
        }
        remaining = &remaining[1..];
        let val_end = remaining.find('"')?;
        let value = remaining[..val_end].to_string();
        remaining = &remaining[val_end + 1..];
        attrs.insert(key, value);
    }
    Some(DirectiveRecord {
        name,
        attrs,
        start_byte,
    })
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

    fn make_index<'a>(source: &'a [u8], tree: &'a Tree) -> CommentIndex<'a> {
        CommentIndex::build(tree, source, SourceLanguage::TypeScript)
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
    fn inf_05_extract_directives_returns_directive_for_top_of_file_form() {
        let source = b"/// <reference path=\"./x.ts\" />\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        let dirs = idx.extract_directives();
        assert_eq!(dirs.len(), 1, "expected 1 directive; got {:?}", dirs);
        assert_eq!(dirs[0].name, "reference");
        assert_eq!(dirs[0].attrs.get("path").map(|s| s.as_str()), Some("./x.ts"));
    }

    #[test]
    fn inf_05_directive_after_statement_is_ignored() {
        let source = b"export const x = 1;\n/// <reference path=\"./x.ts\" />\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        assert_eq!(idx.extract_directives().len(), 0);
    }

    #[test]
    fn inf_05_multiple_top_of_file_directives_ordered_by_byte() {
        let source = b"/// <reference types=\"node\" />\n/// <reference lib=\"es2017\" />\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        let dirs = idx.extract_directives();
        assert_eq!(dirs.len(), 2);
        assert!(dirs[0].start_byte < dirs[1].start_byte);
    }

    #[test]
    fn inf_05_preserve_attribute_captured() {
        let source = b"/// <reference path=\"./x.ts\" preserve=\"true\" />\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        let dirs = idx.extract_directives();
        assert_eq!(dirs.len(), 1);
        assert_eq!(
            dirs[0].attrs.get("preserve").map(|s| s.as_str()),
            Some("true")
        );
        assert_eq!(dirs[0].attrs.get("path").map(|s| s.as_str()), Some("./x.ts"));
    }

    #[test]
    fn inf_05_attribute_order_independent_equality() {
        let src_a = b"/// <amd-dependency path=\"a\" name=\"b\" />\n";
        let src_b = b"/// <amd-dependency name=\"b\" path=\"a\" />\n";
        let tree_a = parse_ts(src_a);
        let tree_b = parse_ts(src_b);
        let a = make_index(src_a, &tree_a).extract_directives();
        let b = make_index(src_b, &tree_b).extract_directives();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        // BTreeMap equality is order-independent.
        assert_eq!(a[0].attrs, b[0].attrs);
        assert_eq!(a[0].name, b[0].name);
    }

    #[test]
    fn inf_05_leading_plain_comments_do_not_block_directives() {
        // Plain comments before directives are OK. Only non-comment statements block.
        let source = b"// preamble\n/* block */\n/// <reference path=\"./x.ts\" />\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        assert_eq!(idx.extract_directives().len(), 1);
    }

    #[test]
    fn comment_index_collects_all_comments() {
        // Smoke test: verify the (comment) @c query captures both /** */ and //.
        let source = b"// line\n/** block */\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = CommentIndex::build(&tree, source, SourceLanguage::TypeScript);
        assert_eq!(idx.comments_len(), 2);
    }

    // IN-02 (Phase 19 re-review): lock in the "silently drop malformed
    // directive" contract. `parse_directive_body` is deliberately permissive
    // — it mirrors the TypeScript compiler's behavior of dropping any
    // directive whose body fails to parse cleanly, rather than emitting a
    // partial record or a noisy warning. These tests fail-fast if a future
    // refactor flips the contract to panic, warn, or produce a partial parse.

    #[test]
    fn inf_05_malformed_directive_unterminated_quote_silently_skipped() {
        // Opening quote on `path=` is never closed, so the body is malformed.
        // Expected: zero directives extracted, no panic.
        let source = b"/// <reference path=\"unterminated />\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        assert_eq!(
            idx.extract_directives().len(),
            0,
            "malformed directive (unterminated quote) must be silently dropped"
        );
    }

    #[test]
    fn inf_05_malformed_directive_missing_gt_silently_skipped() {
        // No closing `>` on the directive body — entire directive is dropped.
        let source = b"/// <reference path=\"./x.ts\" /\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        assert_eq!(
            idx.extract_directives().len(),
            0,
            "malformed directive (missing '>') must be silently dropped"
        );
    }

    #[test]
    fn inf_05_partial_parse_drops_whole_directive() {
        // `types="node"` parses fine, but the next attribute has no closing
        // quote. Current contract: the entire directive returns None; no
        // partial record with only the first attribute is emitted.
        let source = b"/// <reference types=\"node\" path=\"missing />\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        assert_eq!(
            idx.extract_directives().len(),
            0,
            "partial-parse failure (mid-body unterminated quote) must drop the entire directive, not emit a partial record"
        );
    }

    #[test]
    fn inf_05_non_identifier_name_silently_skipped() {
        // Directive name contains a character outside [A-Za-z0-9-]: rejected.
        let source = b"/// <ref.bad path=\"./x.ts\" />\nexport const x = 1;\n";
        let tree = parse_ts(source);
        let idx = make_index(source, &tree);
        assert_eq!(
            idx.extract_directives().len(),
            0,
            "directive with non-identifier name must be silently dropped"
        );
    }
}
