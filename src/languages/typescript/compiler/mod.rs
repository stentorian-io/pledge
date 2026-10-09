//! Public type enrichment through the TypeScript compiler, with reusable LSP clients.

pub(crate) mod bridge;
pub mod protocol;
pub mod toolchain;

use crate::ecosystems::node::discovery::resolver;
use crate::ecosystems::node::{dependencies, discovery};
#[cfg(all(test, feature = "lsp-tests"))]
use crate::languages::javascript::model as js_model;
use crate::languages::javascript::model::ExportMap;
#[cfg(all(test, feature = "lsp-tests"))]
use crate::languages::typescript::model as ts_model;
#[cfg(all(test, feature = "lsp-tests"))]
use crate::languages::typescript::types::{
    parser as type_parser, reachability, shape as type_shapes,
};
#[cfg(all(test, feature = "lsp-tests"))]
use std::collections::HashMap;
use std::path::Path;
#[cfg(all(test, feature = "lsp-tests"))]
use std::path::PathBuf;
#[cfg(all(test, feature = "lsp-tests"))]
use std::time::Instant;
use thiserror::Error;

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

impl From<dependencies::DependencyError> for LspError {
    fn from(error: dependencies::DependencyError) -> Self {
        use crate::ecosystems::node::dependencies::DependencyError;
        match error {
            DependencyError::Timeout => Self::Timeout("dependency installation".into()),
            DependencyError::Failed(message) => Self::InitFailed(message),
        }
    }
}

/// Result of LSP enrichment — either successful enrichment or graceful degradation.
#[derive(Debug)]
pub enum EnrichResult {
    /// All requested exports returned type information and compiler evaluation completed.
    Enriched,
    /// Some analysis was unavailable or incomplete; resolved evidence remains usable.
    Degraded(String),
}

impl EnrichResult {
    /// Retain source/discovery failures while retiring limits resolved by the compiler.
    pub fn reconcile_discovery_warnings(&self, warnings: &mut Vec<resolver::ResolveWarning>) {
        if matches!(self, Self::Enriched) {
            warnings.retain(|warning| !warning.type_expansion_only);
        }
    }
}

/// Prepare dependencies and resolve public contracts and declaration locations.
/// Missing compiler or type evidence returns `EnrichResult::Degraded`.
pub async fn enrich_exports(
    exports: &mut ExportMap,
    pkg_root: &Path,
) -> Result<EnrichResult, LspError> {
    let installed = match dependencies::prepare_dependencies(pkg_root).await {
        Ok(installed) => installed,
        Err(error) => return Ok(EnrichResult::Degraded(LspError::from(error).to_string())),
    };
    let mut discovery_warnings = Vec::new();
    if installed {
        // Low-level callers may have discovered before dependencies existed.
        // Replace forwarding placeholders and add the newly reachable exports.
        let (mut discovered, warnings) = match discovery::discover_exports(pkg_root) {
            Ok(result) => result,
            Err(error) => return Ok(EnrichResult::Degraded(error.to_string())),
        };
        discovery_warnings = warnings;
        exports.retain(|key, symbol| !symbol.is_reexport || discovered.contains_key(key));
        for (key, symbol) in discovered.drain() {
            if exports
                .get(&key)
                .is_none_or(|existing| existing.is_reexport)
            {
                exports.insert(key, symbol);
            }
        }
        exports.directives.extend(discovered.directives);
        let keys: std::collections::HashSet<_> = exports.keys().cloned().collect();
        exports.typeof_targets.retain(|key, _| keys.contains(key));
    }
    let result = enrich_prepared_exports(exports, pkg_root).await?;
    result.reconcile_discovery_warnings(&mut discovery_warnings);
    let mut incomplete: Vec<_> = discovery_warnings
        .into_iter()
        .filter(|warning| warning.incomplete)
        .map(|warning| warning.message)
        .collect();
    if incomplete.is_empty() {
        Ok(result)
    } else {
        if let EnrichResult::Degraded(issue) = result {
            incomplete.push(issue);
        }
        Ok(EnrichResult::Degraded(incomplete.join("; ")))
    }
}

/// Enrich a map discovered after successful dependency preparation.
/// Use [`dependencies::prepare_dependencies`] before discovery, or `enrich_exports`
/// to prepare and refresh a previously discovered map automatically.
pub async fn enrich_prepared_exports(
    exports: &mut ExportMap,
    pkg_root: &Path,
) -> Result<EnrichResult, LspError> {
    let compiler = match toolchain::resolve_compiler(pkg_root).await {
        Ok(compiler) => compiler,
        Err(error) => return Ok(EnrichResult::Degraded(error.to_string())),
    };
    let mut issues = bridge::evaluate_public_types(exports, pkg_root, &compiler).await;
    issues.extend(
        exports
            .keys()
            .filter(|key| key.name.starts_with("*<"))
            .map(|key| {
                format!(
                    "external export barrel `{}` has not been resolved",
                    key.name
                )
            }),
    );
    if issues.is_empty() {
        Ok(EnrichResult::Enriched)
    } else {
        Ok(EnrichResult::Degraded(issues.join("; ")))
    }
}

