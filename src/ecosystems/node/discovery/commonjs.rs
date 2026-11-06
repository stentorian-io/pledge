//! Ask the installed Node parser for synthetic CommonJS import names without execution.

use super::entry_points::{package_source_files, resolve_entry_points_with_warnings, EntryPoint};
use crate::core::source::{MAX_LINE_BYTES, MAX_SOURCE_BYTES};
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

const MAX_ENTRIES: usize = 256;
const MAX_CONDITION_CONTEXTS: usize = 8;
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const PARSER: &str = include_str!("commonjs.cjs");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ImportEntry {
    pub path: PathBuf,
    pub subpath: String,
    pub condition: Option<String>,
    #[serde(default)]
    pub context: BTreeSet<String>,
    #[serde(default)]
    pub names: Option<BTreeSet<String>>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Default, Deserialize)]
pub(crate) struct NamedImports {
    #[serde(default)]
    pub node: String,
    pub entries: Vec<ImportEntry>,
    #[serde(skip)]
    pub errors: Vec<String>,
}

fn import_entry(entry: &EntryPoint) -> bool {
    matches!(
        entry
            .path
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("js" | "cjs")
    ) && !entry
        .condition
        .as_deref()
        .unwrap_or("")
        .split('/')
        .any(|condition| {
            condition == "require"
                || condition == "types"
                || condition == "typesVersions"
                || condition.starts_with("types@")
        })
}

fn condition_context(entry: &EntryPoint) -> BTreeSet<String> {
    entry
        .condition
        .as_deref()
        .unwrap_or("")
        .split('/')
        .filter(|condition| {
            !matches!(
                *condition,
                "" | "default" | "import" | "node" | "node-addons" | "module-sync"
            )
        })
        .map(str::to_string)
        .collect()
}

fn combined_contexts<'a>(
    declared: impl Iterator<Item = &'a BTreeSet<String>>,
) -> anyhow::Result<BTreeSet<BTreeSet<String>>> {
    let mut contexts = BTreeSet::from([BTreeSet::new()]);
    for conditions in declared {
        let combined: Vec<_> = contexts
            .iter()
            .map(|context| context.union(conditions).cloned().collect())
            .collect();
        contexts.extend(combined);
        if contexts.len() > MAX_CONDITION_CONTEXTS {
            bail!("CommonJS condition context limit reached");
        }
    }
    Ok(contexts)
}

fn known_esm(path: &Path, root: &Path) -> bool {
    if path.extension().is_none_or(|extension| extension != "js") {
        return false;
    }
    for directory in path.ancestors().skip(1).take(64) {
        if !directory.starts_with(root) {
            break;
        }
        let manifest = directory.join("package.json");
        if manifest.is_file() {
            if std::fs::metadata(&manifest)
                .map(|metadata| metadata.len() > MAX_SOURCE_BYTES as u64)
                .unwrap_or(true)
            {
                return false;
            }
            return std::fs::read(manifest)
                .ok()
                .and_then(|source| serde_json::from_slice::<serde_json::Value>(&source).ok())
                .is_some_and(|manifest| {
                    manifest.get("type").and_then(|value| value.as_str()) == Some("module")
                });
        }
    }
    false
}

fn requests(root: &Path) -> anyhow::Result<BTreeMap<BTreeSet<String>, Vec<ImportEntry>>> {
    if std::fs::metadata(root.join("package.json"))?.len() > MAX_SOURCE_BYTES as u64 {
        bail!("package metadata exceeds the CommonJS analysis limit");
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("package.json"))?)?;
    let (mut entries, _) = resolve_entry_points_with_warnings(root)?;
    if manifest
        .get("exports")
        .is_none_or(serde_json::Value::is_null)
    {
        entries.extend(package_source_files(root).into_iter().filter_map(|path| {
            let subpath = format!("./{}", path.strip_prefix(root).ok()?.to_string_lossy());
            Some(EntryPoint {
                path: path.canonicalize().ok()?,
                subpath,
                condition: None,
            })
        }));
    }
    let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for entry in entries
        .into_iter()
        .filter(|entry| import_entry(entry) && !known_esm(&entry.path, root))
    {
        if !seen.insert((
            entry.subpath.clone(),
            entry.condition.clone(),
            entry.path.clone(),
        )) {
            continue;
        }
        if seen.len() > MAX_ENTRIES {
            bail!("CommonJS public entry count limit reached");
        }
        groups
            .entry(condition_context(&entry))
            .or_default()
            .push(ImportEntry {
                path: entry.path,
                subpath: entry.subpath,
                condition: entry.condition,
                context: BTreeSet::new(),
                names: None,
                error: None,
            });
    }
    if groups.len() > MAX_CONDITION_CONTEXTS {
        bail!("CommonJS condition context limit reached");
    }
    Ok(groups)
}

async fn read_limited(mut stream: impl AsyncRead + Unpin, limit: usize) -> anyhow::Result<Vec<u8>> {
    let mut output = Vec::new();
    (&mut stream)
        .take(limit as u64 + 1)
        .read_to_end(&mut output)
        .await?;
    if output.len() > limit {
        bail!("Node parser output limit reached");
    }
    Ok(output)
}

