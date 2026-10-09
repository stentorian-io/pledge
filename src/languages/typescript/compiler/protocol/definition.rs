//! Resolve cross-file type references through `textDocument/definition`.
//!
//! Requests target the property identifier in `typeof import('./y').Z` and
//! return the first resolved declaration location.

use crate::languages::typescript::compiler::protocol::client::LspSession;
use crate::languages::typescript::compiler::protocol::position;
use crate::languages::typescript::compiler::LspError;
use async_lsp::lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, TextDocumentIdentifier,
    TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use async_lsp::LanguageServer;
use std::time::Duration;

/// 5-second per-request timeout — matches the hover convention.
const DEFINITION_TIMEOUT_SECS: u64 = 5;

/// Resolve a `textDocument/definition` request. Returns the resolved
/// `Location` on success, `Ok(None)` on timeout / no-result, and
/// `Err(LspError::RequestFailed)` on real request errors (server exit,
/// channel closed). The Err path lets the caller's CRASH_THRESHOLD branch
/// fire, mirroring the hover pass.
///
/// Source positions are one-based UTF-8 byte offsets; LSP requests use
/// zero-based UTF-16 positions, including after non-ASCII text.
///
/// `GotoDefinitionResponse` has three variants — Scalar (single Location),
/// Array (Vec<Location>), Link (Vec<LocationLink>).
/// we collapse to a single Location: take the first entry on Array/Link.
#[allow(dead_code)]
pub async fn resolve_definition_location(
    session: &LspSession,
    file_uri: Url,
    line: u32,
    col: u32,
) -> Result<Option<Location>, LspError> {
    let position = position::source_position(&file_uri, line, col).await?;

    let fut = session.server.clone().definition(GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: file_uri },
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: Default::default(),
    });

    match tokio::time::timeout(Duration::from_secs(DEFINITION_TIMEOUT_SECS), fut).await {
        //: timeouts are NOT crashes — return Ok(None)
        // so the per-ref budget enforcement keeps running. Real request errors
        // (server exit, channel closed) propagate as Err so the CRASH_THRESHOLD
        // branch in `enrich_exports` can fire, mirroring the hover pass.
        Err(_elapsed) => Ok(None),
        Ok(Err(e)) => Err(LspError::RequestFailed(e.to_string())),
        Ok(Ok(None)) => Ok(None),
        Ok(Ok(Some(GotoDefinitionResponse::Scalar(loc)))) => Ok(Some(loc)),
        Ok(Ok(Some(GotoDefinitionResponse::Array(mut locs)))) => {
            // Empty Array means tsserver did not resolve the property — treat
            // as None so the diff pipeline distinguishes "ran but no result"
            // from "didn't run at all".
            if locs.is_empty() {
                Ok(None)
            } else {
                Ok(Some(locs.remove(0)))
            }
        }
        Ok(Ok(Some(GotoDefinitionResponse::Link(mut links)))) => {
            if links.is_empty() {
                Ok(None)
            } else {
                let l = links.remove(0);
                Ok(Some(Location {
                    uri: l.target_uri,
                    range: l.target_range,
                }))
            }
        }
    }
}
