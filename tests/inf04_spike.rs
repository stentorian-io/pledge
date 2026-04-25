//! Phase 22 INF-04 kickoff spike (Plan 22-04 Task 0).
//!
//! Per CONTEXT.md D-05: confirm `textDocument/definition` resolves the
//! `Z` property position in `typeof import('./y').Z` to the declaration of
//! `Z` inside `./y.ts` BEFORE any production code is written.
//!
//! Run with: `cargo test --features lsp-tests --test inf04_spike -- --nocapture`
//!
//! Output goes to `.planning/phases/22-lsp-gated-structural-identity/22-04-SPIKE-TRACE.md`.

#![cfg(feature = "lsp-tests")]

use std::path::PathBuf;
use std::time::Duration;

use async_lsp::lsp_types::{
    DidOpenTextDocumentParams, GotoDefinitionParams, HoverParams, Position,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Url,
    WorkDoneProgressParams,
};
use async_lsp::LanguageServer;

// `GotoTypeDefinitionParams` is a type alias of `GotoDefinitionParams` in lsp-types.
type GotoTypeDefinitionParams = GotoDefinitionParams;

#[tokio::test(flavor = "current_thread")]
async fn inf04_spike_definition_resolves_property() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let pkg_root = PathBuf::from(manifest)
        .join("tests/fixtures/phase-22/inf-04-typeof-import/clean/old");

    let binary = interlint::lsp::managed::resolve_lsp_binary()
        .await
        .expect("LSP binary must be resolvable for spike");
    let session = interlint::lsp::client::start_session(&binary, &pkg_root)
        .await
        .expect("LSP session must start for spike");

    // Open both files so tsserver can resolve the cross-file definition.
    let index_path = pkg_root.join("index.ts");
    let y_path = pkg_root.join("y.ts");
    let index_uri = Url::from_file_path(&index_path).unwrap();
    let y_uri = Url::from_file_path(&y_path).unwrap();
    let index_src = tokio::fs::read_to_string(&index_path).await.unwrap();
    let y_src = tokio::fs::read_to_string(&y_path).await.unwrap();

    session
        .server
        .clone()
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: y_uri,
                language_id: "typescript".into(),
                version: 0,
                text: y_src,
            },
        })
        .unwrap();
    session
        .server
        .clone()
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: index_uri.clone(),
                language_id: "typescript".into(),
                version: 0,
                text: index_src,
            },
        })
        .unwrap();

    // Give tsserver time to index.
    tokio::time::sleep(Duration::from_millis(3000)).await;

    // Probe a few positions and methods to characterize the LSP behavior.
    // Index source line: `export type X = typeof import('./y').Z;`
    //                     0         1         2         3
    //                     0123456789012345678901234567890123456789
    //                     ^      ^    ^         ^         ^^
    //                     0      7    12        22        37 (Z)
    let probe_positions: &[(u32, &str)] = &[
        (12, "X"),
        (22, "import keyword"),
        (32, "specifier"),
        (37, "Z property"),
    ];
    for (col, label) in probe_positions {
        let hover_resp = session
            .server
            .clone()
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: index_uri.clone(),
                    },
                    position: Position::new(0, *col),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .await
            .expect("hover request must succeed");
        eprintln!(
            "INF-04 SPIKE HOVER @col={} ({}): {}",
            col,
            label,
            match &hover_resp {
                Some(h) => serde_json::to_string(h).unwrap(),
                None => "<None>".to_string(),
            }
        );
        let def_resp = session
            .server
            .clone()
            .definition(GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: index_uri.clone(),
                    },
                    position: Position::new(0, *col),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
            })
            .await
            .expect("definition request must succeed");
        eprintln!(
            "INF-04 SPIKE DEFINITION @col={} ({}): {}",
            col,
            label,
            serde_json::to_string(&def_resp).unwrap()
        );
        let typedef_resp = session
            .server
            .clone()
            .type_definition(GotoTypeDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: index_uri.clone(),
                    },
                    position: Position::new(0, *col),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
            })
            .await
            .expect("typeDefinition request must succeed");
        eprintln!(
            "INF-04 SPIKE TYPEDEFINITION @col={} ({}): {}",
            col,
            label,
            serde_json::to_string(&typedef_resp).unwrap()
        );
    }

    // `Z` is at byte offset 37 in `export type X = typeof import('./y').Z;`
    // tree-sitter / LSP positions are 0-indexed.
    let position = Position::new(0, 37);

    // --- Attempt 1: textDocument/definition ---
    let mut def_resp = None;
    for attempt in 0..3 {
        let resp = session
            .server
            .clone()
            .definition(GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: index_uri.clone(),
                    },
                    position,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
            })
            .await
            .expect("definition request must succeed");
        let s = format!("{:?}", resp);
        let non_empty = !s.contains("Array([])") && resp.is_some();
        if non_empty {
            def_resp = Some(resp);
            break;
        }
        eprintln!(
            "definition attempt {} returned empty/None; retry after delay",
            attempt + 1
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;
        if attempt == 2 {
            def_resp = Some(resp);
        }
    }
    let def_resp = def_resp.unwrap();

    // --- Attempt 2: textDocument/typeDefinition (fallback per D-05 pivot rule) ---
    let typedef_resp = session
        .server
        .clone()
        .type_definition(GotoTypeDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: index_uri.clone(),
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        })
        .await
        .expect("typeDefinition request must succeed");

    let def_json = serde_json::to_string_pretty(&def_resp).unwrap();
    let typedef_json = serde_json::to_string_pretty(&typedef_resp).unwrap();

    eprintln!("INF-04 SPIKE RESPONSE [definition]:\n{}", def_json);
    eprintln!(
        "INF-04 SPIKE RESPONSE [typeDefinition]:\n{}",
        typedef_json
    );

    let _ = session.shutdown().await;

    // Decision rule: at least ONE of (definition, typeDefinition) must reference
    // y.ts (the declaration site of `Z`). Capture which method "won".
    let def_str = format!("{:?}", def_resp);
    let typedef_str = format!("{:?}", typedef_resp);
    let def_hit = def_str.contains("y.ts") && !def_str.contains("Array([])");
    let typedef_hit = typedef_str.contains("y.ts") && !typedef_str.contains("Array([])");

    eprintln!(
        "INF-04 SPIKE VERDICT: definition_hit={} typeDefinition_hit={}",
        def_hit, typedef_hit
    );

    assert!(
        def_hit || typedef_hit,
        "Neither definition nor typeDefinition resolved to y.ts.\n\
         definition: {def_str}\n\
         typeDefinition: {typedef_str}\n\
         The whole INF-04 LSP path is in question."
    );
}