async fn run_parser(
    root: &Path,
    conditions: BTreeSet<String>,
    mut entries: Vec<ImportEntry>,
) -> anyhow::Result<NamedImports> {
    for entry in &mut entries {
        entry.context = conditions.clone();
    }
    let request = serde_json::to_vec(&serde_json::json!({
        "root": root, "entries": entries,
        "maxSourceBytes": MAX_SOURCE_BYTES, "maxLineBytes": MAX_LINE_BYTES,
        "maxTotalBytes": 16 * 1024 * 1024, "maxModules": 512, "maxDepth": 64, "maxNames": 4096,
    }))?;
    let mut command = Command::new("node");
    command.arg("--expose-internals");
    for condition in conditions {
        command.arg(format!("--conditions={condition}"));
    }
    command
        .args(["--input-type=commonjs", "--eval", PARSER])
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("could not start Node for CommonJS named-import analysis")?;
    let mut stdin = child
        .stdin
        .take()
        .context("Node parser stdin is unavailable")?;
    let stdout = child
        .stdout
        .take()
        .context("Node parser stdout is unavailable")?;
    let stderr = child
        .stderr
        .take()
        .context("Node parser stderr is unavailable")?;
    let (status, stdout, stderr) = tokio::time::timeout(Duration::from_secs(20), async {
        stdin.write_all(&request).await?;
        drop(stdin);
        tokio::try_join!(
            async { child.wait().await.map_err(anyhow::Error::from) },
            read_limited(stdout, MAX_OUTPUT_BYTES),
            read_limited(stderr, 16 * 1024),
        )
    })
    .await
    .context("Node CommonJS parser timed out after 20 seconds")??;
    if !status.success() {
        bail!(
            "installed Node CommonJS parser is unavailable: {}",
            String::from_utf8_lossy(&stderr).trim()
        );
    }
    let parsed: NamedImports =
        serde_json::from_slice(&stdout).context("invalid Node CommonJS parser response")?;
    if parsed.entries.len() != entries.len() || parsed.node.is_empty() {
        bail!("incomplete Node CommonJS parser response");
    }
    Ok(parsed)
}

async fn discover_named_imports(
    root: &Path,
    entries: Vec<ImportEntry>,
    contexts: &BTreeSet<BTreeSet<String>>,
) -> NamedImports {
    let mut result = NamedImports::default();
    if entries.is_empty() {
        return result;
    }
    for conditions in contexts {
        match run_parser(root, conditions.clone(), entries.clone()).await {
            Ok(parsed) => {
                result.node = parsed.node;
                result.entries.extend(parsed.entries);
            }
            Err(error) => result.errors.push(error.to_string()),
        }
    }
    result
}

pub(crate) async fn discover_named_import_pair(
    old_root: &Path,
    new_root: &Path,
) -> (NamedImports, NamedImports) {
    let prepare = |root: &Path| -> anyhow::Result<_> {
        let root = root.canonicalize()?;
        let groups = requests(&root)?;
        Ok((root, groups))
    };
    let (old, new) = (prepare(old_root), prepare(new_root));
    let (Ok((old_root, old)), Ok((new_root, new))) = (&old, &new) else {
        let failed = |result: anyhow::Result<_>| NamedImports {
            errors: result
                .err()
                .map(|error| error.to_string())
                .into_iter()
                .collect(),
            ..NamedImports::default()
        };
        return (failed(old), failed(new));
    };
    let Ok(contexts) = combined_contexts(old.keys().chain(new.keys())) else {
        return (
            NamedImports {
                errors: vec!["CommonJS condition context limit reached".into()],
                ..NamedImports::default()
            },
            NamedImports::default(),
        );
    };
    tokio::join!(
        discover_named_imports(
            old_root,
            old.values().flatten().cloned().collect(),
            &contexts
        ),
        discover_named_imports(
            new_root,
            new.values().flatten().cloned().collect(),
            &contexts
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_condition_combinations_are_bounded_before_starting_node() {
        let declared: Vec<BTreeSet<String>> = ["browser", "development", "custom", "extra"]
            .into_iter()
            .map(|condition| BTreeSet::from([condition.to_string()]))
            .collect();
        let contexts = combined_contexts(declared[..2].iter()).unwrap();
        assert_eq!(contexts.len(), 4);
        assert!(contexts.contains(&BTreeSet::from(["browser".into(), "development".into(),])));
        assert!(combined_contexts(declared.iter()).is_err());
    }

    #[tokio::test]
    async fn parser_output_limits_preserve_complete_small_responses() {
        assert_eq!(read_limited(&b"names"[..], 5).await.unwrap(), b"names");
        assert!(read_limited(&b"names!"[..], 5).await.is_err());
    }

    #[test]
    fn require_types_and_explicit_esm_do_not_request_commonjs_parsing() {
        let package = tempfile::tempdir().unwrap();
        std::fs::write(package.path().join("index.js"), "export const api=1;").unwrap();
        for exports in [
            serde_json::json!({"require":"./index.js"}),
            serde_json::json!({"types":"./index.js"}),
            serde_json::json!("./index.js"),
        ] {
            std::fs::write(
                package.path().join("package.json"),
                serde_json::json!({"type":"module", "exports":exports}).to_string(),
            )
            .unwrap();
            assert!(requests(&package.path().canonicalize().unwrap())
                .unwrap()
                .is_empty());
        }
    }
}
