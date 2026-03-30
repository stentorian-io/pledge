//! Hover request batching and type string extraction from LSP hover responses.
//!
//! Per decisions D-05, D-06, D-23.

use std::path::Path;
use std::time::Duration;

use async_lsp::lsp_types::{
    DidOpenTextDocumentParams, HoverContents, HoverParams, MarkedString, MarkupKind,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Url,
    WorkDoneProgressParams,
};
use async_lsp::LanguageServer;

use crate::types::parser::parse_type_string;
use crate::types::shape::TypeShape;

use super::client::LspSession;
use super::LspError;

/// Per-request hover timeout (D-06).
const HOVER_TIMEOUT_SECS: u64 = 5;

/// Input for a hover request for a single symbol.
pub struct SymbolHoverInput {
    pub name: String,
    pub line: u32,
    pub col: u32,
}

/// Extract the TypeScript type string from an LSP Hover response.
///
/// The hover markdown contains a fenced code block:
/// ` ```typescript\n<type>\n``` `
///
/// Returns None if no valid type string can be extracted.
pub fn extract_type_string(hover: &async_lsp::lsp_types::Hover) -> Option<String> {
    match &hover.contents {
        HoverContents::Markup(mc) if mc.kind == MarkupKind::Markdown => {
            extract_from_markdown(&mc.value)
        }
        HoverContents::Scalar(MarkedString::LanguageString(ls)) => {
            // MarkedString::LanguageString carries the raw code directly (no markdown wrapping)
            let trimmed = ls.value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        HoverContents::Scalar(MarkedString::String(s)) => {
            // Plain string — try to extract a code block if present, else return as-is
            if s.contains("```") {
                extract_from_markdown(s)
            } else {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
        }
        _ => None,
    }
}

/// Extract the content of the first TypeScript fenced code block from a markdown string.
fn extract_from_markdown(value: &str) -> Option<String> {
    let mut in_block = false;
    let mut lines: Vec<&str> = Vec::new();

    for line in value.lines() {
        if line.starts_with("```typescript") || line.starts_with("```ts") {
            in_block = true;
            continue;
        }
        if in_block && line.starts_with("```") {
            break;
        }
        if in_block {
            lines.push(line);
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Send a hover request for a single export symbol.
///
/// Returns the raw type string if resolved, or None on timeout / no hover.
/// Implements per-request timeout of 5 seconds (D-06).
///
/// LSP positions are 0-indexed; `line` and `col` parameters are 1-indexed
/// (as stored in ExportSymbol) and are adjusted here.
pub async fn hover_symbol(
    session: &LspSession,
    file_uri: Url,
    line: u32,
    col: u32,
) -> Result<Option<String>, LspError> {
    // ExportSymbol stores 1-indexed positions; LSP uses 0-indexed
    let lsp_line = line.saturating_sub(1);
    let lsp_col = col.saturating_sub(1);

    let hover_fut = session.server.clone().hover(HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: file_uri },
            position: async_lsp::lsp_types::Position::new(lsp_line, lsp_col),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    });

    match tokio::time::timeout(Duration::from_secs(HOVER_TIMEOUT_SECS), hover_fut).await {
        Err(_elapsed) => {
            // Timeout — symbol simply won't have type info (D-18: not an error)
            Ok(None)
        }
        Ok(Err(e)) => {
            tracing::debug!("hover request error: {e}");
            Ok(None)
        }
        Ok(Ok(None)) => Ok(None),
        Ok(Ok(Some(hover))) => Ok(extract_type_string(&hover)),
    }
}

/// Open a file and send hover requests for all its symbols.
///
/// Per D-23: sends `textDocument/didOpen` first, then hover requests sequentially.
/// Implements retry logic for the first hover (Pitfall 6: tsserver indexing delay).
///
/// Returns a list of (symbol_name, type_signature, type_shape) for all resolved symbols.
pub async fn hover_file_symbols(
    session: &LspSession,
    file_path: &Path,
    inputs: &[SymbolHoverInput],
) -> Result<Vec<(String, Option<String>, Option<TypeShape>)>, LspError> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    // Determine language ID from file extension
    let lang_id = match file_path.extension().and_then(|e| e.to_str()) {
        Some("ts") | Some("tsx") | Some("d.ts") => "typescript",
        Some("js") | Some("mjs") | Some("cjs") | Some("jsx") => "javascript",
        _ => "typescript", // default
    };

    // Read file content from disk
    let content = match tokio::fs::read_to_string(file_path).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("cannot read {}: {e}", file_path.display());
            return Ok(Vec::new());
        }
    };

    let file_uri = Url::from_file_path(file_path).map_err(|_| {
        LspError::RequestFailed(format!("invalid file path: {}", file_path.display()))
    })?;

    // Send textDocument/didOpen (Pitfall: hover without prior didOpen returns null)
    session
        .server
        .clone()
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: file_uri.clone(),
                language_id: lang_id.into(),
                version: 0,
                text: content,
            },
        })
        .map_err(|e| LspError::RequestFailed(format!("didOpen failed: {e}")))?;

    let mut results = Vec::with_capacity(inputs.len());
    let mut is_first = true;

    for input in inputs {
        let type_sig =
            hover_with_retry(session, file_uri.clone(), input.line, input.col, is_first).await;
        is_first = false;

        tracing::debug!(
            "hover: {} (line={} col={}) => sig={:?}",
            input.name,
            input.line,
            input.col,
            type_sig
        );

        let type_shape = type_sig.as_deref().map(parse_type_string);

        results.push((input.name.clone(), type_sig, type_shape));
    }

    Ok(results)
}

