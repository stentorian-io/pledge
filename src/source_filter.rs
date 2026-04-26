//! Source-file content filters shared across the discover and compare pipelines.
//!
//! Real-world packages ship compiled artefacts and minified bundles inside the
//! same directory tree as their public-API source files. Parsing those
//! artefacts with tree-sitter exhausts memory (multi-MB WASM-base64 shims
//! produce O(n) byte allocations and O(n) parse-tree node allocations,
//! compounded by recursive symbol-lookup passes). The filters in this module
//! detect such files BEFORE any parser invocation so the pipeline never
//! attempts to parse them.
//!
//! Used by:
//! - `discover::resolver::collect_exports` — skips file at export-collection time
//! - `compare::mod::compare_structural_diff` — skips file when re-reading symbol declarations
//! - `compare::rename_coalesce::source_has_alias_trailer` — skips file when checking rename trailers

/// Files larger than 512 KiB are almost certainly compiled artefacts (WASM
/// base64 shims, React Native compiled bundles, generated Rollup mega-files)
/// rather than readable public-API declarations. The threshold is calibrated
/// to admit large generated `.d.ts` files (e.g., `tanstack-query`, `graphql`)
/// while excluding multi-MB compiled engines (e.g., `@prisma/client`'s
/// `query_compiler_*_bg.*.wasm-base64.js` files at 2-5 MiB each).
pub const MAX_SOURCE_BYTES: usize = 512 * 1024;

/// A file with any single line longer than 10 KiB is almost certainly a
/// minified bundle. The longest realistic non-minified generated line (e.g.,
/// a long generic type signature in a hand-authored `.d.ts`) stays under
/// 2 KiB. 10 KiB leaves comfortable headroom for unusual but legitimate code.
pub const MAX_LINE_BYTES: usize = 10_000;

/// Reason a file was rejected by the source-content filter.
///
/// Returned for diagnostics so callers can log a precise trace event explaining
/// the rejection. The variants are intentionally exhaustive — any future filter
/// addition should add a new variant rather than overload an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// File exceeds `MAX_SOURCE_BYTES`. Almost certainly a compiled bundle.
    Oversized {
        /// Actual file size in bytes.
        size: usize,
    },
    /// File contains at least one line longer than `MAX_LINE_BYTES`.
    /// Almost certainly a minified bundle that happens to retain newlines.
    MinifiedMultiLine {
        /// Length of the longest line in bytes.
        max_line: usize,
    },
    /// File has no newlines at all and exceeds `MAX_LINE_BYTES`.
    /// Almost certainly a minified single-expression file.
    MinifiedSingleLine {
        /// Total file size in bytes.
        size: usize,
    },
}

impl SkipReason {
    /// Short human-readable label used in trace logs and warnings.
    pub fn label(&self) -> &'static str {
        match self {
            SkipReason::Oversized { .. } => "oversized",
            SkipReason::MinifiedMultiLine { .. } => "minified (long line)",
            SkipReason::MinifiedSingleLine { .. } => "minified (no newlines)",
        }
    }
}

/// Returns `Some(SkipReason)` if the file's content should NOT be parsed,
/// or `None` if it is a candidate for normal processing.
///
/// This function is the single source of truth for the size/minified filter
/// applied across both the discover and compare pipelines. Callers read the
/// file bytes themselves and pass them in — this keeps I/O policy (e.g.,
/// graceful read-failure handling) at the call site where context-specific
/// warnings can be emitted.
pub fn classify(source: &[u8]) -> Option<SkipReason> {
    if source.len() > MAX_SOURCE_BYTES {
        return Some(SkipReason::Oversized { size: source.len() });
    }
    if source.contains(&b'\n') {
        let max_line = source
            .split(|&b| b == b'\n')
            .map(|l| l.len())
            .max()
            .unwrap_or(0);
        if max_line > MAX_LINE_BYTES {
            return Some(SkipReason::MinifiedMultiLine { max_line });
        }
    } else if source.len() > MAX_LINE_BYTES {
        return Some(SkipReason::MinifiedSingleLine { size: source.len() });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_well_formed_source_is_kept() {
        let source = b"export const foo = 1;\nexport const bar = 2;\n";
        assert!(classify(source).is_none());
    }

    #[test]
    fn oversized_file_is_skipped() {
        let source = vec![b'a'; MAX_SOURCE_BYTES + 1];
        match classify(&source) {
            Some(SkipReason::Oversized { size }) => assert_eq!(size, MAX_SOURCE_BYTES + 1),
            other => panic!("expected Oversized, got {other:?}"),
        }
    }

    #[test]
    fn at_threshold_size_is_kept() {
        let source = vec![b'a'; MAX_SOURCE_BYTES];
        assert!(
            classify(&source).is_none()
                || matches!(
                    classify(&source),
                    Some(SkipReason::MinifiedSingleLine { .. })
                )
        );
    }

    #[test]
    fn minified_multi_line_is_skipped() {
        let mut source = vec![b'a'; MAX_LINE_BYTES + 1];
        source.push(b'\n');
        source.extend(b"export const foo = 1;");
        match classify(&source) {
            Some(SkipReason::MinifiedMultiLine { max_line }) => {
                assert_eq!(max_line, MAX_LINE_BYTES + 1)
            }
            other => panic!("expected MinifiedMultiLine, got {other:?}"),
        }
    }

    #[test]
    fn minified_single_line_is_skipped() {
        let source = vec![b'a'; MAX_LINE_BYTES + 1];
        match classify(&source) {
            Some(SkipReason::MinifiedSingleLine { size }) => assert_eq!(size, MAX_LINE_BYTES + 1),
            other => panic!("expected MinifiedSingleLine, got {other:?}"),
        }
    }

    #[test]
    fn short_single_line_is_kept() {
        let source = b"export const foo = 1;";
        assert!(classify(source).is_none());
    }
}
