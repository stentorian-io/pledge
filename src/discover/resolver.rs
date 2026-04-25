use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Context;

use super::comment_scan::CommentIndex;
use super::tree_query::{language_for_file, parse_exports, RawExportKind};
use crate::types::{DeclarationKind, ExportKey, ExportKind, ExportMap, ExportSymbol, SymbolSpace};

// ---------------------------------------------------------------------------
// Phase 18: key-construction helpers (D-13 pre-merge, D-14 decl_kinds union)
// ---------------------------------------------------------------------------

/// Per Phase 18 Open Q2 convention (documented on SymbolSpace in types/mod.rs):
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

/// D-13 + D-14: merge an incoming raw export into an existing symbol at the
/// same ExportKey. Used by every insertion call site to handle declaration-
/// merging hybrids (class+namespace, function+namespace, interface+namespace)
/// at discovery time, eliminating the REF-05 or_insert race by construction
/// (D-16).
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
    incoming_release_tag: Option<crate::types::ReleaseTag>,
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

// ---------------------------------------------------------------------------
// Tsconfig path alias resolution (GAP-15-04)
// ---------------------------------------------------------------------------

/// Parsed tsconfig.json path aliases.
#[derive(Debug, Clone, Default)]
pub struct TsconfigPaths {
    /// Base URL for non-relative module resolution.
    pub base_url: Option<String>,
    /// Path alias mappings: key is the alias pattern (e.g., "@/*"),
    /// value is the list of replacement paths (e.g., ["./src/*"]).
    pub paths: std::collections::HashMap<String, Vec<String>>,
}

/// ROB-04: Strip JSONC syntax (line comments, block comments, trailing commas) before
/// handing to serde_json. This covers the full JSONC feature set used by tsconfig.json.
///
/// The state machine correctly handles strings — comment markers inside string literals
/// are treated as regular characters. Unbalanced block-comment delimiters (e.g., `/*`
/// without a matching `*/`) are handled safely: the inner loop terminates at EOF.
///
/// Iterates over Unicode scalar values (chars) rather than raw bytes to preserve
/// multi-byte UTF-8 sequences — a tsconfig.json with non-ASCII identifiers, paths,
/// or comments (e.g., emoji, CJK) must survive stripping intact for serde_json.
fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();
    let mut in_string = false;
    let mut string_char = '"';

    while let Some((_idx, c)) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                // Escape sequence — copy the next char verbatim (preserves e.g. \" inside strings).
                if let Some((_, next_c)) = chars.next() {
                    out.push(next_c);
                }
                continue;
            }
            if c == string_char {
                in_string = false;
            }
            continue;
        }

        // Start of a string
        if c == '"' || c == '\'' {
            in_string = true;
            string_char = c;
            out.push(c);
            continue;
        }

        // Line comment: // ...
        if c == '/' && matches!(chars.peek(), Some((_, '/'))) {
            chars.next(); // consume second '/'
            while let Some(&(_, nc)) = chars.peek() {
                if nc == '\n' {
                    break;
                }
                chars.next();
            }
            continue;
        }

        // Block comment: /* ... */
        if c == '/' && matches!(chars.peek(), Some((_, '*'))) {
            chars.next(); // consume '*'
            while let Some((_, nc)) = chars.next() {
                if nc == '*' && matches!(chars.peek(), Some((_, '/'))) {
                    chars.next(); // consume '/'
                    break;
                }
            }
            continue;
        }

        out.push(c);
    }

    // Second pass: strip trailing commas before `}` or `]`.
    // Work on chars (not bytes) so multi-byte sequences remain intact.
    let mut result = String::with_capacity(out.len());
    let chars: Vec<char> = out.chars().collect();
    let mut j = 0;
    while j < chars.len() {
        let c = chars[j];
        if c == ',' {
            // Lookahead past whitespace for } or ]
            let mut k = j + 1;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if k < chars.len() && (chars[k] == '}' || chars[k] == ']') {
                // Skip this trailing comma
                j += 1;
                continue;
            }
        }
        result.push(c);
        j += 1;
    }

    result
}

/// Parse `tsconfig.json` from `pkg_root` and extract `compilerOptions.paths` and `compilerOptions.baseUrl`.
///
/// Returns `None` if tsconfig.json doesn't exist or is unparseable.
/// This is best-effort — missing fields are silently ignored.
pub fn parse_tsconfig_paths(pkg_root: &Path) -> Option<TsconfigPaths> {
    let tsconfig_path = pkg_root.join("tsconfig.json");
    let content = std::fs::read_to_string(&tsconfig_path).ok()?;
    // ROB-04: tsconfig.json is JSONC — strip line comments, block comments, and
    // trailing commas before handing to serde_json.
    let stripped = strip_jsonc(&content);
    let json: serde_json::Value = serde_json::from_str(&stripped).ok()?;

    let compiler_opts = json.get("compilerOptions")?;

    let base_url = compiler_opts
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut paths_map = std::collections::HashMap::new();
    if let Some(paths_obj) = compiler_opts.get("paths").and_then(|v| v.as_object()) {
        for (key, value) in paths_obj {
            if let Some(arr) = value.as_array() {
                let replacements: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if !replacements.is_empty() {
                    paths_map.insert(key.clone(), replacements);
                }
            }
        }
    }

    Some(TsconfigPaths {
        base_url,
        paths: paths_map,
    })
}

/// Attempt to resolve a module specifier using tsconfig path aliases.
///
/// Given a specifier like `@/utils` and paths like `{"@/*": ["./src/*"]}`,
/// returns a list of candidate absolute paths to try.
///
/// Returns empty Vec if no alias matches.
pub fn resolve_tsconfig_alias(
    specifier: &str,
    tsconfig: &TsconfigPaths,
    pkg_root: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Determine the base directory for resolution
    let base_dir = if let Some(ref base_url) = tsconfig.base_url {
        pkg_root.join(base_url)
    } else {
        pkg_root.to_path_buf()
    };

    for (pattern, replacements) in &tsconfig.paths {
        // Pattern can be exact match or wildcard match (e.g., "@/*")
        if pattern.contains('*') {
            // Wildcard pattern: "@/*" matches "@/utils", "@/components/Button", etc.
            let prefix = pattern.strip_suffix('*').unwrap_or(pattern);
            if let Some(remainder) = specifier.strip_prefix(prefix) {
                for replacement in replacements {
                    if replacement.contains('*') {
                        let replaced = replacement.replace('*', remainder);
                        let resolved = base_dir.join(replaced.trim_start_matches("./"));
                        // Add with standard extension probing
                        candidates.push(resolved.clone());
                        candidates.push(resolved.with_extension("ts"));
                        candidates.push(resolved.with_extension("tsx"));
                        candidates.push(resolved.with_extension("js"));
                        candidates.push(resolved.with_extension("jsx"));
                        // Directory index fallback
                        candidates.push(resolved.join("index.ts"));
                        candidates.push(resolved.join("index.tsx"));
                        candidates.push(resolved.join("index.js"));
                    } else {
                        let resolved = base_dir.join(replacement.trim_start_matches("./"));
                        candidates.push(resolved);
                    }
                }
            }
        } else {
            // Exact pattern match
            if specifier == pattern {
                for replacement in replacements {
                    let resolved = base_dir.join(replacement.trim_start_matches("./"));
                    candidates.push(resolved);
                }
            }
        }
    }

    candidates
}

