use crate::ecosystems::node::compare as package_compare;
use crate::ecosystems::node::compare::imports::ImportsMap;
use crate::ecosystems::node::discovery::entry_points;
use crate::languages::javascript::model::{
    DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, SymbolSpace,
};
use crate::languages::javascript::syntax::exports::{parse_exports, RawExportKind};
use crate::languages::javascript::syntax::grammar::language_for_file;
use crate::languages::javascript::syntax::{commonjs, exports as export_syntax, grammar};
use crate::languages::typescript::syntax::comments::CommentIndex;
use crate::languages::typescript::syntax::{comments, types as type_syntax};
use crate::languages::typescript::types::{parser as type_parser, shape as type_shapes};
use crate::languages::typescript::{config as tsconfig, model as ts_model};
use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Map each declaration kind to its export symbol space:
/// - interface/type alias -> Type
/// - namespace -> Namespace
/// - class/enum/function/variable/default/unknown -> Value
#[allow(dead_code)]
fn space_for(kind: Option<&DeclarationKind>) -> SymbolSpace {
    match kind {
        Some(DeclarationKind::Interface) | Some(DeclarationKind::TypeAlias) => SymbolSpace::Type,
        Some(DeclarationKind::Namespace) => SymbolSpace::Namespace,
        _ => SymbolSpace::Value,
    }
}

///  +: merge an incoming raw export into an existing symbol at the
/// same ExportKey. Used by every insertion call site to handle declaration-
/// merging hybrids (class+namespace, function+namespace, interface+namespace)
/// at discovery time, eliminating the REF-05 or_insert race by construction
///.
///
/// Rules:
/// - Union decl_kinds (dedup — if kind already present, skip).
/// - Keep the EARLIER file/line/col for diagnostic stability (first-write-wins
///   on position).
/// - Upgrade None/false scalar fields (type_signature, release_tag) from the
///   incoming when the existing is None — incoming-enriches-existing
///   semantics.
#[allow(dead_code)]
fn merge_hybrid(
    existing: &mut ExportSymbol,
    incoming_kind: Option<DeclarationKind>,
    incoming_type_signature: Option<String>,
    incoming_release_tag: Option<ts_model::ReleaseTag>,
) {
    if let Some(k) = incoming_kind {
        if !existing.decl_kinds.contains(&k) {
            existing.decl_kinds.push(k);
        }
    }
    if existing.type_signature.is_none() && incoming_type_signature.is_some() {
        existing.type_signature = incoming_type_signature;
    }
    if existing.release_tag.is_none() && incoming_release_tag.is_some() {
        existing.release_tag = incoming_release_tag;
    }
    // file/line/col intentionally NOT updated — keep earliest for stability.
}

/// Parse the `imports` field from `pkg_root/package.json` into an ImportsMap.
///
/// Returns None if package.json is unreadable, unparseable, or has no
/// `imports` field. Fail-soft via `.ok()?` chain (mirrors parse_tsconfig_paths).
///
/// Per CLAUDE.md project note: package.json is strict JSON, no JSONC handling.
#[allow(dead_code)]
pub fn parse_imports_map_for_pkg(pkg_root: &Path) -> Option<ImportsMap> {
    let pkg_path = pkg_root.join("package.json");
    let content = std::fs::read_to_string(&pkg_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let imports = json.get("imports")?;
    Some(package_compare::imports::parse_imports_map(imports))
}

/// Resolve a `#`-prefixed specifier through the parsed imports map.
///
/// EXP-14. Returns the on-disk path of the first condition leaf
/// that classifies as Relative or Bare (with extension probing) — skipping
/// leaves whose target classifies as NodeBuiltin / UrlScheme / another
/// ImportsField (`imports` values may legally name `node:` /
/// URL targets which are TERMINAL — no source to recurse into).
///
/// Lookup order:
/// 1. Direct exact-key match.
/// 2. Wildcard pattern fallback (e.g. `#i/*.js` matches `#i/foo.js`).
///
/// Returns None if the specifier is not in the map, no condition leaf
/// resolves to an existing on-disk file, or every leaf is a non-file target.
#[allow(dead_code)]
pub(crate) fn resolve_imports_subpath(
    imports_map: &ImportsMap,
    specifier: &str,
    pkg_root: &Path,
    condition: Option<&str>,
) -> Option<PathBuf> {
    use crate::ecosystems::node::compare::exports::{select_condition, SubpathEntry};
    let mut active: std::collections::BTreeSet<String> = condition
        .into_iter()
        .flat_map(|s| s.split('/'))
        .map(str::to_string)
        .collect();
    if !active.contains("browser") {
        active.insert("node".into());
    }
    if !active.contains("require") {
        active.insert("import".into());
    }

    /// Try a target path string under pkg_root, probing common extensions if
    /// the literal path doesn't exist on disk. Mirrors the extension-probing
    /// strategy of resolve_tsconfig_alias.
    fn probe_target(pkg_root: &Path, target: &str) -> Option<PathBuf> {
        // Strip leading "./" so join produces a clean path.
        let trimmed = target.trim_start_matches("./");
        let base = pkg_root.join(trimmed);
        if base.exists() {
            return Some(base);
        }
        for ext in &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"] {
            let candidate = base.with_extension(ext);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        // Directory index fallback.
        for idx in &[
            "index.ts",
            "index.tsx",
            "index.js",
            "index.mjs",
            "index.cjs",
        ] {
            let candidate = base.join(idx);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    /// Classify a target string and return its on-disk path if it resolves to
    /// a real file. Skip NodeBuiltin / UrlScheme / nested ImportsField targets.
    fn try_target(target: &str, pkg_root: &Path) -> Option<PathBuf> {
        match classify_specifier(target) {
            SpecifierKind::NodeBuiltin | SpecifierKind::UrlScheme | SpecifierKind::ImportsField => {
                None
            }
            SpecifierKind::Relative | SpecifierKind::Bare => probe_target(pkg_root, target),
        }
    }

    if let Some(SubpathEntry { node }) = imports_map.get(specifier) {
        return select_condition(node, &active)
            .flatten()
            .and_then(|t| try_target(t, pkg_root));
    }

    let mut patterns: Vec<_> = imports_map
        .iter()
        .filter_map(|(key, entry)| {
            let (prefix, suffix) = key.split_once('*')?;
            let slot = specifier.strip_prefix(prefix)?.strip_suffix(suffix)?;
            Some((prefix.len(), key.len(), slot, entry))
        })
        .collect();
    patterns.sort_by_key(|(prefix, len, _, _)| std::cmp::Reverse((*prefix, *len)));
    if let Some((_, _, slot, entry)) = patterns.first() {
        return select_condition(&entry.node, &active)
            .flatten()
            .and_then(|target| try_target(&target.replace('*', slot), pkg_root));
    }

    None
}

/// Maximum number of re-export hops before aborting a resolution chain.
#[allow(dead_code)]
const MAX_RESOLVE_DEPTH: usize = 20;

/// CHUNK-01: Normalize hashed bundler chunk filenames so that rollup/vite rebuilds
/// produce stable paths for diff comparison.
///
/// Applies only when the path contains a `/chunks/` directory segment AND the
/// basename matches `chunk-[A-Z0-9]{8,}.<ext>`. Example:
///  `dist/chunks/chunk-A1B2C3D4.js` → `dist/chunks/chunk-*.js`
fn normalize_chunk_path(path: &Path) -> PathBuf {
    //: normalize separators
    // BEFORE every probe so a Windows-style `dist\chunks\chunk-HASH.js`
    // path produces the same normalized result as the forward-slash
    // form. Without this the `/chunks/` substring probe would miss
    // backslash inputs even though we have a `\\chunks\\` branch — the
    // `parent.join(normalized_name)` step at the bottom would still
    // splice the original separator back in, breaking the round-trip
    // path-equality invariant exercised by
    // `chunk_directives_keyed_by_normalized_path`.
    let path_str_lossy = path.to_string_lossy();
    let canonical = path_str_lossy.replace('\\', "/");
    let in_chunks_dir = canonical.contains("/chunks/");
    if !in_chunks_dir {
        return path.to_path_buf();
    }
    // Extract the file_name from the CANONICAL (forward-slash) string so that
    // on Unix a Windows-style path like `dist\chunks\chunk-HASH.js` (treated
    // as a single component by the OS) still yields `chunk-HASH.js` here.
    let file_name = canonical.rsplit('/').next().unwrap_or("");
    if !file_name.starts_with("chunk-") {
        return path.to_path_buf();
    }
    if let Some((stem_part, ext_part)) = file_name.rsplit_once('.') {
        if let Some(dash_idx) = stem_part.rfind('-') {
            let rest = &stem_part[dash_idx + 1..];
            let looks_like_hash = rest.len() >= 8
                && rest
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || c.is_ascii_uppercase());
            if looks_like_hash {
                let normalized_name = format!("chunk-*.{}", ext_part);
                // Build the normalized path FROM the canonicalized string
                // so the parent component uses `/` regardless of the input
                // separator. PathBuf is OS-agnostic about its components;
                // string-level construction guarantees byte-equality of the
                // result across `/`- and `\`-separator inputs.
                if let Some((parent_str, _)) = canonical.rsplit_once('/') {
                    return PathBuf::from(format!("{}/{}", parent_str, normalized_name));
                }
                return PathBuf::from(normalized_name);
            }
        }
    }
    path.to_path_buf()
}

/// A non-fatal warning emitted during export resolution.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ResolveWarning {
    pub message: String,
    pub incomplete: bool,
    /// A complete compiler pass can supersede a source type expansion limit.
    pub type_expansion_only: bool,
}

impl ResolveWarning {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            incomplete: true,
            type_expansion_only: false,
        }
    }
}