#[cfg(all(test, feature = "lsp-tests"))]
async fn enrich_with_binary(
    exports: &mut ExportMap,
    pkg_root: &Path,
    binary: &toolchain::LspBinary,
    total_budget: std::time::Duration,
) -> Result<EnrichResult, LspError> {
    // Step 2: start an LSP session
    let session = match protocol::client::start_session(binary, pkg_root).await {
        Ok(s) => s,
        Err(e) => return Ok(EnrichResult::Degraded(e.to_string())),
    };

    // Step 3: group full ExportKeys by file path.
    // The ExportKey is retained through hover dispatch so the write-back can target
    // the correct (subpath, condition, space, name) bucket rather than only by name.
    let mut file_groups: HashMap<PathBuf, Vec<js_model::ExportKey>> = HashMap::new();
    for (key, symbol) in exports.iter() {
        file_groups
            .entry(symbol.file.clone())
            .or_default()
            .push(key.clone());
    }

    // Step 4: send hover requests within the 60-second total budget.
    let budget_start = Instant::now();
    let mut incomplete = Vec::new();
    incomplete.extend(
        exports
            .keys()
            .filter(|key| key.name.starts_with("*<"))
            .map(|key| {
                format!(
                    "external export barrel `{}` has not been resolved",
                    key.name
                )
            }),
    );

    // Collect file list first so we can process without holding borrow on exports
    let file_list: Vec<(PathBuf, Vec<js_model::ExportKey>)> = file_groups.into_iter().collect();

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
            incomplete
                .push("LSP total budget exhausted before all exports were resolved".to_string());
            break;
        }

        // ExportSymbol.file is relative to pkg_root — resolve to absolute for hover/URI (Bug fix R1)
        let abs_file_path = if rel_file_path.is_absolute() {
            rel_file_path.clone()
        } else {
            pkg_root.join(rel_file_path)
        };

        // Build hover-info list: (name, line, col) for each symbol in this file.
        // the LSP is positional (file + line + col), so we pass the
        // ExportKey's name field across the boundary and match names back on the
        // write-back side. See the results loop below for the name->all-keys
        // multi-update semantics that preserve first-write-wins on type_signature.
        let hover_inputs: Vec<protocol::hover::SymbolHoverInput> = symbol_keys
            .iter()
            .filter_map(|key| {
                exports.get(key).map(|s| protocol::hover::SymbolHoverInput {
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
        let response = tokio::time::timeout(
            total_budget.saturating_sub(budget_start.elapsed()),
            protocol::hover::hover_file_symbols(&session, &abs_file_path, &hover_inputs),
        )
        .await
        .unwrap_or_else(|_| {
            Err(LspError::Timeout(
                "total enrichment budget exhausted".into(),
            ))
        });
        let results = match response {
            Ok(r) => {
                consecutive_failures = 0; // reset on success
                r
            }
            Err(e) => {
                incomplete.push(e.to_string());
                consecutive_failures += 1;
                tracing::warn!("hover failed for {}: {e}", abs_file_path.display());
                if consecutive_failures >= CRASH_THRESHOLD {
                    let msg = format!(
                        "LSP session failed: {} \u{2014} falling back to structural-only diff",
                        e
                    );
                    //: attempt a graceful shutdown handshake
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
        // the hover result is keyed only by name (LSP protocol boundary),
        // so we re-key through the file's ExportKeys. A single hover response
        // legitimately applies to every ExportKey at that (file, line, col)
        // position; matching by name covers the common case. Guarding on
        // `is_none` / `is_empty` preserves the first-write-wins
        // semantic for type_signature on the same key.
        for (name, type_signature, overloads) in results {
            if type_signature.is_none() {
                incomplete.push(format!(
                    "no type information for `{name}` in {}",
                    rel_file_path.display()
                ));
            }
            for key in symbol_keys {
                if key.name == name {
                    if let Some(symbol) = exports.get_mut(key) {
                        if symbol.type_signature.is_none() {
                            symbol.type_signature = type_signature.clone();
                        }
                        if !overloads.is_empty() {
                            symbol.overloads = overloads.clone();
                        }
                        // TYPE-01: if this symbol has an unfilled
                        // `TypeShape::Satisfies { narrowed: None,.. }` slot,
                        // fill it from the LSP hover result. Hover on a
                        // `const foo = {...} satisfies T` returns the narrowed
                        // literal type (not `T`); parse it through
                        // parse_type_string and stash into the `narrowed` slot.
                        // Leaving it None preserves degraded-mode semantics
                        // (Finding message carries
                        // `limitation: satisfies-narrowed-type-not-verified`).
                        if let Some(type_shapes::TypeShape::Satisfies {
                            constraint,
                            narrowed: None,
                        }) = symbol.satisfies_type.as_ref()
                        {
                            if let Some(narrow_text) = type_signature.as_ref() {
                                let narrowed_shape = type_parser::parse_type_string(narrow_text);
                                let constraint_clone = constraint.clone();
                                symbol.satisfies_type = Some(type_shapes::TypeShape::Satisfies {
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

    // INF-04: second LSP pass — resolve typeof_import_refs via
    // textDocument/definition. Runs after all hovers without interleaving
    // to keep hover-completion timing predictable.
    //
    // Shares the 60s total_budget and 3-strike CRASH_THRESHOLD with the
    // hover pass.
    //
    // Resolved locations are stored on the ExportMap via the `typeof_targets`
    // sidecar (parallel to the `directives` sidecar). The diff
    // pipeline reads this sidecar in `compare::types::diff_typeof_import_refs`.
    {
        use async_lsp::lsp_types::Url;

        // Snapshot of (key, refs) pairs — we cannot hold an immutable borrow
        // on `exports` while calling `record_typeof_target` (mutable borrow).
        let keys_with_refs: Vec<(js_model::ExportKey, Vec<ts_model::TypeofImportRef>)> = exports
            .iter()
            .filter(|(_, sym)| !sym.typeof_import_refs.is_empty())
            .map(|(k, sym)| (k.clone(), sym.typeof_import_refs.clone()))
            .collect();

        let mut definition_failures: usize = 0;
        for (key, refs) in keys_with_refs {
            if budget_start.elapsed() >= total_budget {
                tracing::warn!(
                    "LSP total budget exhausted before resolving all typeof_import_refs"
                );
                incomplete
                    .push("LSP total budget exhausted during definition resolution".to_string());
                break;
            }

            // The request URI is the FILE that contains the symbol whose
            // type position holds the typeof-import refs. Each ref's
            // (property_line, property_col) is the position passed to
            // textDocument/definition — that's the column of the `Y` in
            // `typeof import('./x').Y`.
            let abs_file_path = {
                let sym = match exports.get(&key) {
                    Some(s) => s,
                    None => continue,
                };
                if sym.file.is_absolute() {
                    sym.file.clone()
                } else {
                    pkg_root.join(&sym.file)
                }
            };
            let file_uri = match Url::from_file_path(&abs_file_path) {
                Ok(u) => u,
                Err(_) => continue,
            };

            for (idx, r) in refs.iter().enumerate() {
                let result = protocol::definition::resolve_definition_location(
                    &session,
                    file_uri.clone(),
                    r.property_line,
                    r.property_col,
                )
                .await;
                match result {
                    Ok(location_opt) => {
                        definition_failures = 0;
                        exports.record_typeof_target(&key, idx, location_opt);
                    }
                    Err(e) => {
                        incomplete.push(e.to_string());
                        definition_failures += 1;
                        tracing::warn!(
                            "typeof-import definition failed for {} ref#{idx}: {e}",
                            key.name
                        );
                        if definition_failures >= CRASH_THRESHOLD {
                            let msg = format!(
                                "LSP definition pass failed: {} \u{2014} falling back to structural-only diff",
                                e
                            );
                            let _ = session.shutdown().await;
                            return Ok(EnrichResult::Degraded(msg));
                        }
                        //: a single sub-threshold error
                        // should not abort all remaining symbols. Continue to
                        // the next ref, as in the hover pass. The CRASH_THRESHOLD branch already
                        // handles the "too many failures" case.
                        continue;
                    }
                }
            }
        }
    }

    if let Some(issues) = bridge::resolve_public_types(exports, pkg_root, binary).await {
        incomplete.extend(issues);
    } else {
        incomplete.extend(reachability::resolve_public_types(exports, pkg_root, true));
    }

    // Step 5: shutdown
    if let Err(e) = session.shutdown().await {
        tracing::warn!("LSP shutdown error: {e}");
    }

    if incomplete.is_empty() {
        Ok(EnrichResult::Enriched)
    } else {
        incomplete.sort();
        incomplete.dedup();
        Ok(EnrichResult::Degraded(incomplete.join("; ")))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[cfg(test)]
    use crate::languages::typescript::compiler::*;

    /// ROB-02: Structural test — verify that EnrichResult::Degraded carries the crash message
    /// and that the pattern-match branches are correct. The full crash path (CRASH_THRESHOLD
    /// consecutive failures from a real subprocess) is exercised by integration tests in
    /// tests/README.md; this unit test validates the type and message structure.
    #[test]
    fn degraded_carries_failure_message() {
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
            EnrichResult::Enriched => {
                panic!("expected Degraded, got Enriched")
            }
        }
    }
}

#[cfg(all(test, feature = "lsp-tests"))]
mod failure_tests;