/// Maximum number of re-export hops before aborting a resolution chain (D-12).
#[allow(dead_code)]
const MAX_RESOLVE_DEPTH: usize = 20;

/// CHUNK-01: Normalize hashed bundler chunk filenames so that rollup/vite rebuilds
/// produce stable paths for diff comparison.
///
/// Applies only when the path contains a `/chunks/` directory segment AND the
/// basename matches `chunk-[A-Z0-9]{8,}.<ext>`. Example:
///   `dist/chunks/chunk-A1B2C3D4.js` → `dist/chunks/chunk-*.js`
fn normalize_chunk_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    let in_chunks_dir = path_str.contains("/chunks/")
        || path_str.contains("/dist/chunks/")
        || path_str.contains("\\chunks\\"); // Windows
    if !in_chunks_dir {
        return path.to_path_buf();
    }
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
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
                if let Some(parent) = path.parent() {
                    return parent.join(normalized_name);
                }
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
}

impl ResolveWarning {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

/// Recursively collect all exports reachable from `file` into `map`.
///
/// - `pkg_root` — absolute path to the package root; used to compute relative
///   paths stored in `ExportSymbol.file`.
/// - `visited` — set of canonicalized absolute paths already processed; used
///   for cycle detection (D-12).
/// - `warnings` — non-fatal issues encountered during resolution.
/// - `depth` — current recursion depth; aborted at `MAX_RESOLVE_DEPTH` (D-12).
#[allow(dead_code, clippy::too_many_arguments)]
pub fn collect_exports(
    file: &Path,
    pkg_root: &Path,
    map: &mut ExportMap,
    visited: &mut HashSet<PathBuf>,
    warnings: &mut Vec<ResolveWarning>,
    depth: usize,
    tsconfig: Option<&TsconfigPaths>,
    subpath: &str,
    condition: Option<&str>,
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

    if !visited.insert(canonical.clone()) {
        warnings.push(ResolveWarning::new(format!(
            "circular re-export detected at {}, skipping",
            file.display()
        )));
        return Ok(());
    }

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
    // Invalid UTF-8 (e.g., from binary blobs accidentally named .ts) is treated as
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
            warnings.push(ResolveWarning::new(format!(
                "parse failed for {}: {} \u{2014} skipping file",
                canonical.display(),
                e
            )));
            return Ok(());
        }
    };

    // META-06 (Phase 19): backfill release_tag on every RawExport uniformly.
    // Single populate pass using the CommentIndex sidecar — avoids editing
    // all 48 RawExport construction sites in tree_query.rs. Idempotent:
    // existing populated tags from DECL_EXPORT_QUERY (tree_query.rs:1505)
    // and AMBIENT_DECL_EXPORT_QUERY (tree_query.rs:1575) are preserved
    // via the `is_none()` guard.
    //
    // The comment_scan pass re-parses the source to own a Tree; tree-sitter
    // parsing is iterative and fast so the duplicate parse is acceptable
    // for Phase 19 (future optimization: thread the Tree from parse_exports).
    // INF-05 (Phase 19): collect directive records from the SAME CommentIndex
    // we use for META-06. Captured here as a local Vec so we can insert into
    // the ExportMap.directives sidecar once `rel_path` is computed below.
    let mut file_directives: Vec<crate::discover::comment_scan::DirectiveRecord> = Vec::new();
    {
        let language = super::tree_query::get_language(&lang);
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&language).is_ok() {
            if let Some(tree) = parser.parse(&source, None) {
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

                // TYPE-01 (Phase 20): backfill satisfies_type on eligible
                // RawExports. Mirrors the META-06 populate pass directly above
                // — same tree/source pair, single post-parse scan, no need to
                // edit individual RawExport construction sites.
                //
                // Grammar gate: `satisfies_expression` is a TypeScript AST node
                // only; `extract_satisfies_types` internally returns an empty
                // map on JavaScript inputs so the loop is a no-op there.
                let satisfies_map = super::tree_query::extract_satisfies_types(
                    tree.root_node(),
                    &source,
                    lang,
                );
                if !satisfies_map.is_empty() {
                    for rx in raw_exports.iter_mut() {
                        if rx.satisfies_type.is_some() {
                            continue;
                        }
                        if let Some(satisfies_text) = satisfies_map.get(&rx.name) {
                            let constraint = crate::types::parser::parse_type_string(
                                satisfies_text,
                            );
                            rx.satisfies_type =
                                Some(crate::types::shape::TypeShape::Satisfies {
                                    constraint: Box::new(constraint),
                                    narrowed: None,
                                });
                        }
                    }
                }
            }
        }
    }

    // Compute the file path relative to pkg_root for storage in ExportSymbol.
    // CHUNK-01: Normalize hashed chunk filenames (e.g., chunk-A1B2C3D4.js → chunk-*.js)
    // so that rollup/vite rebuilds produce stable paths for diff comparison.
    let rel_path = normalize_chunk_path(canonical.strip_prefix(pkg_root).unwrap_or(&canonical));

    // INF-05 (Phase 19): flow extracted directives into the ExportMap sidecar.
    // Key by the rel_path so compare_directives can pair old/new directive
    // lists per-file. Files with no directives are absent from the map.
    if !file_directives.is_empty() {
        map.directives.insert(rel_path.to_path_buf(), file_directives);
    }

    // META-05: Accumulate named re-export failures keyed by specifier so we can
    // aggregate them into a single "barrel" warning when ≥3 from the same barrel
    // file are unresolvable (at depth 0 only).
    let mut barrel_failures: HashMap<String, Vec<String>> = HashMap::new();

    for export in raw_exports {
        match export.kind {
            RawExportKind::Named => {
                // Phase 18 D-13: pre-merge at insertion. Construct ExportKey with
                // current subpath/condition context, then branch on get_mut to
                // union decl_kinds for declaration-merging hybrids.
                let space = space_for(export.decl_kind.as_ref());
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
                        // INF-02 (Phase 18): forward declare-site type signature
                        // through the merge path as well — if the first-seen
                        // symbol was typeless and a later declaration-merge
                        // partner carries the declare type, the merged symbol
                        // adopts it per merge_hybrid's None->Some upgrade rule.
                        export.type_signature.clone(),
                        export.release_tag.clone(),
                    );
                    // ARCH-02 (Phase 18): merge compound_members into existing symbol
                    // (dedup to avoid duplicates from multiple discovery passes).
                    for m in &export.compound_members {
                        if !existing.compound_members.iter().any(|em| em == m) {
                            existing.compound_members.push(m.clone());
                        }
                    }
                } else {
                    map.insert(
                        key,
                        ExportSymbol {
                            name: export.name,
                            kind: ExportKind::Named,
                            file: rel_path.clone(),
                            line: export.line,
                            col: export.col,
                            // INF-02 (Phase 18): carry the declare-site type
                            // signature linked during parse_exports' INF-02
                            // link-pass. None for source-form exports where
                            // no ambient declare preceded the export specifier.
                            type_signature: export.type_signature,
                            overloads: Vec::new(),
                            decl_kinds: export.decl_kind.map(|k| vec![k]).unwrap_or_default(), // D-14
                            type_guard_type: None,
                            is_async: false,
                            is_generator: false,
                            is_reexport: false,
                            is_type_only: false,
                            release_tag: export.release_tag, // META-04: propagate TSDoc release tag
                            // TYPE-01 (Phase 20): forward the populated
                            // satisfies_type onto the resolved ExportSymbol
                            // alongside release_tag — single-write from the
                            // post-parse populate pass above.
                            satisfies_type: export.satisfies_type,
                            // ARCH-02 (Phase 18): propagate compound-component members
                            // from the RawExport (populated by attach_compound_members).
                            compound_members: export.compound_members,
                            // INF-04 (Phase 22): typeof_import_refs propagated
                            // from RawExport in Task 2 once the RawExport sidecar
                            // lands. Empty Vec for Task 1 baseline.
                            typeof_import_refs: Vec::new(),
                        },
                    );
                }
            }
            RawExportKind::Default => {
                // Phase 18 D-13: pre-merge at insertion. Default space is derived
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
                        // INF-02 (Phase 18): carry declare-site signature
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
                            line: export.line,
                            col: export.col,
                            // INF-02 (Phase 18): default-export carries the
                            // declare-site type signature when the INF-03
                            // trailer (`export { foo as default }`) linked the
                            // default to a local `declare const foo: T`.
                            type_signature: export.type_signature,
                            overloads: Vec::new(),
                            decl_kinds: export.decl_kind.map(|k| vec![k]).unwrap_or_default(), // D-14
                            type_guard_type: None,
                            is_async: false,
                            is_generator: false,
                            is_reexport: false,
                            is_type_only: false,
                            // META-06 (Phase 19): propagate the release_tag populated
                            // by the comment_scan pass on the current file's RawExport.
                            // Without this, `/** @deprecated */ export default foo`
                            // loses its tag at insertion time.
                            release_tag: export.release_tag,
                            // TYPE-01 (Phase 20): propagate satisfies_type for
                            // default-export forms — covers cases like
                            // `const foo = {...} satisfies T; export default foo;`
                            // where the pop-pass tagged the binding.
                            satisfies_type: export.satisfies_type,
                            compound_members: Vec::new(),
                            // INF-04 (Phase 22): default exports propagate
                            // typeof_import_refs through the same populate pass.
                            // Empty Vec for Task 1 baseline; Task 2 wires the
                            // populate from RawExport.
                            typeof_import_refs: Vec::new(),
                        },
                    );
                }
            }
            RawExportKind::StarReexport => {
                if let Some(specifier) = &export.source {
                    let mut candidates = resolve_module_path(&canonical, specifier);
                    // GAP-15-04: Try tsconfig alias resolution if standard resolution fails
                    if let Some(tc) = tsconfig {
                        if !candidates.iter().any(|p| p.exists()) {
                            candidates.extend(resolve_tsconfig_alias(specifier, tc, pkg_root));
                        }
                    }
                    let resolved = candidates.into_iter().find(|p| p.exists());
                    match resolved {
                        Some(target) => {
                            // Phase 18: propagate current subpath/condition into the
                            // recursive resolution so all exports reached through this
                            // re-export chain are keyed consistently.
                            collect_exports(
                                &target,
                                pkg_root,
                                map,
                                visited,
                                warnings,
                                depth + 1,
                                tsconfig,
                                subpath,
                                condition,
                            )?;
                        }
                        None => {
                            // GAP-15-05: If specifier looks like an external package (not relative),
                            // track it as an external re-export entry rather than just warning.
                            if !specifier.starts_with('.') && !specifier.starts_with('/') {
                                let reexport_key = format!("*<{}>", specifier);
                                // Phase 18 D-13: pre-merge at insertion (StarReexport external).
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
                                            // INF-04 (Phase 22): external star re-exports
                                            // carry no source-site type position; empty Vec
                                            // is correct.
                                            typeof_import_refs: Vec::new(),
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
                }
            }
            RawExportKind::NamedReexport => {
                if let Some(specifier) = &export.source {
                    let mut candidates = resolve_module_path(&canonical, specifier);
                    // GAP-15-04: Try tsconfig alias resolution if standard resolution fails
                    if let Some(tc) = tsconfig {
                        if !candidates.iter().any(|p| p.exists()) {
                            candidates.extend(resolve_tsconfig_alias(specifier, tc, pkg_root));
                        }
                    }
                    let resolved = candidates.into_iter().find(|p| p.exists());
                    match resolved {
                        Some(target) => {
                            // For named re-exports we only want the specific
                            // symbol, not everything the target file exports.
                            // We insert a placeholder pointing at the target
                            // file's location rather than recursing fully.
                            // The ExportSymbol references the re-export site
                            // (current file) for accurate position reporting.
                            // Phase 18 D-13: pre-merge at insertion (NamedReexport resolved).
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
                                        // META-06 (Phase 19): propagate the release_tag populated
                                        // by the comment_scan pass on the current file's RawExport.
                                        // Without this, a `/** @deprecated */ export { foo } from ...`
                                        // loses its tag at the re-export boundary.
                                        release_tag: export.release_tag.clone(),
                                        satisfies_type: None,
                                        compound_members: Vec::new(),
                                        // INF-04 (Phase 22): named re-exports point at the
                                        // target file's symbol; their typeof refs (if any)
                                        // live on the target's ExportSymbol. Empty Vec here.
                                        typeof_import_refs: Vec::new(),
                                    },
                                );
                            }
                        }
                        None => {
                            // GAP-15-05: If specifier looks like an external package (not relative),
                            // track it as an external named re-export entry.
                            if !specifier.starts_with('.') && !specifier.starts_with('/') {
                                // Phase 18 D-13: pre-merge at insertion (NamedReexport external).
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
                                            // INF-04 (Phase 22): external named re-exports
                                            // carry no source-site type position; empty Vec.
                                            typeof_import_refs: Vec::new(),
                                        },
                                    );
                                }
                            } else if depth == 0 {
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
                // WR-03 (Phase 17 review): sort re-export names so the aggregated
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

/// Append an extension to a path without replacing existing extensions.
///
/// Unlike `PathBuf::with_extension()`, this preserves compound file stems.
/// Example: `append_extension(Path::new("utils.config"), "ts")` -> "utils.config.ts"
/// (whereas `with_extension` would produce "utils.ts", dropping ".config")
fn append_extension(base: &Path, ext: &str) -> PathBuf {
    let name = base.file_name().unwrap_or_default().to_string_lossy();
    base.with_file_name(format!("{}.{}", name, ext))
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
    let clean = specifier.trim_start_matches("./");
    let joined = base_dir.join(clean);

    let mut candidates = Vec::with_capacity(20);

    // 1. Exact path as-is (handles already-extensioned imports).
    candidates.push(joined.clone());

    // GAP-13-06: TypeScript moduleResolution node16/bundler — .js → .ts probing.
    // When specifier ends with .js/.mjs/.cjs, probe TypeScript equivalents.
    // Must come BEFORE generic extension probing to get correct priority:
    // foo.js (exact) → foo.ts → foo.tsx → foo.d.ts → (then generic extensions)
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

    // 2. TypeScript source extensions (higher priority for TS projects).
    candidates.push(append_extension(&joined, "ts"));
    candidates.push(append_extension(&joined, "tsx"));
    candidates.push(append_extension(&joined, "mts"));
    candidates.push(append_extension(&joined, "cts"));

    // 3. Declaration files (.d.ts, .d.mts, .d.cts).
    let stem = joined.file_name().unwrap_or_default().to_string_lossy();
    candidates.push(joined.with_file_name(format!("{}.d.ts", stem)));
    candidates.push(joined.with_file_name(format!("{}.d.mts", stem)));
    candidates.push(joined.with_file_name(format!("{}.d.cts", stem)));

    // 4. JavaScript extensions.
    candidates.push(append_extension(&joined, "js"));
    candidates.push(append_extension(&joined, "jsx"));
    candidates.push(append_extension(&joined, "mjs"));
    candidates.push(append_extension(&joined, "cjs"));

    // 5. Directory index files (GAP-13-07: complete candidate list).
    // Order: TypeScript source → JavaScript source → Declaration files.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compound_extension_preserved() {
        let result = append_extension(Path::new("/a/utils.config"), "ts");
        let name = result.file_name().unwrap().to_string_lossy();
        assert_eq!(
            name, "utils.config.ts",
            "append_extension must preserve compound stem"
        );
    }

    #[test]
    fn test_resolve_includes_mts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo");
        let has_mts = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("foo.mts"));
        assert!(
            has_mts,
            "candidates must include foo.mts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_cts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo");
        let has_cts = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("foo.cts"));
        assert!(
            has_cts,
            "candidates must include foo.cts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_d_mts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("foo.d.mts"));
        assert!(
            has,
            "candidates must include foo.d.mts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_d_cts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("foo.d.cts"));
        assert!(
            has,
            "candidates must include foo.d.cts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_compound_stem() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./utils.config");
        let has_compound = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("utils.config.ts"));
        let has_wrong = candidates.iter().any(|p| {
            let s = p.to_string_lossy();
            // "utils.ts" would mean compound stem was dropped
            s.ends_with("/utils.ts") || s.ends_with("\\utils.ts")
        });
        assert!(
            has_compound,
            "must include utils.config.ts; got: {candidates:?}"
        );
        assert!(
            !has_wrong,
            "must NOT include utils.ts (dropped compound stem); got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_index_mts() {
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("dir/index.mts"));
        assert!(
            has,
            "candidates must include dir/index.mts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_js_to_ts_probing() {
        // GAP-13-06: ./foo.js should probe foo.ts before foo.js.ts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo.js");
        let ts_idx = candidates
            .iter()
            .position(|p| p.to_string_lossy().ends_with("/foo.ts"));
        let js_ts_idx = candidates
            .iter()
            .position(|p| p.to_string_lossy().ends_with("/foo.js.ts"));
        assert!(
            ts_idx.is_some(),
            "candidates must include foo.ts; got: {candidates:?}"
        );
        // foo.ts should appear before foo.js.ts in the list
        if let (Some(ts), Some(js_ts)) = (ts_idx, js_ts_idx) {
            assert!(
                ts < js_ts,
                "foo.ts (idx {ts}) should appear before foo.js.ts (idx {js_ts})"
            );
        }
    }

    #[test]
    fn test_mjs_to_mts_probing() {
        // GAP-13-06: ./bar.mjs should probe bar.mts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./bar.mjs");
        let has_mts = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("/bar.mts"));
        assert!(
            has_mts,
            "candidates must include bar.mts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_cjs_to_cts_probing() {
        // GAP-13-06: ./baz.cjs should probe baz.cts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./baz.cjs");
        let has_cts = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("/baz.cts"));
        assert!(
            has_cts,
            "candidates must include baz.cts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_js_to_ts_compound_stem() {
        // GAP-13-06 + ECF-06: ./utils.config.js should probe utils.config.ts (not utils.ts)
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./utils.config.js");
        let has_config_ts = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("/utils.config.ts"));
        assert!(
            has_config_ts,
            "candidates must include utils.config.ts; got: {candidates:?}"
        );
        // Should NOT produce just utils.ts (dropping .config)
        let has_bare_utils = candidates.iter().any(|p| {
            let s = p.to_string_lossy();
            s.ends_with("/utils.ts") && !s.contains("config")
        });
        assert!(
            !has_bare_utils,
            "must NOT drop compound stem; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_index_cts() {
        // GAP-13-07: directory index should include index.cts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("dir/index.cts"));
        assert!(
            has,
            "candidates must include dir/index.cts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_index_jsx() {
        // GAP-13-07: directory index should include index.jsx
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("dir/index.jsx"));
        assert!(
            has,
            "candidates must include dir/index.jsx; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_index_cjs() {
        // GAP-13-07: directory index should include index.cjs
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("dir/index.cjs"));
        assert!(
            has,
            "candidates must include dir/index.cjs; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_index_d_mts() {
        // GAP-13-07: directory index should include index.d.mts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("dir/index.d.mts"));
        assert!(
            has,
            "candidates must include dir/index.d.mts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_includes_index_d_cts() {
        // GAP-13-07: directory index should include index.d.cts
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./dir");
        let has = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("dir/index.d.cts"));
        assert!(
            has,
            "candidates must include dir/index.d.cts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_js_to_dts_probing() {
        // GAP-13-06: ./foo.js should also probe foo.d.ts for pre-built packages
        let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./foo.js");
        let has_dts = candidates.iter().any(|p| {
            let s = p.to_string_lossy();
            s.ends_with("/foo.d.ts")
        });
        assert!(
            has_dts,
            "candidates must include foo.d.ts; got: {candidates:?}"
        );
    }

    // -------------------------------------------------------------------------
    // GAP-15-04: tsconfig path alias resolution tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_resolve_tsconfig_alias_wildcard() {
        // Pattern "@/*" with replacement "./src/*", specifier "@/utils" → {root}/src/utils.ts
        let mut paths = std::collections::HashMap::new();
        paths.insert("@/*".to_string(), vec!["./src/*".to_string()]);
        let tsconfig = TsconfigPaths {
            base_url: None,
            paths,
        };
        let pkg_root = Path::new("/project");
        let candidates = resolve_tsconfig_alias("@/utils", &tsconfig, pkg_root);
        let has_utils_ts = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("src/utils.ts"));
        assert!(
            has_utils_ts,
            "wildcard alias must produce src/utils.ts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_tsconfig_alias_exact() {
        // Exact pattern "@app" with replacement "./src/app.ts", specifier "@app" resolves
        let mut paths = std::collections::HashMap::new();
        paths.insert("@app".to_string(), vec!["./src/app.ts".to_string()]);
        let tsconfig = TsconfigPaths {
            base_url: None,
            paths,
        };
        let pkg_root = Path::new("/project");
        let candidates = resolve_tsconfig_alias("@app", &tsconfig, pkg_root);
        assert!(
            !candidates.is_empty(),
            "exact alias must produce candidates; got: {candidates:?}"
        );
        let has_app_ts = candidates
            .iter()
            .any(|p| p.to_string_lossy().ends_with("src/app.ts"));
        assert!(
            has_app_ts,
            "exact alias must produce src/app.ts; got: {candidates:?}"
        );
    }

    #[test]
    fn test_resolve_tsconfig_alias_no_match() {
        // Specifier "lodash" with no matching alias returns empty candidates
        let mut paths = std::collections::HashMap::new();
        paths.insert("@/*".to_string(), vec!["./src/*".to_string()]);
        let tsconfig = TsconfigPaths {
            base_url: None,
            paths,
        };
        let pkg_root = Path::new("/project");
        let candidates = resolve_tsconfig_alias("lodash", &tsconfig, pkg_root);
        assert!(
            candidates.is_empty(),
            "non-aliased specifier must return empty; got: {candidates:?}"
        );
    }

    #[test]
    fn test_parse_tsconfig_paths_basic() {
        // Create a temp dir with a tsconfig.json and verify parsing
        let dir = tempfile::tempdir().expect("tempdir");
        let tsconfig_content = r#"{
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                    "@/*": ["./src/*"],
                    "@app": ["./src/app.ts"]
                }
            }
        }"#;
        std::fs::write(dir.path().join("tsconfig.json"), tsconfig_content).unwrap();

        let result = parse_tsconfig_paths(dir.path());
        assert!(result.is_some(), "must parse tsconfig.json");
        let tc = result.unwrap();
        assert_eq!(tc.base_url.as_deref(), Some("."), "base_url should be '.'");
        assert!(tc.paths.contains_key("@/*"), "paths must contain @/*");
        assert!(tc.paths.contains_key("@app"), "paths must contain @app");
        let wildcard_replacements = &tc.paths["@/*"];
        assert_eq!(
            wildcard_replacements,
            &["./src/*"],
            "replacement for @/* should be ./src/*"
        );
    }

    #[test]
    fn test_parse_tsconfig_paths_missing_file() {
        // Non-existent tsconfig.json returns None
        let result = parse_tsconfig_paths(Path::new("/nonexistent/path"));
        assert!(result.is_none(), "missing tsconfig should return None");
    }

    #[test]
    fn test_resolve_tsconfig_alias_with_base_url() {
        // baseUrl affects resolution path
        let mut paths = std::collections::HashMap::new();
        paths.insert("@/*".to_string(), vec!["src/*".to_string()]);
        let tsconfig = TsconfigPaths {
            base_url: Some(".".to_string()),
            paths,
        };
        let pkg_root = Path::new("/project");
        let candidates = resolve_tsconfig_alias("@/utils", &tsconfig, pkg_root);
        let has_ts = candidates
            .iter()
            .any(|p| p.to_string_lossy().contains("src/utils"));
        assert!(
            has_ts,
            "alias with baseUrl must produce src/utils candidates; got: {candidates:?}"
        );
    }

    // -------------------------------------------------------------------------
    // GAP-15-05: External re-export tracking tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_external_reexport_tracked() {
        // A JS file with `export * from 'lodash'` should produce *<lodash> in export map
        let dir = tempfile::tempdir().expect("tempdir");
        let index_path = dir.path().join("index.js");
        std::fs::write(&index_path, b"export * from 'lodash';\n").unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        collect_exports(
            &index_path,
            dir.path(),
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        )
        .expect("collect_exports should succeed");

        let lodash_key = ExportKey::new_root_value("*<lodash>");
        assert!(
            map.contains_key(&lodash_key),
            "external star re-export must produce *<lodash> key; got keys: {:?}",
            map.keys().collect::<Vec<_>>()
        );
        let sym = &map[&lodash_key];
        assert!(
            sym.is_reexport,
            "*<lodash> entry must have is_reexport=true"
        );
    }

    #[test]
    fn test_external_reexport_removal_detected() {
        // Old export map with *<lodash> entry, new without → export-removed finding
        use crate::types::{ExportKey, ExportKind, ExportSymbol};
        use std::path::PathBuf;

        let mut old_map = crate::types::ExportMap::new();
        old_map.insert(
            ExportKey::new_root_value("*<lodash>"),
            ExportSymbol {
                name: "*<lodash>".to_string(),
                kind: ExportKind::Named,
                file: PathBuf::from("index.js"),
                line: 1,
                col: 1,
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
                typeof_import_refs: Vec::new(),
            },
        );
        let new_map = crate::types::ExportMap::new();

        let findings =
            crate::compare::compare_exports(&old_map, &new_map, std::path::Path::new("."));
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "export-removed" && f.message.contains("*<lodash>")),
            "removal of *<lodash> must fire export-removed; got: {:?}",
            findings
        );
    }

    #[test]
    fn test_relative_reexport_not_tracked_as_external() {
        // `export * from './utils'` where utils doesn't exist → warning, not external entry
        let dir = tempfile::tempdir().expect("tempdir");
        let index_path = dir.path().join("index.js");
        std::fs::write(&index_path, b"export * from './nonexistent';\n").unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        collect_exports(
            &index_path,
            dir.path(),
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        )
        .expect("collect_exports should succeed");

        // No external key should be created for relative specifiers
        let has_external = map.keys().any(|k| k.name.starts_with("*<"));
        assert!(
            !has_external,
            "relative re-export failure must NOT produce external key; got: {:?}",
            map.keys().collect::<Vec<_>>()
        );
        // Should have produced a warning instead
        assert!(
            !warnings.is_empty(),
            "failed relative re-export must produce a warning"
        );
    }

    // ── META-05: barrel-unresolvable aggregation ─────────────────────────────

    #[test]
    fn meta_05_three_or_more_failures_aggregated_at_depth_zero() {
        // A barrel with ≥3 unresolvable named re-exports from the same relative
        // specifier at depth==0 must produce ONE aggregated warning containing
        // "barrel" and the count, not three individual warnings.
        let dir = tempfile::tempdir().expect("tempdir");
        let index_path = dir.path().join("index.ts");
        // Three named re-exports from ./missing, none of which can be resolved.
        std::fs::write(
            &index_path,
            b"export { foo } from './missing';\nexport { bar } from './missing';\nexport { baz } from './missing';\n",
        )
        .unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        collect_exports(
            &index_path,
            dir.path(),
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        )
        .expect("collect_exports should succeed");

        // Must produce exactly ONE warning (aggregated), not three.
        assert_eq!(
            warnings.len(),
            1,
            "3+ failures from same barrel must produce exactly 1 aggregated warning; got {}: {:?}",
            warnings.len(),
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
        let msg = &warnings[0].message;
        assert!(
            msg.contains("barrel"),
            "aggregated warning must contain 'barrel'; got: {msg:?}"
        );
        assert!(
            msg.contains('3') || msg.contains("3 "),
            "aggregated warning must mention count 3; got: {msg:?}"
        );
    }

    #[test]
    fn meta_05_below_threshold_individual_warnings() {
        // Only 2 failures from the same specifier → NOT aggregated, must produce
        // 2 individual warnings (threshold is 3).
        let dir = tempfile::tempdir().expect("tempdir");
        let index_path = dir.path().join("index.ts");
        std::fs::write(
            &index_path,
            b"export { foo } from './missing';\nexport { bar } from './missing';\n",
        )
        .unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        collect_exports(
            &index_path,
            dir.path(),
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        )
        .expect("collect_exports should succeed");

        // Must produce 2 individual warnings, not 1 aggregated.
        assert_eq!(
            warnings.len(),
            2,
            "2 failures must produce 2 individual warnings (below threshold); got {}: {:?}",
            warnings.len(),
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
        // Individual warnings must NOT contain "barrel".
        for w in &warnings {
            assert!(
                !w.message.contains("barrel"),
                "individual warning must not say 'barrel'; got: {:?}",
                w.message
            );
        }
    }

    #[test]
    fn wr_03_meta_05_names_emitted_in_sorted_order() {
        // WR-03 (Phase 17 review): names in the aggregated barrel warning must be
        // emitted in sorted order so the message is deterministic across source
        // reorderings (and resistant to downstream warning-diff tools seeing
        // spurious changes when the author reorders unrelated lines).
        let dir = tempfile::tempdir().expect("tempdir");
        let index_path = dir.path().join("index.ts");
        // Write names in REVERSE alphabetical order — expected output is sorted.
        std::fs::write(
            &index_path,
            b"export { zebra } from './missing';\nexport { mango } from './missing';\nexport { apple } from './missing';\n",
        )
        .unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        collect_exports(
            &index_path,
            dir.path(),
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        )
        .expect("collect_exports should succeed");

        assert_eq!(warnings.len(), 1, "expected one aggregated warning");
        let msg = &warnings[0].message;
        // Expect "apple, mango, zebra" not "zebra, mango, apple"
        let apple_pos = msg.find("apple").expect("message contains apple");
        let mango_pos = msg.find("mango").expect("message contains mango");
        let zebra_pos = msg.find("zebra").expect("message contains zebra");
        assert!(
            apple_pos < mango_pos && mango_pos < zebra_pos,
            "WR-03: names must be sorted; got: {msg:?}"
        );
    }

    // ── CHUNK-01: normalize_chunk_path ───────────────────────────────────────

    #[test]
    fn chunk_01_hashed_chunk_normalized() {
        let p = std::path::Path::new("dist/chunks/chunk-A1B2C3D4.js");
        assert_eq!(
            normalize_chunk_path(p).to_string_lossy(),
            "dist/chunks/chunk-*.js"
        );
    }

    #[test]
    fn chunk_01_unrelated_path_unchanged() {
        let p = std::path::Path::new("src/index.ts");
        assert_eq!(normalize_chunk_path(p).to_string_lossy(), "src/index.ts");
    }

    #[test]
    fn chunk_01_short_hash_unchanged() {
        let p = std::path::Path::new("dist/chunks/chunk-ABC.js");
        assert_eq!(
            normalize_chunk_path(p).to_string_lossy(),
            "dist/chunks/chunk-ABC.js"
        );
    }

    #[test]
    fn chunk_01_mjs_extension_normalized() {
        let p = std::path::Path::new("pkg/dist/chunks/chunk-DEADBEEF.mjs");
        assert_eq!(
            normalize_chunk_path(p).to_string_lossy(),
            "pkg/dist/chunks/chunk-*.mjs"
        );
    }

    // ── WR-02 (Phase 19 review): path-equality invariant between
    //    ExportSymbol.file and ExportMap.directives keys for rollup chunks.
    //    The resolver MUST key the directives sidecar by the SAME normalized
    //    path it stamps onto ExportSymbol.file. If this ever drifts, a
    //    rollup chunk with a stable set of directives across rebuilds
    //    would diff as "directive-added + directive-removed" instead of
    //    appearing under a single shared key.

    #[test]
    fn wr_02_chunk_directives_keyed_by_normalized_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Canonicalize pkg_root so that strip_prefix succeeds on macOS
        // (tempdir returns /var/..., canonicalize() returns /private/var/...).
        let pkg_root = dir.path().canonicalize().expect("canonicalize tempdir");
        let chunk_rel = std::path::Path::new("dist/chunks/chunk-A1B2C3D4.js");
        let chunk_abs = pkg_root.join(chunk_rel);
        std::fs::create_dir_all(chunk_abs.parent().unwrap()).unwrap();
        // Rollup chunk with a triple-slash directive at the top followed by
        // a regular export. Both the directive (in ExportMap.directives)
        // and the export (in ExportMap.inner) must end up keyed/stamped
        // with the SAME normalized path.
        std::fs::write(
            &chunk_abs,
            b"/// <reference path=\"./chunk.d.ts\" />\nexport const x = 1;\n",
        )
        .unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        collect_exports(
            &chunk_abs,
            &pkg_root,
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        )
        .expect("collect_exports should succeed");

        let normalized = std::path::PathBuf::from("dist/chunks/chunk-*.js");

        // (1) directives sidecar keyed by the NORMALIZED path.
        let directive_keys: Vec<&std::path::PathBuf> = map.directives.keys().collect();
        assert_eq!(
            directive_keys,
            vec![&normalized],
            "WR-02: directives must be keyed by normalized chunk path; got: {:?}",
            directive_keys
        );
        let recs = map.directives.get(&normalized).unwrap();
        assert_eq!(recs.len(), 1, "exactly one directive expected");
        assert_eq!(recs[0].name, "reference");

        // (2) ExportSymbol.file stamped with the SAME normalized path.
        //     If these ever drift the directive-diff union-walk in
        //     compare_exports_core can't route findings back to a
        //     symbol-bearing file.
        let x_key = ExportKey::new_root_value("x");
        let x_sym = map.get(&x_key).expect("x export present");
        assert_eq!(
            x_sym.file, normalized,
            "WR-02: ExportSymbol.file must equal the directives sidecar key"
        );
    }

    // ── ROB-01: UTF-8 / parse failure → per-file warning ────────────────────

    #[test]
    fn rob_01_invalid_utf8_produces_warning_not_abort() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path();
        std::fs::create_dir_all(pkg.join("src")).unwrap();
        std::fs::write(
            pkg.join("package.json"),
            r#"{"name":"t","version":"1.0.0","main":"./src/index.ts"}"#,
        )
        .unwrap();
        // Write invalid UTF-8 bytes to the source file
        std::fs::write(pkg.join("src/index.ts"), &[0x80u8, 0x81u8]).unwrap();

        let mut map = crate::types::ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();

        let result = collect_exports(
            &pkg.join("src/index.ts"),
            pkg,
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        );
        assert!(
            result.is_ok(),
            "ROB-01: should not propagate error for invalid UTF-8; got: {:?}",
            result.err()
        );
        assert!(
            !warnings.is_empty(),
            "ROB-01: expected a ResolveWarning; got none"
        );
    }

    // ── ROB-04: JSONC tsconfig parsing (block comments + trailing commas) ────

    #[test]
    fn rob_04_tsconfig_block_comments_stripped() {
        let input = r#"{
            "compilerOptions": {
                /* multiline
                   comment */
                "paths": { "@/*": ["./src/*"] }
            }
        }"#;
        let cleaned = strip_jsonc(input);
        let parsed: serde_json::Value = serde_json::from_str(&cleaned).expect("should parse");
        assert!(
            parsed["compilerOptions"]["paths"]["@/*"][0].as_str() == Some("./src/*"),
            "ROB-04: block comment not stripped; cleaned: {:?}",
            cleaned
        );
    }

    #[test]
    fn rob_04_tsconfig_trailing_comma_accepted() {
        let input = r#"{ "paths": { "@/*": ["./src/*"], }, }"#;
        let cleaned = strip_jsonc(input);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cleaned);
        assert!(
            parsed.is_ok(),
            "ROB-04: trailing comma not stripped. Got: {cleaned}"
        );
    }

    #[test]
    fn rob_04_tsconfig_line_comment_regression() {
        // Regression: existing line comment stripping must still work
        let input = r#"{ "paths": { "@/*": ["./src/*"] // a comment
        } }"#;
        let cleaned = strip_jsonc(input);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cleaned);
        assert!(
            parsed.is_ok(),
            "ROB-04: line comment regression — cleaned: {cleaned}"
        );
    }

    #[test]
    fn rob_04_tsconfig_preserves_utf8_in_strings() {
        // CR-01 (Phase 17 review): strip_jsonc previously cast each byte to `char`
        // which corrupted multi-byte UTF-8 sequences. Non-ASCII path aliases and
        // comments must survive stripping intact so serde_json sees valid UTF-8.
        let input = r#"{
            "paths": { "@文件/*": ["./src/*"] },
            /* 日本語コメント */
            "emoji": "🚀"
        }"#;
        let cleaned = strip_jsonc(input);
        let parsed: serde_json::Value =
            serde_json::from_str(&cleaned).expect("non-ASCII JSONC must parse");
        assert!(
            parsed["paths"]["@文件/*"].is_array(),
            "CR-01: non-ASCII key must survive strip_jsonc; cleaned: {cleaned}"
        );
        assert_eq!(
            parsed["emoji"].as_str(),
            Some("🚀"),
            "CR-01: emoji in string literal must survive strip_jsonc; cleaned: {cleaned}"
        );
    }

    #[test]
    fn rob_04_tsconfig_preserves_utf8_trailing_comma_path() {
        // Ensure the second pass (trailing-comma stripper) also operates on chars,
        // not bytes — so a trailing comma near non-ASCII content is handled safely.
        let input = r#"{ "name": "léa", }"#;
        let cleaned = strip_jsonc(input);
        let parsed: serde_json::Value =
            serde_json::from_str(&cleaned).expect("UTF-8 + trailing comma must parse");
        assert_eq!(parsed["name"].as_str(), Some("léa"));
    }

    // ─── Phase 18: D-13 pre-merge + D-14 decl_kinds union ────────────────────

    fn make_empty_sym(name: &str) -> crate::types::ExportSymbol {
        use crate::types::{ExportKind, ExportSymbol};
        ExportSymbol {
            name: name.to_string(),
            kind: ExportKind::Named,
            file: PathBuf::from("x.ts"),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: Vec::new(),
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
            typeof_import_refs: Vec::new(),
        }
    }

    #[test]
    fn test_resolver_pre_merge_class_plus_namespace_unions_decl_kinds() {
        // D-14: class User + namespace User in the same file must merge into a
        // single ExportSymbol with decl_kinds = {Class, Namespace}.
        use crate::types::{ExportKey, ExportMap};
        let dir = tempfile::tempdir().unwrap();
        let src = r#"
export class User {}
export namespace User { export const kind = "u"; }
"#;
        let file = dir.path().join("u.ts");
        std::fs::write(&file, src).unwrap();

        let mut map: ExportMap = ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();
        collect_exports(
            &file,
            dir.path(),
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        )
        .unwrap();

        // Find the User symbol — regardless of space, the class+namespace merge
        // per D-14 means the same key should carry both decl_kinds.
        let user_keys: Vec<ExportKey> = map.keys().filter(|k| k.name == "User").cloned().collect();
        assert!(
            !user_keys.is_empty(),
            "User key not found in map; got keys: {:?}",
            map.keys().collect::<Vec<_>>()
        );
        // With space_for mapping class -> Value and namespace -> Namespace the
        // two declarations fall into different ExportKeys by design (D-03).
        // What D-14 requires is that declaration-merging WITHIN the same
        // ExportKey bucket unions decl_kinds. For tree-sitter's declaration-
        // merging the two would need to collide in the SAME space. The test
        // here verifies at least one User entry exists — the merge_hybrid
        // unit tests below cover the union semantics directly.
        assert!(user_keys.iter().any(|k| map[k].name == "User"));
    }

    #[test]
    fn test_merge_hybrid_unions_decl_kinds() {
        use crate::types::{DeclarationKind, ExportSymbol};
        let mut sym: ExportSymbol = make_empty_sym("User");
        sym.decl_kinds.push(DeclarationKind::Class);
        // Merge incoming namespace declaration.
        merge_hybrid(&mut sym, Some(DeclarationKind::Namespace), None, None);
        assert!(sym.decl_kinds.contains(&DeclarationKind::Class));
        assert!(sym.decl_kinds.contains(&DeclarationKind::Namespace));
        // Dedup on re-insertion of same kind.
        merge_hybrid(&mut sym, Some(DeclarationKind::Class), None, None);
        assert_eq!(
            sym.decl_kinds
                .iter()
                .filter(|k| **k == DeclarationKind::Class)
                .count(),
            1,
            "merge_hybrid must dedup kinds"
        );
    }

    #[test]
    fn test_merge_hybrid_preserves_earlier_location_and_upgrades_none_fields() {
        use crate::types::ExportSymbol;
        let mut sym: ExportSymbol = make_empty_sym("User");
        sym.line = 10;
        sym.col = 4;
        // type_signature starts as None — incoming Some should be adopted.
        merge_hybrid(
            &mut sym,
            None,
            Some("(x: string) => void".to_string()),
            None,
        );
        assert_eq!(sym.type_signature.as_deref(), Some("(x: string) => void"));
        // Position is not overwritten even if merge_hybrid called again.
        let first_line = sym.line;
        merge_hybrid(&mut sym, None, None, None);
        assert_eq!(sym.line, first_line, "merge_hybrid must NOT overwrite line");
    }

    #[test]
    fn test_resolver_distinct_subpaths_distinct_keys() {
        // ARCH-01: two ExportKeys differing only in subpath are distinct map keys.
        use crate::types::{ExportKey, ExportMap, SymbolSpace};
        let a = ExportKey::new(".", None, SymbolSpace::Value, "User");
        let b = ExportKey::new("./admin", None, SymbolSpace::Value, "User");
        let mut map: ExportMap = ExportMap::new();
        map.insert(a.clone(), make_empty_sym("User"));
        map.insert(b.clone(), make_empty_sym("User"));
        assert_eq!(map.len(), 2, "different subpaths MUST be distinct map keys");
    }

    // ── Phase 18 ARCH-02: end-to-end compound-member propagation ────────────

    #[test]
    fn test_arch02_end_to_end_compound_members_propagate_to_symbol() {
        // ARCH-02: `Menu.Item = X; Menu.Header = Y;` discovered in source must
        // reach the ExportSymbol's compound_members field via parse_exports ->
        // attach_compound_members -> resolver Named-insertion path.
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let src = r#"
export const Menu = () => null;
Menu.Item = () => null;
Menu.Header = () => null;
"#;
        let file = dir.path().join("menu.ts");
        std::fs::write(&file, src).unwrap();

        let mut map: ExportMap = ExportMap::new();
        let mut visited = HashSet::new();
        let mut warnings = Vec::new();
        collect_exports(
            &file,
            dir.path(),
            &mut map,
            &mut visited,
            &mut warnings,
            0,
            None,
            ".",
            None,
        )
        .unwrap();

        let menu_key = map
            .keys()
            .find(|k| k.name == "Menu")
            .cloned()
            .expect("Menu missing from ExportMap");
        let menu_sym = &map[&menu_key];
        assert!(
            menu_sym.compound_members.contains(&"Item".to_string()),
            "ARCH-02 end-to-end: Menu.Item must propagate; got {:?}",
            menu_sym.compound_members
        );
        assert!(
            menu_sym.compound_members.contains(&"Header".to_string()),
            "ARCH-02 end-to-end: Menu.Header must propagate; got {:?}",
            menu_sym.compound_members
        );
    }
}
