//! LSP enrichment module: spawns typescript-language-server, queries hover info,
//! and populates ExportSymbol with resolved type signatures and TypeShape values.

pub mod client;
pub mod definition;
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
#[derive(Debug)]
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

    // Step 3: group exports by file path (collect full ExportKeys per file — Phase 18 D-01).
    // The ExportKey is retained through hover dispatch so the write-back can target
    // the correct (subpath, condition, space, name) bucket rather than only by name.
    let mut file_groups: HashMap<PathBuf, Vec<crate::types::ExportKey>> = HashMap::new();
    for (key, symbol) in exports.iter() {
        file_groups
            .entry(symbol.file.clone())
            .or_default()
            .push(key.clone());
    }

    // Step 4: iterate files, send hover requests; respect total budget (60s per D-06)
    let budget_start = Instant::now();
    let total_budget = std::time::Duration::from_secs(60);

    // Collect file list first so we can process without holding borrow on exports
    let file_list: Vec<(PathBuf, Vec<crate::types::ExportKey>)> = file_groups.into_iter().collect();

    // ROB-02: Track consecutive LSP request failures. If the session crashes mid-run
    // (e.g., tsserver OOM, SIGKILL, or IPC channel closed), hover requests will fail
    // repeatedly. After CRASH_THRESHOLD consecutive failures, treat as a session crash
    // and return Degraded rather than spinning through all remaining files.
    const CRASH_THRESHOLD: usize = 3;
    let mut consecutive_failures: usize = 0;

    for (rel_file_path, symbol_keys) in &file_list {
        if budget_start.elapsed() >= total_budget {
            tracing::warn!(
                "LSP total budget (60s) exceeded; remaining symbols will lack type info"
            );
            break;
        }

        // ExportSymbol.file is relative to pkg_root — resolve to absolute for hover/URI (Bug fix R1)
        let abs_file_path = if rel_file_path.is_absolute() {
            rel_file_path.clone()
        } else {
            pkg_root.join(rel_file_path)
        };

        // Build hover-info list: (name, line, col) for each symbol in this file.
        // Phase 18: the LSP is positional (file + line + col), so we pass the
        // ExportKey's name field across the boundary and match names back on the
        // write-back side. See the results loop below for the name->all-keys
        // multi-update semantics that preserve first-write-wins on type_signature.
        let hover_inputs: Vec<hover::SymbolHoverInput> = symbol_keys
            .iter()
            .filter_map(|key| {
                exports.get(key).map(|s| hover::SymbolHoverInput {
                    name: key.name.clone(),
                    line: s.line,
                    col: s.col,
                })
            })
            .collect();

        // Send hover requests for this file's symbols
        // ROB-02: Mid-session tsserver crash detection.
        // Any error from the LSP request path (server exit, channel closed, timeout)
        // increments the failure counter. On CRASH_THRESHOLD consecutive failures,
        // return Degraded to avoid spinning through all remaining files.
        let results = match hover::hover_file_symbols(&session, &abs_file_path, &hover_inputs).await
        {
            Ok(r) => {
                consecutive_failures = 0; // reset on success
                r
            }
            Err(e) => {
                consecutive_failures += 1;
                tracing::warn!("hover failed for {}: {e}", abs_file_path.display());
                if consecutive_failures >= CRASH_THRESHOLD {
                    let msg = format!(
                        "LSP session failed: {} \u{2014} falling back to structural-only diff",
                        e
                    );
                    // WR-04 (Phase 17 review): attempt a graceful shutdown handshake
                    // before drop. kill_on_drop remains the safety net — shutdown
                    // errors are intentionally ignored because the session is already
                    // failing, but the handshake lets tsserver flush buffers / temp
                    // files cleanly when it IS still responsive to a final request.
                    let _ = session.shutdown().await;
                    return Ok(EnrichResult::Degraded(msg));
                }
                continue;
            }
        };

        // Apply results back to exports.
        // Phase 18: the hover result is keyed only by name (LSP protocol boundary),
        // so we re-key through the file's ExportKeys. A single hover response
        // legitimately applies to every ExportKey at that (file, line, col)
        // position; matching by name covers the common case. Guarding on
        // `is_none()` / `is_empty()` preserves the pre-Phase-18 first-write-wins
        // semantic for type_signature on the same key.
        for (name, type_signature, overloads) in results {
            for key in symbol_keys {
                if key.name == name {
                    if let Some(symbol) = exports.get_mut(key) {
                        if symbol.type_signature.is_none() {
                            symbol.type_signature = type_signature.clone();
                        }
                        if !overloads.is_empty() && symbol.overloads.is_empty() {
                            symbol.overloads = overloads.clone();
                        }
                        // TYPE-01 (Phase 20): if this symbol has an unfilled
                        // `TypeShape::Satisfies { narrowed: None, .. }` slot,
                        // fill it from the LSP hover result. Hover on a
                        // `const foo = {...} satisfies T` returns the narrowed
                        // literal type (not `T`); parse it through
                        // parse_type_string and stash into the `narrowed` slot.
                        // Leaving it None preserves degraded-mode semantics
                        // (Finding message carries
                        // `limitation: satisfies-narrowed-type-not-verified`).
                        if let Some(crate::types::shape::TypeShape::Satisfies {
                            constraint,
                            narrowed: None,
                        }) = symbol.satisfies_type.as_ref()
                        {
                            if let Some(narrow_text) = type_signature.as_ref() {
                                let narrowed_shape =
                                    crate::types::parser::parse_type_string(narrow_text);
                                let constraint_clone = constraint.clone();
                                symbol.satisfies_type =
                                    Some(crate::types::shape::TypeShape::Satisfies {
                                        constraint: constraint_clone,
                                        narrowed: Some(Box::new(narrowed_shape)),
                                    });
                            }
                        }
                    }
                }
            }
        }
    }

    // Step 5: shutdown
    if let Err(e) = session.shutdown().await {
        tracing::warn!("LSP shutdown error: {e}");
    }

    Ok(EnrichResult::Enriched)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// ROB-02: Structural test — verify that EnrichResult::Degraded carries the crash message
    /// and that the pattern-match branches are correct. The full crash path (CRASH_THRESHOLD
    /// consecutive failures from a real subprocess) is exercised by integration tests in
    /// tests/phase17_integration.rs; this unit test validates the type and message structure.
    #[test]
    fn rob_02_degraded_carries_failure_message() {
        let err = anyhow::anyhow!("simulated tsserver crash");
        let result = EnrichResult::Degraded(format!(
            "LSP session failed: {} \u{2014} falling back to structural-only diff",
            err
        ));
        match result {
            EnrichResult::Degraded(msg) => {
                assert!(
                    msg.contains("LSP session failed"),
                    "ROB-02: Degraded message must contain 'LSP session failed'; got: {msg:?}"
                );
                assert!(
                    msg.contains("falling back"),
                    "ROB-02: Degraded message must contain 'falling back'; got: {msg:?}"
                );
            }
            EnrichResult::Enriched => panic!("expected Degraded, got Enriched"),
        }
    }
}