/// Send a hover request with retry logic for the first request in a session.
///
/// Pitfall 6: tsserver needs time to index after initialization.
/// The first hover may return None while indexing; retry up to 3 times with 500ms delay.
async fn hover_with_retry(
    session: &LspSession,
    file_uri: Url,
    line: u32,
    col: u32,
    with_retry: bool,
) -> Option<String> {
    if !with_retry {
        return hover_symbol(session, file_uri, line, col)
            .await
            .ok()
            .flatten();
    }

    // First hover — retry up to 3 times if None is returned
    const MAX_RETRIES: u32 = 3;
    for attempt in 0..MAX_RETRIES {
        match hover_symbol(session, file_uri.clone(), line, col).await {
            Ok(Some(sig)) => return Some(sig),
            Ok(None) => {
                if attempt < MAX_RETRIES - 1 {
                    tracing::debug!(
                        "First hover returned None (attempt {}); waiting 500ms for tsserver indexing",
                        attempt + 1
                    );
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
            Err(e) => {
                tracing::debug!("hover error on attempt {}: {e}", attempt + 1);
                break;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

    fn markup_hover(value: &str) -> Hover {
        Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: value.to_string(),
            }),
            range: Some(Range {
                start: Position::new(0, 0),
                end: Position::new(0, 1),
            }),
        }
    }

    fn marked_string_hover(lang: &str, value: &str) -> Hover {
        use async_lsp::lsp_types::LanguageString;
        Hover {
            contents: HoverContents::Scalar(MarkedString::LanguageString(LanguageString {
                language: lang.to_string(),
                value: value.to_string(),
            })),
            range: None,
        }
    }

    #[test]
    fn test_extract_typescript_code_block() {
        let hover = markup_hover("```typescript\nfunction foo(x: string): void\n```\n");
        let result = extract_type_string(&hover);
        assert_eq!(result, Some("function foo(x: string): void".to_string()));
    }

    #[test]
    fn test_extract_ts_language_tag() {
        let hover = markup_hover("```ts\nstring | number\n```\n");
        let result = extract_type_string(&hover);
        assert_eq!(result, Some("string | number".to_string()));
    }

    #[test]
    fn test_extract_marked_string_language_string() {
        let hover = marked_string_hover("typescript", "(x: string) => void");
        let result = extract_type_string(&hover);
        assert_eq!(result, Some("(x: string) => void".to_string()));
    }

    #[test]
    fn test_extract_no_code_block_returns_none() {
        let hover = markup_hover("No code block here, just plain text.");
        let result = extract_type_string(&hover);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_empty_code_block_returns_none() {
        let hover = markup_hover("```typescript\n```\n");
        let result = extract_type_string(&hover);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_only_first_block() {
        // Documentation after `---` separator should be ignored; only first block extracted
        let content = "```typescript\nfunction foo(): string\n```\n\n---\n\nSome docs\n\n```typescript\nsecond block\n```\n";
        let hover = markup_hover(content);
        let result = extract_type_string(&hover);
        assert_eq!(result, Some("function foo(): string".to_string()));
    }

    #[test]
    fn test_extract_multiline_type() {
        let content = "```typescript\n{\n  x: number;\n  y: string;\n}\n```\n";
        let hover = markup_hover(content);
        let result = extract_type_string(&hover);
        assert_eq!(result, Some("{\n  x: number;\n  y: string;\n}".to_string()));
    }
}