/// Classification of a re-export specifier — drives whether the resolver
/// attempts filesystem resolution, routes through the package.json `imports`
/// map, or silently skips.
///
/// RES-01: `node:`-prefixed builtins are not paths and must not be
/// probed against the filesystem nor inserted as synthetic external entries.
/// RES-02: URL-scheme specifiers (`data:`, `https:`, `http:`, `file:`) are
/// not paths under our static-analysis model and are filtered identically.
/// EXP-14: `#`-prefixed specifiers route through
/// the package's own `imports` field rather than the filesystem.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpecifierKind {
    /// `node:fs`, `node:path`, `node:test`,...
    NodeBuiltin,
    /// `data:`, `https:`, `http:`, `file:` — URL-scheme imports.
    UrlScheme,
    /// `#subpath` — resolved through package.json `imports` field (EXP-14).
    ImportsField,
    /// `./`, `../`, `/` — filesystem-relative or absolute path.
    Relative,
    /// External package: `react`, `@scope/pkg`,... — anything not above.
    Bare,
}

/// Classify a re-export specifier string. Pure function, no I/O.
///
/// Order of checks matters: `node:` and URL schemes are checked first so
/// that a hypothetical relative path like `./node-something` (which starts
/// with `.`) is correctly classified as `Relative`, not `NodeBuiltin`.
#[allow(dead_code)]
pub(crate) fn classify_specifier(spec: &str) -> SpecifierKind {
    if spec.starts_with("node:") {
        return SpecifierKind::NodeBuiltin;
    }
    if spec.starts_with("data:")
        || spec.starts_with("https:")
        || spec.starts_with("http:")
        || spec.starts_with("file:")
    {
        return SpecifierKind::UrlScheme;
    }
    if spec.starts_with('#') {
        return SpecifierKind::ImportsField;
    }
    if spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/') {
        return SpecifierKind::Relative;
    }
    SpecifierKind::Bare
}

#[cfg(test)]
mod specifier_classifier_tests {
    use crate::ecosystems::node::discovery::resolver::{classify_specifier, SpecifierKind};

    #[test]
    fn node_prefix_is_node_builtin() {
        assert_eq!(classify_specifier("node:fs"), SpecifierKind::NodeBuiltin);
        assert_eq!(classify_specifier("node:path"), SpecifierKind::NodeBuiltin);
        assert_eq!(classify_specifier("node:test"), SpecifierKind::NodeBuiltin);
        assert_eq!(classify_specifier("node:"), SpecifierKind::NodeBuiltin);
    }

    #[test]
    fn url_schemes_are_url_scheme() {
        assert_eq!(
            classify_specifier("data:application/json,{}"),
            SpecifierKind::UrlScheme
        );
        assert_eq!(
            classify_specifier("https://example.com/m.js"),
            SpecifierKind::UrlScheme
        );
        assert_eq!(
            classify_specifier("http://example.com/m.js"),
            SpecifierKind::UrlScheme
        );
        assert_eq!(
            classify_specifier("file:///abs/path.js"),
            SpecifierKind::UrlScheme
        );
    }

    #[test]
    fn hash_prefix_is_imports_field() {
        assert_eq!(classify_specifier("#internal"), SpecifierKind::ImportsField);
        assert_eq!(classify_specifier("#"), SpecifierKind::ImportsField);
        assert_eq!(classify_specifier("#dep/sub"), SpecifierKind::ImportsField);
    }

    #[test]
    fn dot_or_slash_is_relative() {
        assert_eq!(classify_specifier("./foo"), SpecifierKind::Relative);
        assert_eq!(classify_specifier("../foo"), SpecifierKind::Relative);
        assert_eq!(classify_specifier("/abs/foo"), SpecifierKind::Relative);
    }

    #[test]
    fn relative_path_with_node_prefix_in_name_is_relative_not_builtin() {
        // Pitfall: `./node-utils.js` starts with `.` — must classify as Relative,
        // never as NodeBuiltin (the leading `.` check fires first because
        // `node:` requires the COLON, not just the four characters).
        assert_eq!(
            classify_specifier("./node-utils.js"),
            SpecifierKind::Relative
        );
        assert_eq!(classify_specifier("../nodey/foo"), SpecifierKind::Relative);
    }

    #[test]
    fn bare_packages_are_bare() {
        assert_eq!(classify_specifier("react"), SpecifierKind::Bare);
        assert_eq!(classify_specifier("@scope/pkg"), SpecifierKind::Bare);
        assert_eq!(classify_specifier(""), SpecifierKind::Bare);
        // Bare specifiers whose name happens to begin with "node" but no colon:
        assert_eq!(classify_specifier("nodemon"), SpecifierKind::Bare);
    }
}

/// Recursively collect all exports reachable from `file` into `map`.
///
/// - `pkg_root` — absolute path to the package root; used to compute relative
///   paths stored in `ExportSymbol.file`.
/// - `visited` — set of canonicalized absolute paths already processed; used
///   for cycle detection.
/// - `warnings` — non-fatal issues encountered during resolution.
/// - `depth` — current recursion depth; aborted at `MAX_RESOLVE_DEPTH`.
#[allow(dead_code, clippy::too_many_arguments)]
pub fn collect_exports(
    file: &Path,
    pkg_root: &Path,
    map: &mut ExportMap,
    visited: &mut HashSet<PathBuf>,
    warnings: &mut Vec<ResolveWarning>,
    depth: usize,
    tsconfig: Option<&tsconfig::TsconfigPaths>,
    imports_map: Option<&ImportsMap>,
    subpath: &str,
    condition: Option<&str>,
) -> anyhow::Result<()> {
    collect_exports_cached(
        file,
        pkg_root,
        map,
        visited,
        warnings,
        depth,
        tsconfig,
        imports_map,
        subpath,
        condition,
        &mut ParseCache::default(),
    )
}

#[derive(Default)]
pub(crate) struct ParseCache {
    resolved: HashMap<(PathBuf, String, Option<String>), ExportMap>,
    namespace_exports: HashMap<(PathBuf, String, Option<String>), HashSet<String>>,
    files: HashMap<PathBuf, Result<ParsedSource, String>>,
}

