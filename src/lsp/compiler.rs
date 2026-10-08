//! Evaluate public TypeScript contracts with the compiler paired with the LSP.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use super::managed::CompilerBinary;
#[cfg(all(test, feature = "lsp-tests"))]
use super::managed::LspBinary;
use crate::types::{shape::TypeShape, ExportMap};

#[derive(Deserialize)]
struct CompilerResponse {
    results: Vec<ResolvedSymbol>,
    issues: Vec<String>,
}

#[derive(Deserialize)]
struct ResolvedSymbol {
    id: usize,
    overloads: Vec<TypeShape>,
    signature: Option<String>,
    typeof_targets: Vec<Option<async_lsp::lsp_types::Location>>,
    issues: Vec<String>,
}

/// `None` is reserved for test servers without a paired compiler.
#[cfg(all(test, feature = "lsp-tests"))]
pub(super) async fn resolve_public_types(
    exports: &mut ExportMap,
    root: &Path,
    binary: &LspBinary,
) -> Option<Vec<String>> {
    let server = binary.tsserver_path(root)?;
    let toolchain = CompilerBinary {
        library: server.parent()?.join("typescript.js"),
        node: match binary {
            LspBinary::Managed { node, .. } => node.clone(),
            LspBinary::Direct(_) => "node".into(),
        },
    };
    Some(evaluate_public_types(exports, root, &toolchain).await)
}

pub(super) async fn evaluate_public_types(
    exports: &mut ExportMap,
    root: &Path,
    toolchain: &CompilerBinary,
) -> Vec<String> {
    let mut keys: Vec<_> = exports.keys().cloned().collect();
    keys.sort_by_key(|key| {
        format!(
            "{}|{:?}|{:?}|{}",
            key.subpath, key.condition, key.space, key.name
        )
    });
    let symbols: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(id, key)| {
            let symbol = &exports[key];
            let refs: Vec<_> = symbol.typeof_import_refs.iter().map(|reference| serde_json::json!({
                "name":reference.property, "line":reference.property_line, "col":reference.property_col
            })).collect();
            serde_json::json!({"id":id, "file":root.join(&symbol.file), "name":symbol.name,
            "local_name":symbol.local_name, "line":symbol.line, "col":symbol.col, "refs":refs})
        })
        .collect();
    let input = serde_json::json!({"root":root, "compiler":toolchain.library, "symbols":symbols});
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let mut child = tokio::process::Command::new(&toolchain.node)
            .args([
                "--max-old-space-size=512",
                "-e",
                include_str!("compiler.cjs"),
            ])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("compiler startup failed: {error}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(input.to_string().as_bytes())
                .await
                .map_err(|error| format!("compiler input failed: {error}"))?;
        }
        let output = child
            .wait_with_output()
            .await
            .map_err(|error| format!("compiler process failed: {error}"))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "compiler exited with {}: {}",
                output.status,
                detail.chars().take(1000).collect::<String>()
            ));
        }
        if output.stdout.len() > 16 * 1024 * 1024 {
            return Err("compiler output budget exhausted".into());
        }
        serde_json::from_slice::<CompilerResponse>(&output.stdout)
            .map_err(|error| format!("invalid compiler response: {error}"))
    })
    .await;
    let response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(issue)) => return vec![issue],
        Err(_) => return vec!["compiler public type evaluation timed out".into()],
    };
    let mut issues = response.issues;
    if response.results.len() != keys.len() {
        issues.push("compiler did not resolve every public declaration".into());
    }
    for result in response.results {
        let Some(key) = keys.get(result.id) else {
            issues.push("compiler returned an unknown public declaration".into());
            continue;
        };
        issues.extend(
            result
                .issues
                .into_iter()
                .map(|issue| format!("{}: {issue}", key.name)),
        );
        if !result.overloads.is_empty() {
            if let Some(symbol) = exports.get_mut(key) {
                symbol.overloads = result
                    .overloads
                    .into_iter()
                    .map(TypeShape::normalize)
                    .collect();
                symbol.type_signature = result.signature;
                if let Some(TypeShape::Satisfies { narrowed, .. }) = symbol.satisfies_type.as_mut()
                {
                    *narrowed = symbol.overloads.first().cloned().map(Box::new);
                }
            }
        }
        for (index, location) in result.typeof_targets.into_iter().enumerate() {
            exports.record_typeof_target(key, index, location);
        }
    }
    issues
}
