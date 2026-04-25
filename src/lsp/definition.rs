//! LSP `textDocument/definition` requests — Phase 22 INF-04.
//!
//! Mirrors `src/lsp/hover.rs`. Used exclusively by INF-04 cross-file
//! resolution of `typeof import('./y').Z` type references.
//!
//! ## Method choice
//!
//! CONTEXT.md D-05 mandates a kickoff spike (Plan 22-04 Task 0) to confirm
//! `textDocument/definition` resolves the property-identifier position in
//! `typeof import('./y').Z` to the declaration of `Z` inside `./y.ts`. The
//! spike's verdict is committed in 22-04-SPIKE-TRACE.md:
//!   - **VERDICT: definition** — this module uses `LanguageServer::definition`.
//!   - VERDICT: typeDefinition  — would swap `definition` /
//!     `GotoDefinitionParams` / `GotoDefinitionResponse` for
//!     `type_definition` / `GotoTypeDefinitionParams` /
//!     `GotoTypeDefinitionResponse`. Not triggered for INF-04.
//!
//! See `.planning/phases/22-lsp-gated-structural-identity/22-04-SPIKE-TRACE.md`.

use std::time::Duration;

use async_lsp::lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use async_lsp::LanguageServer;

use super::client::LspSession;
use super::LspError;

/// 5-second per-request timeout — matches the hover convention (D-06).
/// See `src/lsp/hover.rs:22` for the precedent.
const DEFINITION_TIMEOUT_SECS: u64 = 5;

/// Resolve a `textDocument/definition` request. Returns the resolved
/// `Location` on success, `Ok(None)` on timeout / unresponsive server /
/// no-result (matches `hover_symbol` behavior at `src/lsp/hover.rs:118-127`).
///
/// Position conversion: `ExportSymbol`/`TypeofImportRef` store 1-indexed
/// positions; LSP uses 0-indexed. Pitfall 1 precedent.
///
/// `GotoDefinitionResponse` has three variants — Scalar (single Location),
/// Array (Vec<Location>), Link (Vec<LocationLink>). Per RESEARCH Pitfall 7
/// we collapse to a single Location: take the first entry on Array/Link.
#[allow(dead_code)]
pub async fn resolve_definition_location(
    session: &LspSession,
    file_uri: Url,
    line: u32,
    col: u32,
) -> Result<Option<Location>, LspError> {
    let lsp_line = line.saturating_sub(1);
    let lsp_col = col.saturating_sub(1);

    let fut = session.server.clone().definition(GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: file_uri },
            position: Position::new(lsp_line, lsp_col),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: Default::default(),
    });

    match tokio::time::timeout(Duration::from_secs(DEFINITION_TIMEOUT_SECS), fut).await {
        Err(_elapsed) => Ok(None),
        Ok(Err(e)) => {
            tracing::debug!("definition request error: {e}");
            Ok(None)
        }
        Ok(Ok(None)) => Ok(None),
        Ok(Ok(Some(GotoDefinitionResponse::Scalar(loc)))) => Ok(Some(loc)),
        Ok(Ok(Some(GotoDefinitionResponse::Array(mut locs)))) => {
            // Empty Array means tsserver did not resolve the property — treat
            // as None so the diff pipeline distinguishes "ran but no result"
            // from "didn't run at all" (Task 4 / Task 5 semantics).
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
