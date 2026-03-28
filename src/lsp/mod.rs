//! LSP enrichment module: spawns typescript-language-server, queries hover info,
//! and populates ExportSymbol with resolved type signatures and TypeShape values.

pub mod client;
pub mod hover;
pub mod managed;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use thiserror::Error;

use crate::types::ExportMap;

/// Errors that can occur in the LSP enrichment pipeline.
#[derive(Debug, Error)]
pub enum LspError {
    #[error("typescript-language-server not available: {0}")]
    Unavailable(String),
    #[error("LSP initialization failed: {0}")]
    InitFailed(String),
    #[error("LSP request failed: {0}")]
    RequestFailed(String),
    #[error("LSP timeout: {0}")]
    Timeout(String),
}

/// Result of LSP enrichment — either successful enrichment or graceful degradation.
pub enum EnrichResult {
    /// Types were resolved (possibly partially — some symbols may have type_shape: None)
    Enriched,
    /// LSP was unavailable — no types resolved, analysis continues in degraded mode
    Degraded(String),
}

/// Enrich all exports in the given map with type signatures from typescript-language-server.
///
/// Spawns a single LSP session per call, batches hover requests for all symbols,
/// then shuts down. On LSP unavailability, returns `EnrichResult::Degraded` (not an error).
pub async fn enrich_exports(
    exports: &mut ExportMap,
    pkg_root: &Path,
) -> Result<EnrichResult, LspError> {
    // Step 1: resolve the LSP binary (managed cache → PATH → download)
    let binary = match managed::resolve_lsp_binary().await {
        Ok(b) => b,
        Err(LspError::Unavailable(msg)) => return Ok(EnrichResult::Degraded(msg)),
        Err(e) => return Ok(EnrichResult::Degraded(e.to_string())),
    };

    // Step 2: start an LSP session
    let session = match client::start_session(&binary, pkg_root).await {
        Ok(s) => s,
        Err(e) => return Ok(EnrichResult::Degraded(e.to_string())),
    };

    // Step 3: group exports by file path (collect symbol names per file)
    let mut file_groups: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for (name, symbol) in exports.iter() {
        file_groups
            .entry(symbol.file.clone())
            .or_default()
            .push(name.clone());
    }

    // Step 4: iterate files, send hover requests; respect total budget (60s per D-06)
    let budget_start = Instant::now();
    let total_budget = std::time::Duration::from_secs(60);

    // Collect file list first so we can process without holding borrow on exports
    let file_list: Vec<(PathBuf, Vec<String>)> = file_groups.into_iter().collect();

    for (file_path, symbol_names) in &file_list {
        if budget_start.elapsed() >= total_budget {
            tracing::warn!(
                "LSP total budget (60s) exceeded; remaining symbols will lack type info"
            );
            break;
        }

        // Build hover-info list: (name, line, col) for each symbol in this file
        let hover_inputs: Vec<hover::SymbolHoverInput> = symbol_names
            .iter()
            .filter_map(|name| {
                exports.get(name).map(|s| hover::SymbolHoverInput {
                    name: name.clone(),
                    line: s.line,
                    col: s.col,
                })
            })
            .collect();

        // Send hover requests for this file's symbols
        let results =
            match hover::hover_file_symbols(&session, file_path, &hover_inputs).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("hover failed for {}: {e}", file_path.display());
                    continue;
                }
            };

        // Apply results back to exports
        for (name, type_signature, type_shape) in results {
            if let Some(symbol) = exports.get_mut(&name) {
                symbol.type_signature = type_signature;
                symbol.type_shape = type_shape;
            }
        }
    }

    // Step 5: shutdown
    if let Err(e) = session.shutdown().await {
        tracing::warn!("LSP shutdown error: {e}");
    }

    Ok(EnrichResult::Enriched)
}