#[derive(Clone)]
struct ParsedSource {
    exports: Vec<export_syntax::RawExport>,
    directives: Vec<comments::DirectiveRecord>,
    forwards: Vec<commonjs::Forward>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_exports_cached(
    file: &Path,
    pkg_root: &Path,
    map: &mut ExportMap,
    visited: &mut HashSet<PathBuf>,
    warnings: &mut Vec<ResolveWarning>,
    depth: usize,
    tsconfig: Option<&tsconfig::TsconfigPaths>,
    imports_map: Option<&ImportsMap>,
    subpath: &str,
    condition: Option<&str>,
    cache: &mut ParseCache,
) -> anyhow::Result<()> {
    let canonical = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let key = (
        canonical.clone(),
        subpath.to_string(),
        condition.map(str::to_string),
    );
    if !visited.contains(&canonical) {
        if let Some(resolved) = cache.resolved.get(&key) {
            map.extend(
                resolved
                    .iter()
                    .map(|(key, symbol)| (key.clone(), symbol.clone())),
            );
            map.directives.extend(resolved.directives.clone());
            return Ok(());
        }
    }
    let mut resolved = ExportMap::new();
    let warning_count = warnings.len();
    collect_exports_uncached(
        file,
        pkg_root,
        &mut resolved,
        visited,
        warnings,
        depth,
        tsconfig,
        imports_map,
        subpath,
        condition,
        cache,
    )?;
    // Cyclic or incomplete results depend on the active path and cannot be reused.
    if warning_count == warnings.len() {
        cache.resolved.insert(key, resolved.clone());
    }
    map.extend(
        resolved
            .iter()
            .map(|(key, symbol)| (key.clone(), symbol.clone())),
    );
    map.directives.extend(resolved.directives);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_exports_uncached(
    file: &Path,
    pkg_root: &Path,
    map: &mut ExportMap,
    visited: &mut HashSet<PathBuf>,
    warnings: &mut Vec<ResolveWarning>,
    depth: usize,
    tsconfig: Option<&tsconfig::TsconfigPaths>,
    imports_map: Option<&ImportsMap>,
    subpath: &str,
    condition: Option<&str>,
    cache: &mut ParseCache,
) -> anyhow::Result<()> {
    if depth > MAX_RESOLVE_DEPTH {
        warnings.push(ResolveWarning::new(format!(
            "maximum resolution depth ({MAX_RESOLVE_DEPTH}) reached at {}",
            file.display()
        )));
        return Ok(());
    }

    if !file.exists() {
        warnings.push(ResolveWarning::new(format!(
            "file not found: {}, skipping",
            file.display()
        )));
        return Ok(());
    }

    let canonical = file
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", file.display()))?;

    let mut active = visited.clone();
    let visited = &mut active;
    if !visited.insert(canonical.clone()) {
        warnings.push(ResolveWarning {
            message: format!(
                "circular re-export detected at {}, skipping",
                file.display()
            ),
            incomplete: false,
            type_expansion_only: false,
        });
        return Ok(());
    }

    let ParsedSource {
        exports: mut raw_exports,
        directives: file_directives,
        forwards,
    } = if let Some(parsed) = cache.files.get(&canonical) {
        match parsed {
            Ok(parsed) => parsed.clone(),
            Err(message) => {
                warnings.push(ResolveWarning::new(message.clone()));
                return Ok(());
            }
        }
    } else {
        let lang = match language_for_file(file) {
            Some(l) => l,
            None => {
                warnings.push(ResolveWarning::new(format!(
                    "unsupported file extension: {}",
                    file.display()
                )));
                return Ok(());
            }
        };

        // ROB-01: Read the file bytes, then validate UTF-8 before passing to the parser.
        // tree-sitter accepts raw bytes but subsequent processing expects valid UTF-8.
        // Invalid UTF-8 (e.g., from binary blobs accidentally named.ts) is treated as
        // a per-file warning rather than aborting analysis on remaining files.
        let source = match std::fs::read(&canonical) {
            Ok(bytes) => bytes,
            Err(e) => {
                warnings.push(ResolveWarning::new(format!(
                    "failed to read {}: {} \u{2014} skipping file",
                    canonical.display(),
                    e
                )));
                return Ok(());
            }
        };

        if let Some(reason) = crate::core::source::classify(&source) {
            warnings.push(ResolveWarning::new(format!(
                "analysis skipped {}: {} source exceeds processing limits",
                canonical.display(),
                reason.label()
            )));
            return Ok(());
        }

        // ROB-01: Validate UTF-8 explicitly. tree-sitter can technically handle arbitrary
        // bytes, but a file that is not valid UTF-8 is almost certainly not a real JS/TS
        // source file; treat it as a parse failure and emit a warning.
        if std::str::from_utf8(&source).is_err() {
            warnings.push(ResolveWarning::new(format!(
                "parse failed for {}: invalid UTF-8 encoding \u{2014} skipping file",
                canonical.display()
            )));
            return Ok(());
        }

        // ROB-01: parse_exports Err is treated as a per-file warning rather than a
        // global error — this keeps analysis running on remaining files.
        let mut raw_exports = match parse_exports(&source, lang) {
            Ok(rx) => rx,
            Err(e) => {
                let message = format!(
                    "parse failed for {}: {} \u{2014} skipping file",
                    canonical.display(),
                    e
                );
                cache.files.insert(canonical.clone(), Err(message.clone()));
                warnings.push(ResolveWarning::new(message));
                return Ok(());
            }
        };

        // META-06: backfill release_tag on every RawExport uniformly.
        // Single populate pass using the CommentIndex sidecar — avoids editing
        // every RawExport construction site. Existing populated tags from
        // declaration and ambient-declaration export queries are preserved
        // via the `is_none()` guard.
        //
        // The comment_scan pass re-parses the source to own a Tree; tree-sitter
        // parsing is iterative and fast so the duplicate parse is acceptable
        // for (future optimization: thread the Tree from parse_exports).
        // INF-05: collect directive records from the SAME CommentIndex
        // we use for META-06. Captured here as a local Vec so we can insert into
        // the ExportMap.directives sidecar once `rel_path` is computed below.
        let mut forwards = Vec::new();
        let mut file_directives: Vec<comments::DirectiveRecord> = Vec::new();
        {
            let language = grammar::get_language(&lang);
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&language).is_ok() {
                if let Some(tree) = parser.parse(&source, None) {
                    let commonjs = commonjs::analyze(tree.root_node(), &source);
                    forwards = commonjs.forwards;
                    for line in commonjs.mutation_lines {
                        warnings.push(ResolveWarning::new(format!(
                            "CommonJS export mutation requires runtime information at {}:{line}",
                            canonical.display()
                        )));
                    }
                    for (node, key) in commonjs::computed_exports(tree.root_node(), &source) {
                        if key.is_none() {
                            warnings.push(ResolveWarning::new(format!(
                                "unresolved computed CommonJS export key at {}:{}",
                                canonical.display(),
                                node.start_position().row + 1
                            )));
                        }
                    }
                    let comment_index = CommentIndex::build(&tree, &source, lang);
                    for rx in raw_exports.iter_mut() {
                        if rx.release_tag.is_none() {
                            rx.release_tag =
                                comment_index.release_tag_for_export_at(rx.line, rx.col);
                        }
                    }
                    // INF-05: extract triple-slash directives (honors the TS
                    // compiler position rule: only directives at the top of the
                    // file, before any non-comment statement, are recognized).
                    file_directives = comment_index.extract_directives();

                    // TYPE-01: backfill satisfies_type on eligible
                    // RawExports. Mirrors the META-06 populate pass directly above
                    // same tree/source pair, single post-parse scan, no need to
                    // edit individual RawExport construction sites.
                    //
                    // Grammar gate: `satisfies_expression` is a TypeScript AST node
                    // only; `extract_satisfies_types` internally returns an empty
                    // map on JavaScript inputs so the loop is a no-op there.
                    let satisfies_map =
                        type_syntax::extract_satisfies_types(tree.root_node(), &source, lang);
                    if !satisfies_map.is_empty() {
                        for rx in raw_exports.iter_mut() {
                            if rx.satisfies_type.is_some() {
                                continue;
                            }
                            if let Some(satisfies_text) = satisfies_map.get(&rx.name) {
                                let constraint = type_parser::parse_type_string(satisfies_text);
                                rx.satisfies_type = Some(type_shapes::TypeShape::Satisfies {
                                    constraint: Box::new(constraint),
                                    narrowed: None,
                                });
                            }
                        }
                    }

                    // INF-04: walk the tree once for `typeof import('./x').Y`
                    // / `import('./x').Y` member-expression shapes. Bucket each ref to
                    // its owning RawExport by the greatest export line <= ref line.
                    // Both single-line `export type X = typeof import('./y').Z;` and
                    // multi-line declarations are handled — the bucketing is by line
                    // ordering rather than line equality.
                    let all_typeof_refs = type_syntax::extract_typeof_import_refs(
                        tree.root_node(),
                        source.as_slice(),
                    );
                    if !all_typeof_refs.is_empty() {
                        let mut export_lines: Vec<(u32, usize)> = raw_exports
                            .iter()
                            .enumerate()
                            .map(|(i, rx)| (rx.line, i))
                            .collect();
                        export_lines.sort_by_key(|(line, _)| *line);
                        for r in &all_typeof_refs {
                            let owner_idx = export_lines
                                .iter()
                                .rev()
                                .find(|(line, _)| *line <= r.property_line)
                                .map(|(_, idx)| *idx);
                            if let Some(idx) = owner_idx {
                                raw_exports[idx].typeof_import_refs.push(r.clone());
                            }
                        }
                    }
                }
            }
        }

        let parsed = ParsedSource {
            exports: raw_exports,
            directives: file_directives,
            forwards,
        };
        cache.files.insert(canonical.clone(), Ok(parsed.clone()));
        parsed
    };

    // Compute the file path relative to pkg_root for storage in ExportSymbol.
    // CHUNK-01: Normalize hashed chunk filenames (e.g., chunk-A1B2C3D4.js → chunk-*.js)
    // so that rollup/vite rebuilds produce stable paths for diff comparison.
    let rel_path = canonical
        .strip_prefix(pkg_root)
        .unwrap_or(&canonical)
        .to_path_buf();

    // INF-05: flow extracted directives into the ExportMap sidecar.
    // Key by the rel_path so compare_directives can pair old/new directive
    // lists per-file. Files with no directives are absent from the map.
    if !file_directives.is_empty() {
        map.directives
            .insert(normalize_chunk_path(&rel_path), file_directives);
    }

    for forward in &forwards {
        let root_aggregate = raw_exports.iter().any(|export| {
            export.kind == RawExportKind::Default
                && forward.value_position.row as u32 + 1 == export.line
                && forward.value_position.column as u32 + 1 == export.col
        });
        if forward.uncertain {
            warnings.push(ResolveWarning::new(format!(
                "unsupported CommonJS branch predicate at {}:{}",
                canonical.display(),
                forward.line
            )));
        }
        let context = match (&forward.condition, condition) {
            (Some(branch), Some(parent)) => Some(format!("{parent}/cjs:{branch}")),
            (Some(branch), None) => Some(format!("cjs:{branch}")),
            (None, parent) => parent.map(str::to_string),
        };
        let target = if classify_specifier(&forward.specifier) == SpecifierKind::Bare {
            resolve_package_path(&canonical, &forward.specifier, Some("require"))
        } else {
            resolve_module_path(&canonical, &forward.specifier)
                .into_iter()
                .find(|path| path.is_file())
        };
        if let Some(target) = target {
            let mut resolved = ExportMap::new();
            collect_exports_cached(
                &target,
                pkg_root,
                &mut resolved,
                visited,
                warnings,
                depth + 1,
                tsconfig,
                imports_map,
                subpath,
                context.as_deref(),
                cache,
            )?;
            for (key, mut symbol) in resolved.drain() {
                if forward.shadowed.contains(&key.name)
                    || (forward.kind != commonjs::ForwardKind::WholeModule
                        && symbol.kind == ExportKind::Default)
                    || (forward.kind == commonjs::ForwardKind::StarProperties
                        && key.name == "default")
                    || (forward.kind == commonjs::ForwardKind::Properties
                        && root_aggregate
                        && key.name == "default")
                {
                    continue;
                }
                symbol.is_reexport = true;
                map.insert(key, symbol);
            }
        } else {
            warnings.push(ResolveWarning::new(format!(
                "cannot resolve CommonJS forwarding target `{}` from {}",
                forward.specifier,
                canonical.display()
            )));
        }
    }
    raw_exports.retain(|export| {
        !(export.kind == RawExportKind::Default
            && forwards.iter().any(|forward| {
                forward.kind == commonjs::ForwardKind::WholeModule
                    && forward.value_position.row as u32 + 1 == export.line
                    && forward.value_position.column as u32 + 1 == export.col
            }))
    });

    // META-05: Accumulate named re-export failures keyed by specifier so we can
    // aggregate them into a single "barrel" warning when ≥3 from the same barrel
    // file are unresolvable (at depth 0 only).
    let mut barrel_failures: HashMap<String, Vec<String>> = HashMap::new();

    // Explicit exports take precedence regardless of their source order.
    raw_exports.sort_by_key(|e| matches!(e.kind, RawExportKind::StarReexport));
    let explicit: HashSet<String> = raw_exports
        .iter()
        .filter(|e| e.kind != RawExportKind::StarReexport)
        .map(|e| {
            if e.kind == RawExportKind::Default {
                "default".to_string()
            } else {
                e.name.clone()
            }
        })
        .collect();
    let mut star_origins: HashMap<ExportKey, (PathBuf, u32, u32)> = HashMap::new();
    let mut ambiguous = HashSet::new();
    let mut reexport_cache = HashMap::new();
    let namespace_key = (
        canonical.clone(),
        subpath.to_string(),
        condition.map(str::to_string),
    );
    // A dotted public name alone does not establish a namespace binding.
    cache.namespace_exports.insert(
        namespace_key.clone(),
        raw_exports
            .iter()
            .filter(|export| export.kind == RawExportKind::StarReexport && export.name != "*")
            .map(|export| export.name.clone())
            .collect(),
    );

    for export in raw_exports {
        if matches!(
            export.kind,
            RawExportKind::StarReexport | RawExportKind::NamedReexport
        ) {
            let target = export.source.as_deref().and_then(|specifier| {
                if classify_specifier(specifier) == SpecifierKind::ImportsField {
                    let alias = uses_typescript_resolution(&canonical)
                        .then(|| {
                            tsconfig.and_then(|config| {
                                resolve_config_alias(specifier, config, pkg_root)
                                    .into_iter()
                                    .find(|path| path.is_file())
                            })
                        })
                        .flatten();
                    alias.or_else(|| {
                        imports_map.and_then(|im| {
                            resolve_imports_subpath(im, specifier, pkg_root, condition)
                        })
                    })
                } else if matches!(
                    classify_specifier(specifier),
                    SpecifierKind::Relative | SpecifierKind::Bare
                ) {
                    let paths = source_resolution_paths(&canonical, specifier, tsconfig, pkg_root);
                    paths
                        .into_iter()
                        .find(|p| p.is_file())
                        .or_else(|| resolve_package_path(&canonical, specifier, condition))
                } else {
                    None
                }
            });
            if let Some(target) = target {
                if !reexport_cache.contains_key(&target) {
                    let mut resolved = ExportMap::new();
                    collect_exports_cached(
                        &target,
                        pkg_root,
                        &mut resolved,
                        visited,
                        warnings,
                        depth + 1,
                        tsconfig,
                        imports_map,
                        subpath,
                        condition,
                        cache,
                    )?;
                    reexport_cache.insert(target.clone(), resolved);
                }
                let resolved = &reexport_cache[&target];
                map.directives.extend(resolved.directives.clone());
                let imported_name = export.local_name.as_deref().unwrap_or(&export.name);
                let target_namespace_key = (
                    target.canonicalize().unwrap_or_else(|_| target.clone()),
                    subpath.to_string(),
                    condition.map(str::to_string),
                );
                let target_namespaces = cache
                    .namespace_exports
                    .get(&target_namespace_key)
                    .cloned()
                    .unwrap_or_default();
                let forwards_namespace = export.kind == RawExportKind::NamedReexport
                    && target_namespaces.contains(imported_name);
                if forwards_namespace {
                    cache
                        .namespace_exports
                        .entry(namespace_key.clone())
                        .or_default()
                        .insert(export.name.clone());
                } else if export.kind == RawExportKind::StarReexport && export.name == "*" {
                    cache
                        .namespace_exports
                        .entry(namespace_key.clone())
                        .or_default()
                        .extend(
                            target_namespaces
                                .into_iter()
                                .filter(|name| name != "default" && !explicit.contains(name)),
                        );
                }
                let mut found = false;
                for (resolved_key, resolved_symbol) in resolved.iter() {
                    if export.kind == RawExportKind::NamedReexport
                        && resolved_key.name != imported_name
                        && !(forwards_namespace
                            && resolved_key
                                .name
                                .strip_prefix(imported_name)
                                .is_some_and(|suffix| suffix.starts_with('.')))
                    {
                        continue;
                    }
                    let mut key = resolved_key.clone();
                    let mut symbol = resolved_symbol.clone();
                    if export.kind == RawExportKind::NamedReexport {
                        found = true;
                        symbol.local_name = Some(symbol.local_name.unwrap_or(symbol.name));
                        symbol.name =
                            format!("{}{}", export.name, &key.name[imported_name.len()..]);
                        symbol.kind = if symbol.name == "default" {
                            ExportKind::Default
                        } else {
                            ExportKind::Named
                        };
                        key.name = symbol.name.clone();
                    } else if export.name == "*" {
                        if key.name == "default" || explicit.contains(&key.name) {
                            continue;
                        }
                    } else {
                        symbol.local_name = Some(symbol.local_name.unwrap_or(symbol.name.clone()));
                        key.name = format!("{}.{}", export.name, key.name);
                        symbol.name = key.name.clone();
                    }
                    symbol.is_reexport = true;
                    if export.release_tag.is_some() {
                        symbol.release_tag = export.release_tag.clone();
                    }
                    symbol.is_type_only |= export.is_type_only;
                    if symbol.is_type_only {
                        let mut type_key = resolved_key.clone();
                        type_key.space = SymbolSpace::Type;
                        // A type-only import can use both Brand and typeof Brand.
                        // Keep its value declaration distinct from a same-name type alias.
                        if resolved_key.space != SymbolSpace::Value
                            || !resolved.contains_key(&type_key)
                        {
                            key.space = SymbolSpace::Type;
                        }
                    }
                    if export.kind == RawExportKind::StarReexport {
                        let identity = (symbol.file.clone(), symbol.line, symbol.col);
                        if let Some(previous) = star_origins.get(&key) {
                            if previous != &identity {
                                ambiguous.insert(key.clone());
                                map.remove(&key);
                            }
                        } else {
                            star_origins.insert(key.clone(), identity);
                        }
                        if ambiguous.contains(&key) {
                            continue;
                        }
                    }
                    map.entry(key).or_insert(symbol);
                }
                if export.kind == RawExportKind::NamedReexport && !found {
                    barrel_failures
                        .entry(export.source.clone().unwrap_or_default())
                        .or_default()
                        .push(export.name);
                }
                continue;
            }
        }
        match export.kind {
            RawExportKind::Named | RawExportKind::Symbol => {
                //: pre-merge at insertion. Construct ExportKey with
                // current subpath/condition context, then branch on get_mut to
                // union decl_kinds for declaration-merging hybrids.
                let space = if export.kind == RawExportKind::Symbol {
                    SymbolSpace::Symbol
                } else {
                    space_for(export.decl_kind.as_ref())
                };
                let key = ExportKey {
                    subpath: subpath.to_string(),
                    condition: condition.map(|c| c.to_string()),
                    space,
                    name: export.name.clone(),
                };
                if let Some(existing) = map.get_mut(&key) {
                    merge_hybrid(
                        existing,
                        export.decl_kind.clone(),
                        // INF-02: forward declare-site type signature
                        // through the merge path as well — if the first-seen
                        // symbol was typeless and a later declaration-merge
                        // partner carries the declare type, the merged symbol
                        // adopts it per merge_hybrid's None->Some upgrade rule.
                        export.type_signature.clone(),
                        export.release_tag.clone(),
                    );
                    // ARCH-02: merge compound_members into existing symbol
                    // (dedup to avoid duplicates from multiple discovery passes).
                    for m in &export.compound_members {
                        if !existing.compound_members.iter().any(|em| em == m) {
                            existing.compound_members.push(m.clone());
                        }
                    }
                    //: also merge typeof_import_refs to
                    // mirror the compound_members handling above. Without this,
                    // a later declaration-merge partner carrying typeof refs
                    // would silently lose them. Idempotent dedup on the four
                    // identity fields (specifier + property + line/col).
                    for r in export.typeof_import_refs.iter() {
                        if !existing.typeof_import_refs.iter().any(|er| {
                            er.specifier == r.specifier
                                && er.property == r.property
                                && er.property_line == r.property_line
                                && er.property_col == r.property_col
                        }) {
                            existing.typeof_import_refs.push(r.clone());
                        }
                    }
                } else {
                    map.insert(
                        key,
                        ExportSymbol {
                            name: export.name,
                            kind: ExportKind::Named,
                            file: rel_path.clone(),
                            line: export.declaration_site.map_or(export.line, |site| site.0),
                            col: export.declaration_site.map_or(export.col, |site| site.1),
                            // INF-02: carry the declare-site type
                            // signature linked during parse_exports' INF-02
                            // link-pass. None for source-form exports where
                            // no ambient declare preceded the export specifier.
                            type_signature: export.type_signature,
                            overloads: Vec::new(),
                            decl_kinds: export.decl_kind.map(|k| vec![k]).unwrap_or_default(), //
                            type_guard_type: None,
                            is_async: false,
                            is_generator: false,
                            is_reexport: false,
                            is_type_only: export.is_type_only,
                            release_tag: export.release_tag, // META-04: propagate TSDoc release tag
                            // TYPE-01: forward the populated
                            // satisfies_type onto the resolved ExportSymbol
                            // alongside release_tag — single-write from the
                            // post-parse populate pass above.
                            satisfies_type: export.satisfies_type,
                            // ARCH-02: propagate compound-component members
                            // from the RawExport (populated by attach_compound_members).
                            compound_members: export.compound_members,
                            // INF-04: forward typeof_import_refs from
                            // the populated RawExport (extract_typeof_import_refs
                            // populate pass below at lines ~485-506). Empty Vec
                            // when the export's type position contains no
                            // `typeof import(...).Y` references.
                            typeof_import_refs: export.typeof_import_refs.clone(),
                            // DOC-01: forward proxy-dispatched flag from RawExport.
                            is_proxy_dispatched: export.is_proxy_dispatched,
                            local_name: export.local_name.clone(),
                        },
                    );
                }
            }
            RawExportKind::Default => {
                //: pre-merge at insertion. Default space is derived
                // from the declaration's kind when one is present (e.g., `export
                // default function foo` -> Value; `export default class Foo` ->
                // Value); anonymous expression-form defaults default to Value.
                let space = space_for(export.decl_kind.as_ref());
                let key = ExportKey {
                    subpath: subpath.to_string(),
                    condition: condition.map(|c| c.to_string()),
                    space,
                    name: "default".to_string(),
                };
                if let Some(existing) = map.get_mut(&key) {
                    merge_hybrid(
                        existing,
                        export.decl_kind.clone(),
                        // INF-02: carry declare-site signature
                        // through default-export merge path for symmetry with
                        // Named.
                        export.type_signature.clone(),
                        None,
                    );
                } else {
                    map.insert(
                        key,
                        ExportSymbol {
                            name: "default".to_string(),
                            kind: ExportKind::Default,
                            file: rel_path.clone(),
                            line: export.declaration_site.map_or(export.line, |site| site.0),
                            col: export.declaration_site.map_or(export.col, |site| site.1),
                            // INF-02: default-export carries the
                            // declare-site type signature when the INF-03
                            // trailer (`export { foo as default }`) linked the
                            // default to a local `declare const foo: T`.
                            type_signature: export.type_signature,
                            overloads: Vec::new(),
                            decl_kinds: export.decl_kind.map(|k| vec![k]).unwrap_or_default(), //
                            type_guard_type: None,
                            is_async: false,
                            is_generator: false,
                            is_reexport: false,
                            is_type_only: export.is_type_only,
                            // META-06: propagate the release_tag populated
                            // by the comment_scan pass on the current file's RawExport.
                            // Without this, `/** @deprecated */ export default foo`
                            // loses its tag at insertion time.
                            release_tag: export.release_tag,
                            // TYPE-01: propagate satisfies_type for
                            // default-export forms — covers cases like
                            // `const foo = {...} satisfies T; export default foo;`
                            // where the pop-pass tagged the binding.
                            satisfies_type: export.satisfies_type,
                            compound_members: Vec::new(),
                            // INF-04: default exports propagate
                            // typeof_import_refs from the populated RawExport.
                            typeof_import_refs: export.typeof_import_refs.clone(),
                            // DOC-01: forward proxy-dispatched flag from RawExport.
                            is_proxy_dispatched: export.is_proxy_dispatched,
                            local_name: export.local_name.clone(),
                        },
                    );
                }
            }
            RawExportKind::StarReexport => {
                if let Some(specifier) = &export.source {
                    match classify_specifier(specifier) {
                        SpecifierKind::NodeBuiltin | SpecifierKind::UrlScheme => {
                            warnings.push(ResolveWarning::new(format!(
                                "cannot model public re-export from host module `{specifier}` in {}",
                                file.display()
                            )));
                            continue;
                        }
                        SpecifierKind::ImportsField => {
                            // EXP-14: route #subpath through package.json imports map.
                            if let Some(im) = imports_map {
                                if let Some(target) =
                                    resolve_imports_subpath(im, specifier, pkg_root, condition)
                                {
                                    collect_exports_cached(
                                        &target,
                                        pkg_root,
                                        map,
                                        visited,
                                        warnings,
                                        depth + 1,
                                        tsconfig,
                                        imports_map,
                                        subpath,
                                        condition,
                                        cache,
                                    )?;
                                    continue;
                                }
                            }
                            // A missing imports map or target leaves resolution incomplete.
                            warnings.push(ResolveWarning::new(format!(
                                "could not resolve `{}` through package.json imports map (from {})",
                                specifier,
                                file.display()
                            )));
                            continue;
                        }
                        SpecifierKind::Relative | SpecifierKind::Bare => {
                            let candidates =
                                source_resolution_paths(&canonical, specifier, tsconfig, pkg_root);
                            let resolved = candidates.into_iter().find(|p| p.is_file());
                            match resolved {
                                Some(target) => {
                                    // propagate current subpath/condition into the
                                    // recursive resolution so all exports reached through this
                                    // re-export chain are keyed consistently.
                                    collect_exports_cached(
                                        &target,
                                        pkg_root,
                                        map,
                                        visited,
                                        warnings,
                                        depth + 1,
                                        tsconfig,
                                        imports_map,
                                        subpath,
                                        condition,
                                        cache,
                                    )?;
                                }
                                None => {
                                    // If specifier looks like an external package (not relative),
                                    // track it as an external re-export entry rather than just warning.
                                    if !specifier.starts_with('.') && !specifier.starts_with('/') {
                                        let reexport_key = format!("*<{}>", specifier);
                                        //: pre-merge at insertion (StarReexport external).
                                        // External re-exports carry no decl_kind to merge — first-write-wins
                                        // via get_mut/insert branch is the correct semantic.
                                        let key = ExportKey {
                                            subpath: subpath.to_string(),
                                            condition: condition.map(|c| c.to_string()),
                                            space: SymbolSpace::Value,
                                            name: reexport_key.clone(),
                                        };
                                        #[allow(clippy::map_entry)]
                                        if !map.contains_key(&key) {
                                            map.insert(
                                                key,
                                                ExportSymbol {
                                                    name: reexport_key,
                                                    kind: ExportKind::Named,
                                                    file: rel_path.clone(),
                                                    line: export.line,
                                                    col: export.col,
                                                    type_signature: None,
                                                    overloads: Vec::new(),
                                                    decl_kinds: Vec::new(),
                                                    type_guard_type: None,
                                                    is_async: false,
                                                    is_generator: false,
                                                    is_reexport: true,
                                                    is_type_only: false,
                                                    release_tag: None,
                                                    satisfies_type: None,
                                                    compound_members: Vec::new(),
                                                    // INF-04: external star re-exports
                                                    // carry no source-site type position; empty Vec
                                                    // is correct.
                                                    typeof_import_refs: Vec::new(),
                                                    is_proxy_dispatched: false,
                                                    local_name: None,
                                                },
                                            );
                                        }
                                    } else {
                                        warnings.push(ResolveWarning::new(format!(
                                            "could not resolve star re-export '{}' from {}",
                                            specifier,
                                            file.display()
                                        )));
                                    }
                                }
                            }
                        } // end SpecifierKind::Relative | Bare arm
                    } // end match classify_specifier
                }
            }
            RawExportKind::NamedReexport => {
                if let Some(specifier) = &export.source {
                    match classify_specifier(specifier) {
                        SpecifierKind::NodeBuiltin | SpecifierKind::UrlScheme => {
                            warnings.push(ResolveWarning::new(format!(
                                "cannot model public re-export from host module `{specifier}` in {}",
                                file.display()
                            )));
                            continue;
                        }
                        SpecifierKind::ImportsField => {
                            // EXP-14: route #subpath through package.json imports map.
                            if let Some(im) = imports_map {
                                if let Some(target) =
                                    resolve_imports_subpath(im, specifier, pkg_root, condition)
                                {
                                    // Insert a named re-export pointing at the resolved target file —
                                    // mirrors the existing `Some(target)` insertion at the (now
                                    // `Relative | Bare`) arm. Use space_for(export.decl_kind.as_ref())
                                    // and propagate release_tag the same way.
                                    let space = space_for(export.decl_kind.as_ref());
                                    let key = ExportKey {
                                        subpath: subpath.to_string(),
                                        condition: condition.map(|c| c.to_string()),
                                        space,
                                        name: export.name.clone(),
                                    };
                                    #[allow(clippy::map_entry)]
                                    if !map.contains_key(&key) {
                                        map.insert(
                                            key,
                                            ExportSymbol {
                                                name: export.name.clone(),
                                                kind: ExportKind::Named,
                                                file: target
                                                    .strip_prefix(pkg_root)
                                                    .unwrap_or(&target)
                                                    .to_path_buf(),
                                                line: export.line,
                                                col: export.col,
                                                type_signature: None,
                                                overloads: Vec::new(),
                                                decl_kinds: export
                                                    .decl_kind
                                                    .clone()
                                                    .map(|k| vec![k])
                                                    .unwrap_or_default(),
                                                type_guard_type: None,
                                                is_async: false,
                                                is_generator: false,
                                                is_reexport: false,
                                                is_type_only: false,
                                                release_tag: export.release_tag.clone(),
                                                satisfies_type: None,
                                                compound_members: Vec::new(),
                                                typeof_import_refs: Vec::new(),
                                                is_proxy_dispatched: false,
                                                local_name: None,
                                            },
                                        );
                                    }
                                    continue;
                                }
                            }
                            warnings.push(ResolveWarning::new(format!(
                                "could not resolve `{}` through package.json imports map (from {})",
                                specifier,
                                file.display()
                            )));
                            continue;
                        }
                        SpecifierKind::Relative | SpecifierKind::Bare => {
                            let candidates =
                                source_resolution_paths(&canonical, specifier, tsconfig, pkg_root);
                            let resolved = candidates.into_iter().find(|p| p.is_file());
                            match resolved {
                                Some(target) => {
                                    // For named re-exports we only want the specific
                                    // symbol, not everything the target file exports.
                                    // We insert a placeholder pointing at the target
                                    // file's location rather than recursing fully.
                                    // The ExportSymbol references the re-export site
                                    // (current file) for accurate position reporting.
                                    //: pre-merge at insertion (NamedReexport resolved).
                                    // Named re-exports don't declaration-merge (they point to another
                                    // file's symbol) — first-write-wins is the correct semantic.
                                    let space = space_for(export.decl_kind.as_ref());
                                    let key = ExportKey {
                                        subpath: subpath.to_string(),
                                        condition: condition.map(|c| c.to_string()),
                                        space,
                                        name: export.name.clone(),
                                    };
                                    #[allow(clippy::map_entry)]
                                    if !map.contains_key(&key) {
                                        map.insert(
                                            key,
                                            ExportSymbol {
                                                name: export.name.clone(),
                                                kind: ExportKind::Named,
                                                file: target
                                                    .strip_prefix(pkg_root)
                                                    .unwrap_or(&target)
                                                    .to_path_buf(),
                                                line: export.line,
                                                col: export.col,
                                                type_signature: None,
                                                overloads: Vec::new(),
                                                decl_kinds: export
                                                    .decl_kind
                                                    .clone()
                                                    .map(|k| vec![k])
                                                    .unwrap_or_default(),
                                                type_guard_type: None,
                                                is_async: false,
                                                is_generator: false,
                                                is_reexport: false,
                                                is_type_only: false,
                                                // META-06: propagate the release_tag populated
                                                // by the comment_scan pass on the current file's RawExport.
                                                // Without this, a `/** @deprecated */ export { foo } from...`
                                                // loses its tag at the re-export boundary.
                                                release_tag: export.release_tag.clone(),
                                                satisfies_type: None,
                                                compound_members: Vec::new(),
                                                // INF-04: named re-exports point at the
                                                // target file's symbol; their typeof refs (if any)
                                                // live on the target's ExportSymbol. Empty Vec here.
                                                typeof_import_refs: Vec::new(),
                                                is_proxy_dispatched: false,
                                                local_name: None,
                                            },
                                        );
                                    }
                                }
                                None => {
                                    // If specifier looks like an external package (not relative),
                                    // track it as an external named re-export entry.
                                    if !specifier.starts_with('.') && !specifier.starts_with('/') {
                                        //: pre-merge at insertion (NamedReexport external).
                                        // decl_kind is unknown for external packages so space defaults to Value.
                                        // External re-export names don't participate in declaration
                                        // merging — first-write-wins via get_mut/insert is correct.
                                        let key = ExportKey {
                                            subpath: subpath.to_string(),
                                            condition: condition.map(|c| c.to_string()),
                                            space: SymbolSpace::Value,
                                            name: export.name.clone(),
                                        };
                                        #[allow(clippy::map_entry)]
                                        if !map.contains_key(&key) {
                                            map.insert(
                                                key,
                                                ExportSymbol {
                                                    name: export.name.clone(),
                                                    kind: ExportKind::Named,
                                                    file: rel_path.clone(),
                                                    line: export.line,
                                                    col: export.col,
                                                    type_signature: None,
                                                    overloads: Vec::new(),
                                                    decl_kinds: Vec::new(),
                                                    type_guard_type: None,
                                                    is_async: false,
                                                    is_generator: false,
                                                    is_reexport: true,
                                                    is_type_only: false,
                                                    release_tag: None,
                                                    satisfies_type: None,
                                                    compound_members: Vec::new(),
                                                    // INF-04: external named re-exports
                                                    // carry no source-site type position; empty Vec.
                                                    typeof_import_refs: Vec::new(),
                                                    is_proxy_dispatched: false,
                                                    local_name: None,
                                                },
                                            );
                                        }
                                    }
                                    if depth == 0 {
                                        // META-05: At root depth, accumulate failures per
                                        // specifier for potential barrel aggregation below.
                                        barrel_failures
                                            .entry(specifier.to_string())
                                            .or_default()
                                            .push(export.name.clone());
                                    } else {
                                        // At non-root depth, emit individual warning immediately.
                                        warnings.push(ResolveWarning::new(format!(
                                    "could not resolve named re-export '{}' from {} (specifier '{}')",
                                    export.name,
                                    file.display(),
                                    specifier,
                                )));
                                    }
                                }
                            }
                        } // end SpecifierKind::Relative | Bare arm
                    } // end match classify_specifier
                }
            }
        }
    }

    // META-05: Flush accumulated barrel failures.
    // At depth 0, specifiers with ≥ BARREL_AGGREGATE_THRESHOLD unresolvable named
    // exports are aggregated into a single "barrel" warning to reduce noise.
    // Specifiers with fewer failures produce individual warnings as before.
    const BARREL_AGGREGATE_THRESHOLD: usize = 3;
    if depth == 0 {
        let mut sorted_specifiers: Vec<String> = barrel_failures.keys().cloned().collect();
        sorted_specifiers.sort();
        for spec in sorted_specifiers {
            let names = &barrel_failures[&spec];
            if names.len() >= BARREL_AGGREGATE_THRESHOLD {
                //: sort re-export names so the aggregated
                // message is deterministic across source reorderings. Matches the
                // sorted_specifiers pattern two lines above.
                let mut sorted_names = names.clone();
                sorted_names.sort();
                warnings.push(ResolveWarning::new(format!(
                    "barrel '{}' unresolvable: {} named re-exports could not be resolved ({})",
                    spec,
                    sorted_names.len(),
                    sorted_names.join(", ")
                )));
            } else {
                for name in names {
                    warnings.push(ResolveWarning::new(format!(
                        "could not resolve named re-export '{name}' from {} (specifier '{spec}')",
                        file.display(),
                    )));
                }
            }
        }
    }

    Ok(())
}

/// Resolve an installed dependency, including generated dot-prefixed packages.
pub(crate) fn resolve_package_path(
    from_file: &Path,
    specifier: &str,
    condition: Option<&str>,
) -> Option<PathBuf> {
    if classify_specifier(specifier) != SpecifierKind::Bare {
        return None;
    }
    let parts: Vec<_> = specifier.split('/').collect();
    let package_len = if specifier.starts_with('@') { 2 } else { 1 };
    if parts.len() < package_len {
        return None;
    }
    let name = parts[..package_len].join("/");
    let subpath = if parts.len() == package_len {
        ".".into()
    } else {
        format!("./{}", parts[package_len..].join("/"))
    };
    let typescript = matches!(
        from_file
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("ts" | "tsx" | "mts" | "cts")
    );
    let mut active: HashSet<_> = condition.unwrap_or("").split('/').collect();
    active.insert("node");
    if typescript {
        active.insert("types");
    }
    if !active.contains("import") && !active.contains("require") {
        active.insert(
            if from_file
                .extension()
                .is_some_and(|extension| extension == "cjs" || extension == "cts")
            {
                "require"
            } else {
                "import"
            },
        );
    }
    for ancestor in from_file.ancestors().skip(1) {
        let root = ancestor.join("node_modules").join(&name);
        if !root.is_dir() {
            continue;
        }
        let manifest = std::fs::read(root.join("package.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        if let Some(manifest) = manifest.as_ref() {
            if manifest.get("exports").is_some() {
                let entries = entry_points::resolve_entry_points(&root).ok()?;
                return entries
                    .into_iter()
                    .filter(|entry| entry.subpath == subpath)
                    .find(|entry| {
                        entry
                            .condition
                            .as_deref()
                            .unwrap_or("default")
                            .split('/')
                            .all(|flag| flag == "default" || active.contains(flag))
                    })
                    .map(|entry| entry.path);
            }
        }
        let target = if subpath == "." {
            manifest
                .as_ref()
                .and_then(|manifest| {
                    if typescript {
                        manifest
                            .get("types")
                            .or_else(|| manifest.get("typings"))
                            .or_else(|| manifest.get("main"))
                    } else {
                        manifest.get("main")
                    }
                })
                .and_then(serde_json::Value::as_str)
                .unwrap_or("./index")
                .to_string()
        } else {
            subpath.clone()
        };
        let probe = root.join(if typescript {
            "__resolution__.ts"
        } else {
            "__resolution__.cjs"
        });
        return resolve_module_path(&probe, &target)
            .into_iter()
            .find(|path| path.is_file());
    }
    None
}

/// Append an extension to a path without replacing existing extensions.
///
/// Unlike `PathBuf::with_extension()`, this preserves compound file stems.
/// Example: `append_extension(Path::new("utils.config"), "ts")` -> "utils.config.ts"
/// (whereas `with_extension` would produce "utils.ts", dropping ".config")
fn append_extension(base: &Path, ext: &str) -> PathBuf {
    let name = base.file_name().unwrap_or_default().to_string_lossy();
    base.with_file_name(format!("{}.{}", name, ext))
}

fn resolve_config_alias(
    specifier: &str,
    config: &tsconfig::TsconfigPaths,
    package_root: &Path,
) -> Vec<PathBuf> {
    tsconfig::resolve_tsconfig_alias(specifier, config, package_root)
        .into_iter()
        .flat_map(|target| module_path_candidates(target, true))
        .collect()
}

fn source_resolution_paths(
    from_file: &Path,
    specifier: &str,
    config: Option<&tsconfig::TsconfigPaths>,
    package_root: &Path,
) -> Vec<PathBuf> {
    if classify_specifier(specifier) == SpecifierKind::Bare {
        if let Some(config) = config {
            let aliases = resolve_config_alias(specifier, config, package_root);
            if !aliases.is_empty() {
                return aliases;
            }
        }
        return Vec::new();
    }
    resolve_module_path(from_file, specifier)
}

/// Return an ordered list of candidate absolute paths for a module specifier.
///
/// The caller should pick the first candidate that exists on disk.
/// Extension probing order matches Node.js/TypeScript resolution heuristics.
#[allow(dead_code)]
pub fn resolve_module_path(from_file: &Path, specifier: &str) -> Vec<PathBuf> {
    let base_dir = match from_file.parent() {
        Some(d) => d.to_path_buf(),
        None => PathBuf::from("."),
    };

    // Strip leading "./" from the specifier before joining.
    let clean = specifier.strip_prefix("./").unwrap_or(specifier);
    let joined = base_dir.join(clean);

    module_path_candidates(joined, uses_typescript_resolution(from_file))
}

fn uses_typescript_resolution(from_file: &Path) -> bool {
    matches!(
        from_file
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("ts" | "tsx" | "mts" | "cts")
    )
}

fn module_path_candidates(joined: PathBuf, typescript: bool) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(32);
    if !typescript {
        candidates.push(joined.clone());
    }

    // TypeScript substitutes declarations/source before an existing JS target.
    // Runtime JS traversal retains the exact runtime file as its first choice.
    if let Some(ext) = joined.extension() {
        let ext_str = ext.to_string_lossy();
        match ext_str.as_ref() {
            "js" => {
                let stem = joined.with_extension("");
                candidates.push(append_extension(&stem, "ts"));
                candidates.push(append_extension(&stem, "tsx"));
                candidates.push(stem.with_file_name(format!(
                    "{}.d.ts",
                    stem.file_name().unwrap_or_default().to_string_lossy()
                )));
            }
            "mjs" => {
                let stem = joined.with_extension("");
                candidates.push(append_extension(&stem, "mts"));
                candidates.push(stem.with_file_name(format!(
                    "{}.d.mts",
                    stem.file_name().unwrap_or_default().to_string_lossy()
                )));
            }
            "cjs" => {
                let stem = joined.with_extension("");
                candidates.push(append_extension(&stem, "cts"));
                candidates.push(stem.with_file_name(format!(
                    "{}.d.cts",
                    stem.file_name().unwrap_or_default().to_string_lossy()
                )));
            }
            _ => {}
        }
    }

    if typescript {
        candidates.push(joined.clone());
    }

    if !typescript && joined.extension().is_none() {
        for extension in ["js", "json", "node", "cjs", "mjs"] {
            candidates.push(append_extension(&joined, extension));
        }
    }

    // 2. TypeScript source extensions (higher priority for TS projects).
    candidates.push(append_extension(&joined, "ts"));
    candidates.push(append_extension(&joined, "tsx"));
    candidates.push(append_extension(&joined, "mts"));
    candidates.push(append_extension(&joined, "cts"));

    // 3. Declaration files (.d.ts,.d.mts,.d.cts).
    let stem = joined.file_name().unwrap_or_default().to_string_lossy();
    candidates.push(joined.with_file_name(format!("{}.d.ts", stem)));
    candidates.push(joined.with_file_name(format!("{}.d.mts", stem)));
    candidates.push(joined.with_file_name(format!("{}.d.cts", stem)));

    // 4. JavaScript extensions.
    candidates.push(append_extension(&joined, "js"));
    candidates.push(append_extension(&joined, "jsx"));
    candidates.push(append_extension(&joined, "mjs"));
    candidates.push(append_extension(&joined, "cjs"));

    // Runtime traversal must prefer the directory's JavaScript over TypeScript
    // sources shipped beside it. TypeScript callers keep extension substitution.
    if !typescript {
        for extension in ["js", "json", "node"] {
            candidates.push(joined.join(format!("index.{extension}")));
        }
    }
    // TypeScript and remaining source/declaration directory candidates.
    candidates.push(joined.join("index.ts"));
    candidates.push(joined.join("index.tsx"));
    candidates.push(joined.join("index.mts"));
    candidates.push(joined.join("index.cts"));
    candidates.push(joined.join("index.js"));
    candidates.push(joined.join("index.jsx"));
    candidates.push(joined.join("index.mjs"));
    candidates.push(joined.join("index.cjs"));
    candidates.push(joined.join("index.d.ts"));
    candidates.push(joined.join("index.d.mts"));
    candidates.push(joined.join("index.d.cts"));

    candidates
}

/// Resolve runtime files without TypeScript extension substitution.
pub(crate) fn resolve_runtime_dependency(
    from: &Path,
    specifier: &str,
    mode: &str,
) -> Option<PathBuf> {
    let target = from.parent()?.join(specifier);
    if mode == "import" {
        return target.is_file().then_some(target);
    }
    fn file(target: &Path) -> Option<PathBuf> {
        if target.is_file() {
            return Some(target.to_path_buf());
        }
        ["js", "json", "node"]
            .iter()
            .map(|extension| {
                let mut path = target.as_os_str().to_os_string();
                path.push(format!(".{extension}"));
                PathBuf::from(path)
            })
            .find(|path| path.is_file())
    }
    if let Some(path) = file(&target) {
        return Some(path);
    }
    if let Some(main) = std::fs::read_to_string(target.join("package.json"))
        .ok()
        .and_then(|source| serde_json::from_str::<serde_json::Value>(&source).ok())
        .and_then(|manifest| {
            manifest
                .get("main")
                .and_then(|main| main.as_str())
                .map(str::to_owned)
        })
    {
        let main = target.join(main);
        if let Some(path) = file(&main).or_else(|| file(&main.join("index"))) {
            return Some(path);
        }
    }
    file(&target.join("index"))
}

#[cfg(test)]
mod tests;
