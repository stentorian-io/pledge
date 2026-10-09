//! Select JS/TS grammars and cache compiled queries.

use std::path::Path;
use tree_sitter::Query;

pub(crate) fn cached_query(
    language: &tree_sitter::Language,
    source: &'static str,
) -> anyhow::Result<std::sync::Arc<Query>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    type Cache = HashMap<(tree_sitter::Language, &'static str), Result<Arc<Query>, String>>;
    static QUERIES: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut queries = QUERIES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    queries
        .entry((language.clone(), source))
        .or_insert_with(|| {
            Query::new(language, source)
                .map(Arc::new)
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(anyhow::Error::msg)
}

/// Which grammar should be used to parse a source file.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    TypeScript,
    /// INF-01: JSX-bearing TypeScript files (`.tsx`) — routed to
    /// `tree_sitter_typescript::LANGUAGE_TSX`. TSX is a strict superset of
    /// LANGUAGE_TYPESCRIPT so every TypeScript query compiles and matches
    /// identically on TSX source. `.tsx` was routed to
    /// LANGUAGE_TYPESCRIPT and produced ERROR nodes on any JSX body,
    /// suppressing export discovery.
    TypeScriptTsx,
    JavaScript,
}

/// Determine which grammar to use based on the file extension.
///
/// Returns `None` for unsupported extensions so callers can emit a warning and
/// skip the file rather than hard-failing.
#[allow(dead_code)]
pub fn language_for_file(path: &Path) -> Option<SourceLanguage> {
    let ext = path.extension()?.to_str()?;
    // Check for compound extensions like `.d.ts`, `.d.mts`, `.d.cts`.
    // Declaration files are pure type surface — no JSX — routed to standard
    // TypeScript grammar (NOT TypeScriptTsx).
    let file_name = path.file_name()?.to_str().unwrap_or("");
    if file_name.ends_with(".d.ts")
        || file_name.ends_with(".d.mts")
        || file_name.ends_with(".d.cts")
    {
        return Some(SourceLanguage::TypeScript);
    }
    match ext {
        // INF-01: `.tsx` routes to LANGUAGE_TSX — a strict superset
        // of LANGUAGE_TYPESCRIPT that handles JSX expression syntax.
        "tsx" => Some(SourceLanguage::TypeScriptTsx),
        "ts" | "mts" | "cts" => Some(SourceLanguage::TypeScript),
        "js" | "mjs" | "cjs" | "jsx" => Some(SourceLanguage::JavaScript),
        _ => None,
    }
}

/// Return the tree-sitter `Language` handle for the given `SourceLanguage`.
///
/// Uses the non-deprecated `LANGUAGE_TYPESCRIPT` / `LANGUAGE_TSX` / `LANGUAGE` constants.
///
/// made `pub(super)` so `comment_scan.rs` can call it.
pub(crate) fn get_language(lang: &SourceLanguage) -> tree_sitter::Language {
    match lang {
        SourceLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    }
}
