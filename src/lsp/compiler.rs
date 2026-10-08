//! Evaluate public TypeScript contracts with the compiler paired with the LSP.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::AsyncWriteExt;

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
    issues: Vec<String>,
}

/// `None` is reserved for test servers without a paired compiler.
pub(super) async fn resolve_public_types(
    exports: &mut ExportMap,
    root: &Path,
    binary: &LspBinary,
) -> Option<Vec<String>> {
    let server = binary.tsserver_path(root)?;
    let compiler = server.parent()?.join("typescript.js");
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
            serde_json::json!({"id":id, "file":root.join(&symbol.file), "name":symbol.name,
            "local_name":symbol.local_name, "line":symbol.line, "col":symbol.col})
        })
        .collect();
    let input = serde_json::json!({"root":root, "compiler":compiler, "symbols":symbols});
    let node = match binary {
        LspBinary::Managed { node, .. } => node.as_os_str(),
        LspBinary::Direct(_) => std::ffi::OsStr::new("node"),
    };
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let mut child = tokio::process::Command::new(node)
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
        Ok(Err(issue)) => return Some(vec![issue]),
        Err(_) => return Some(vec!["compiler public type evaluation timed out".into()]),
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
            }
        }
    }
    Some(issues)
}
