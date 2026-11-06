//! Discover exports and their source declarations in JavaScript and TypeScript.

use crate::languages::javascript::model as js_model;
use crate::languages::javascript::model::DeclarationKind;
use crate::languages::javascript::syntax::{
    commonjs, grammar, property_key, queries, runtime_surface,
};
use crate::languages::typescript::model as ts_model;
#[cfg(test)]
use crate::languages::typescript::model::EnumMember;
use crate::languages::typescript::syntax::annotations;
use crate::languages::typescript::types::shape as type_shapes;
use anyhow::Context;
use tree_sitter::{QueryCursor, StreamingIterator};

/// The kind of export as detected by tree-sitter — before semantic resolution.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawExportKind {
    /// `export { foo }` or `export { foo as bar }` (no `from` clause)
    Named,
    /// A symbol-keyed CommonJS property, separate from string export names.
    Symbol,
    /// `export default …`
    Default,
    /// `export * from './module'`
    StarReexport,
    /// `export { foo } from './module'`
    NamedReexport,
}

/// A single export statement captured by tree-sitter before semantic enrichment.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RawExport {
    /// The exported name. For `Default` this is `"default"`. For `StarReexport`
    /// this is `"*"`.
    pub name: String,
    pub kind: RawExportKind,
    /// One-based line number; tree-sitter positions are zero-based.
    pub line: u32,
    /// 1-indexed column number.
    pub col: u32,
    /// The module specifier string for re-exports (e.g., `"./utils"`), with
    /// surrounding quotes stripped. `None` for non-re-export forms.
    pub source: Option<String>,
    /// The syntactic form of the declaration (function, class, interface, etc.).
    /// `None` for re-exports, default expression-form exports, and plain `export { foo }`.
    pub decl_kind: Option<DeclarationKind>,
    /// TLF-07: true for `export type * from './mod'` (type-only star re-export).
    pub is_type_only: bool,
    /// META-04: TSDoc release tag extracted from adjacent block comment.
    /// Populated during parse_exports for named declaration exports.
    pub release_tag: Option<ts_model::ReleaseTag>,
    /// ARCH-02: compound-component members attached to this export
    /// via `Target.Member = X` or `Object.assign(Target, { Member: X })`.
    /// Populated during the compound-assignment second pass in `parse_exports`.
    /// conservative: only populated when Target is itself an exported
    /// symbol in the same file.
    pub compound_members: Vec<String>,
    /// INF-02: type annotation text extracted from a matching
    /// `declare` statement in the same file (rollup-plugin-dts / api-extractor
    /// bundle pattern: `declare const foo: T;... export { foo };`). Populated
    /// during `parse_exports` Pass 1 by lookup against the file-scope ambient-
    /// declaration sidecar map. `None` for source-form exports where no
    /// `declare` statement precedes the `export { foo }` specifier and for
    /// re-exports where the symbol comes from another module.
    pub type_signature: Option<String>,
    /// TYPE-01: `satisfies T` RHS — populated by the resolver's
    /// post-parse populate pass that calls `extract_satisfies_types` and
    /// routes each hit through `parse_type_string`. `None` for exports that
    /// have no adjacent `satisfies` clause, and on every initial construction
    /// from this module (the populate pass is the single producer).
    pub satisfies_type: Option<type_shapes::TypeShape>,
    /// INF-04: `typeof import('./y').Z` references in this raw
    /// export's type position. Empty by default; populated by
    /// `extract_typeof_import_refs` in the resolver's post-parse populate
    /// pass alongside the TYPE-01 satisfies-type populate. Each entry is
    /// later forwarded onto the resolved `ExportSymbol.typeof_import_refs`
    /// at the resolver's insertion sites.
    pub typeof_import_refs: Vec<ts_model::TypeofImportRef>,
    /// DOC-01: true when this RawExport was synthesized from a
    /// `module.exports = new Proxy(...)` or `module.exports = Reflect.construct(...)`
    /// RHS. The comparator emits a paired Severity::Info finding with rule
    /// `limitation: proxy-dispatched-exports` when transitioning to or from this
    /// shape. Default `false` for all non-proxy export forms.
    pub is_proxy_dispatched: bool,
    /// Original declaration name, independent of the public export alias.
    pub local_name: Option<String>,
    /// Declaration coordinates; export-site coordinates remain available for metadata.
    pub declaration_site: Option<(u32, u32)>,
}

/// INF-02 Pass 1: build a file-scope sidecar of top-level `declare` statements so that
/// Pass 2 (the existing export-statement processing) can enrich `export { foo }`
/// entries with the matching declare site's decl_kind and type_signature.
///
/// rollup-plugin-dts and api-extractor ship declare + export as sibling top-level
/// nodes; the existing parse_exports treats the plain `export { foo }` as a
/// typeless re-export. INF-02 restores the type identity by lookup.
///
/// On duplicate name (rare — same identifier declared twice), first-seen wins
/// using entry.or_insert semantics.
///
/// Query-compile failures are handled gracefully: each
/// AMBIENT_*_DECL_QUERY is wrapped in `if let Ok(q)` and a failing query silently
/// skips that pattern.
fn collect_ambient_decls(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    lang: &grammar::SourceLanguage,
) -> std::collections::HashMap<String, (DeclarationKind, Option<String>)> {
    use std::collections::HashMap;
    let mut decls: HashMap<String, (DeclarationKind, Option<String>)> = HashMap::new();
    let language = grammar::get_language(lang);

    // Tuple element semantics:
    //  (query_str, kind, capture_type)
    // where `capture_type = true` means the query exposes a `@decl_type` capture
    // (variable/type-annotation form) whose text becomes the type_signature.
    // Otherwise `@decl_node` whole-node text (minus trailing `;`) is used as the
    // type_signature stand-in for function signatures; for class/interface/
    // enum/type-alias the type_signature stays None (name + decl_kind alone is
    // enough for downstream identity — downstream consumers care most about
    // the variable-type case).
    let queries = [
        (
            queries::AMBIENT_CONST_DECL_QUERY,
            DeclarationKind::Variable,
            true,
        ),
        (
            queries::AMBIENT_FUNCTION_DECL_QUERY,
            DeclarationKind::Function,
            false,
        ),
        (
            queries::AMBIENT_CLASS_DECL_QUERY,
            DeclarationKind::Class,
            false,
        ),
        (
            queries::AMBIENT_INTERFACE_DECL_QUERY,
            DeclarationKind::Interface,
            false,
        ),
        (
            queries::AMBIENT_TYPE_ALIAS_DECL_QUERY,
            DeclarationKind::TypeAlias,
            false,
        ),
        (
            queries::AMBIENT_ENUM_DECL_QUERY,
            DeclarationKind::Enum,
            false,
        ),
    ];

    for (query_str, kind, capture_type) in queries {
        let Ok(q) = grammar::cached_query(&language, query_str) else {
            continue;
        };
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&q, root, source);
        let capture_names = q.capture_names();
        while let Some(m) = matches_iter.next() {
            let mut name: Option<String> = None;
            let mut type_text: Option<String> = None;
            let mut decl_node_text: Option<String> = None;
            for cap in m.captures {
                let cap_name = capture_names[cap.index as usize];
                let text = cap.node.utf8_text(source).ok().map(|s| s.to_string());
                match cap_name {
                    "decl_name" => name = text,
                    "decl_type" => {
                        // `type_annotation` node text includes the leading `:` and any
                        // leading whitespace — trim so callers see the bare type.
                        type_text = text.map(|t| t.trim_start_matches(':').trim().to_string())
                    }
                    "decl_node" => decl_node_text = text,
                    _ => {}
                }
            }
            if let Some(n) = name {
                let type_sig = if capture_type {
                    type_text
                } else {
                    // Function signature: use full node text (minus trailing `;`)
                    // as a coarse type_signature. `None` for class/interface/
                    // enum/type-alias (kind alone conveys identity).
                    match kind {
                        DeclarationKind::Function => {
                            decl_node_text.map(|s| s.trim_end_matches(';').trim().to_string())
                        }
                        _ => None,
                    }
                };
                decls.entry(n).or_insert((kind.clone(), type_sig));
            }
        }
    }
    decls
}

/// Map a tree-sitter node kind string to a DeclarationKind.
fn decl_kind_from_node_kind(kind: &str) -> DeclarationKind {
    match kind {
        "function_declaration" | "function_signature" | "generator_function_declaration" => {
            DeclarationKind::Function
        }
        "class_declaration" | "abstract_class_declaration" => DeclarationKind::Class,
        "interface_declaration" => DeclarationKind::Interface,
        "type_alias_declaration" => DeclarationKind::TypeAlias,
        "enum_declaration" => DeclarationKind::Enum,
        "internal_module" => DeclarationKind::Namespace,
        "lexical_declaration" | "variable_declaration" => DeclarationKind::Variable,
        _ => DeclarationKind::Unknown,
    }
}

/// Parse `source` bytes with the given grammar and return all detected exports.
///
/// Line and column numbers in the returned `RawExport`s are **1-indexed**
/// (tree-sitter provides 0-indexed positions; this function adds 1 to both).
#[allow(dead_code)]
pub fn parse_exports(
    source: &[u8],
    lang: grammar::SourceLanguage,
) -> anyhow::Result<Vec<RawExport>> {
    let language = grammar::get_language(&lang);

    let tree = grammar::parse_source(source, &lang)?;

    let root = tree.root_node();
    let mut exports: Vec<RawExport> = Vec::new();

    // INF-02 Pass 1: collect top-level `declare` statements keyed by
    // identifier. The resulting sidecar is consulted post-collection by the
    // link-pass below to enrich rollup-plugin-dts / api-extractor split
    // declarations (`declare const foo: T;... export { foo };`) with the
    // declare-site decl_kind and type_signature. Empty HashMap for non-TS or
    // TS files without ambient declarations — link-pass no-ops.
    let ambient_decls = collect_ambient_decls(root, source, &lang);

    // --- Named exports (no `from` clause) ------------------------------------
    // We run the named re-export query first to collect (name, source) pairs,
    // then use that set to exclude them from the plain named export results.
    let reexport_positions = collect_reexport_positions(root, source, &language)?;

    // Tracks (line, col) positions of names captured by type-only re-export queries
    // so the later NAMED_REEXPORT_QUERY section can skip them (avoiding double-emit).
    let mut type_only_reexport_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();

    // --- `export { default as Named }` forms ----------------------
    // Process BEFORE regular named export queries to set positions for dedup.
    // The `default` keyword is an unnamed node in the grammar, so existing
    // identifier-based queries miss it.

    // 1. `export { default as Named } from './foo'` — re-export with alias
    if let Ok(query) =
        grammar::cached_query(&language, queries::DEFAULT_SPECIFIER_ALIAS_REEXPORT_QUERY)
    {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            let mut alias_opt: Option<(String, u32, u32)> = None;
            let mut src_opt: Option<String> = None;
            for capture in m.captures {
                let node = capture.node;
                if capture.index == 0 {
                    // @alias_name
                    if let Ok(text) = node.utf8_text(source) {
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        alias_opt = Some((text.to_string(), line, col));
                    }
                } else if capture.index == 1 {
                    // @source
                    if let Ok(text) = node.utf8_text(source) {
                        src_opt = Some(strip_quotes(text).to_string());
                    }
                }
            }
            if let (Some((name, line, col)), Some(src)) = (alias_opt, src_opt) {
                exports.push(RawExport {
                    name,
                    kind: RawExportKind::NamedReexport,
                    line,
                    col,
                    source: Some(src),
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                    declaration_site: None,
                });
            }
        }
    }

    // 2. `export { default } from './foo'` — re-export default (no alias)
    if let Ok(query) = grammar::cached_query(&language, queries::DEFAULT_SPECIFIER_REEXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @source
                    let node = capture.node;
                    if let Ok(text) = node.utf8_text(source) {
                        let src = strip_quotes(text).to_string();
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        let already = exports
                            .iter()
                            .any(|e| e.name == "default" && e.source.as_deref() == Some(&src));
                        if !already {
                            exports.push(RawExport {
                                name: "default".to_string(),
                                kind: RawExportKind::NamedReexport,
                                line,
                                col,
                                source: Some(src),
                                decl_kind: None,
                                is_type_only: false,
                                release_tag: None,
                                compound_members: Vec::new(),
                                type_signature: None,
                                satisfies_type: None,
                                typeof_import_refs: Vec::new(),
                                is_proxy_dispatched: false,
                                local_name: None,
                                declaration_site: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // 3. `export { default as Named }` — local default aliased to Named (no from clause)
    if let Ok(query) = grammar::cached_query(&language, queries::DEFAULT_SPECIFIER_ALIAS_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @alias_name
                    let node = capture.node;
                    if let Ok(text) = node.utf8_text(source) {
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        exports.push(RawExport {
                            name: text.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // Process aliased named exports first: `export { foo as bar }`.
    // We track their local-name positions so the plain query below skips them.
    let mut alias_local_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();
    {
        let query = grammar::cached_query(&language, queries::NAMED_EXPORT_ALIAS_QUERY)
            .context("failed to compile NAMED_EXPORT_ALIAS_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // index 0 = @local_name, index 1 = @alias_name
            let mut local_pos: Option<(u32, u32)> = None;
            let mut alias_name_opt: Option<(String, u32, u32)> = None;
            for capture in m.captures {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    local_pos = Some((line, col));
                } else if capture.index == 1 {
                    // alias_name is the PUBLIC exported name
                    if !reexport_positions.contains(&(line, col)) {
                        let name = node
                            .utf8_text(source)
                            .context("UTF-8 decode failed on alias name")?
                            .to_string();
                        alias_name_opt = Some((name, line, col));
                    }
                }
            }
            if let (Some(lp), Some((name, line, col))) = (local_pos, alias_name_opt) {
                alias_local_positions.insert(lp);
                // INF-03: tsup/esbuild ESM trailer
                // `export { foo as default }`. The alias_name is the PUBLIC
                // exported name — when it is the keyword `default`, the
                // specifier is semantically a default-export, not a named
                // export with name "default". Emit `RawExport { name:
                // "default", kind: Default }` so downstream resolver keying
                // treats this as the default export (the resolver's default
                // path is keyed as `"default"` and carries distinct merge
                // semantics). Non-"default" aliases preserve
                // Named behavior — `export { foo as Baz }` remains a Named
                // export with name "Baz".
                let export_kind = if name == "default" {
                    RawExportKind::Default
                } else {
                    RawExportKind::Named
                };
                exports.push(RawExport {
                    name,
                    kind: export_kind,
                    line,
                    col,
                    source: None,
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                    declaration_site: None,
                });
            }
        }
    }

    // INF-06: JS-specific `export { foo as default }` alias capture.
    // The grammar uses an anonymous `default` keyword token for this alias,
    // which the identifier-typed NAMED_EXPORT_ALIAS_QUERY cannot match.
    // Shares `alias_local_positions` with the existing ALIAS loop for dedup.
    if matches!(lang, grammar::SourceLanguage::JavaScript) {
        if let Ok(query) =
            grammar::cached_query(&language, queries::NAMED_EXPORT_ALIAS_DEFAULT_QUERY_JS)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // cap 0 = @local_name (identifier), cap 1 = @alias_default ("default" keyword)
                let mut local_pos: Option<(u32, u32)> = None;
                let mut alias_pos: Option<(u32, u32)> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        local_pos = Some((line, col));
                    } else if capture.index == 1 {
                        alias_pos = Some((line, col));
                    }
                }
                if let (Some(lp), Some((line, col))) = (local_pos, alias_pos) {
                    // Dedup against the existing NAMED_EXPORT_ALIAS_QUERY loop.
                    if alias_local_positions.contains(&lp) {
                        continue;
                    }
                    alias_local_positions.insert(lp);
                    exports.push(RawExport {
                        name: "default".to_string(),
                        kind: RawExportKind::Default,
                        line,
                        col,
                        source: None,
                        decl_kind: None,
                        is_type_only: false,
                        release_tag: None,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                        typeof_import_refs: Vec::new(),
                        is_proxy_dispatched: false,
                        local_name: None,
                        declaration_site: None,
                    });
                }
            }
        }
    }

    // Process non-aliased named exports: `export { foo }`.
    // Skip positions already handled by the alias query or re-export query.
    {
        let query = grammar::cached_query(&language, queries::NAMED_EXPORT_QUERY)
            .context("failed to compile NAMED_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @name
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;

                    // Skip if this position belongs to a named re-export.
                    if reexport_positions.contains(&(line, col)) {
                        continue;
                    }
                    // Skip if already handled by the alias query above.
                    if alias_local_positions.contains(&(line, col)) {
                        continue;
                    }

                    let name = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on export name")?
                        .to_string();
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::Named,
                        line,
                        col,
                        source: None,
                        decl_kind: None,
                        is_type_only: false,
                        release_tag: None,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                        typeof_import_refs: Vec::new(),
                        is_proxy_dispatched: false,
                        local_name: None,
                        declaration_site: None,
                    });
                }
            }
        }
    }

    // --- Type-only exports (TypeScript only: `export type { Foo }`) ------------
    // TS 3.8+ type-only export forms. The `"type"` keyword is an anonymous token.
    // These are tracked as Named/NamedReexport — same kind as regular named exports.
    // Processing order: alias forms first (to set type_only_alias_local_positions),
    // then non-alias forms (skipping positions handled by alias queries).
    // We also skip positions already in reexport_positions to avoid double-counting.
    // INF-01: TypeScriptTsx included — TSX is a strict superset; every TS query matches identically.
    if matches!(
        lang,
        grammar::SourceLanguage::TypeScript | grammar::SourceLanguage::TypeScriptTsx
    ) {
        let mut type_only_alias_local_positions: std::collections::HashSet<(u32, u32)> =
            std::collections::HashSet::new();

        // Process `export type { Foo as Bar }` (no from clause)
        if let Ok(query) =
            grammar::cached_query(&language, queries::TYPE_ONLY_ALIAS_EXPORT_QUERY_TS)
        {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut local_pos: Option<(u32, u32)> = None;
                let mut alias_name_opt: Option<(String, u32, u32)> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        // @local_name
                        local_pos = Some((line, col));
                    } else if capture.index == 1 {
                        // @alias_name — public exported name; skip if it's a re-export
                        if !reexport_positions.contains(&(line, col)) {
                            if let Ok(name) = node.utf8_text(source) {
                                alias_name_opt = Some((name.to_string(), line, col));
                            }
                        }
                    }
                }
                if let (Some(lp), Some((name, line, col))) = (local_pos, alias_name_opt) {
                    type_only_alias_local_positions.insert(lp);
                    // Skip if already captured by the non-type-only alias query
                    if !alias_local_positions.contains(&lp) {
                        exports.push(RawExport {
                            name,
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }

        // Process `export type { Foo as Bar } from './bar'` (with source)
        if let Ok(query) =
            grammar::cached_query(&language, queries::TYPE_ONLY_ALIAS_REEXPORT_QUERY_TS)
        {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut local_pos: Option<(u32, u32)> = None;
                let mut alias_name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        local_pos = Some((line, col));
                    } else if capture.index == 1 {
                        if let Ok(name) = node.utf8_text(source) {
                            alias_name_opt = Some((name.to_string(), line, col));
                        }
                    } else if capture.index == 2 {
                        if let Ok(text) = node.utf8_text(source) {
                            src_opt = Some(strip_quotes(text).to_string());
                        }
                    }
                }
                if let (Some(lp), Some((name, line, col)), Some(src)) =
                    (local_pos, alias_name_opt, src_opt)
                {
                    type_only_alias_local_positions.insert(lp);
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::NamedReexport,
                        line,
                        col,
                        source: Some(src),
                        decl_kind: None,
                        is_type_only: false,
                        release_tag: None,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                        typeof_import_refs: Vec::new(),
                        is_proxy_dispatched: false,
                        local_name: None,
                        declaration_site: None,
                    });
                }
            }
        }

        // Process `export type { Foo }` (no alias, no from clause)
        if let Ok(query) =
            grammar::cached_query(&language, queries::TYPE_ONLY_NAMED_EXPORT_QUERY_TS)
        {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        let node = capture.node;
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        // Skip if already a re-export position
                        if reexport_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if handled by alias query above
                        if type_only_alias_local_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if already handled by the non-type-only alias query
                        if alias_local_positions.contains(&(line, col)) {
                            continue;
                        }
                        // Skip if already captured by the non-type-only named export query
                        // (the existing NAMED_EXPORT_QUERY also matches these nodes)
                        let already = exports.iter().any(|e| {
                            e.kind == RawExportKind::Named && e.line == line && e.col == col
                        });
                        if !already {
                            if let Ok(name) = node.utf8_text(source) {
                                exports.push(RawExport {
                                    name: name.to_string(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: None,
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                    typeof_import_refs: Vec::new(),
                                    is_proxy_dispatched: false,
                                    local_name: None,
                                    declaration_site: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Process `export type { Foo } from './bar'` (no alias, with source)
        if let Ok(query) =
            grammar::cached_query(&language, queries::TYPE_ONLY_NAMED_REEXPORT_QUERY_TS)
        {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if capture.index == 0 {
                        // @name
                        if type_only_alias_local_positions.contains(&(line, col)) {
                            break; // alias variant already handled
                        }
                        if let Ok(name) = node.utf8_text(source) {
                            name_opt = Some((name.to_string(), line, col));
                        }
                    } else if capture.index == 1 {
                        if let Ok(text) = node.utf8_text(source) {
                            src_opt = Some(strip_quotes(text).to_string());
                        }
                    }
                }
                if let (Some((name, line, col)), Some(src)) = (name_opt, src_opt) {
                    // Track this position so the later NAMED_REEXPORT_QUERY section
                    // skips it and avoids emitting a duplicate NamedReexport entry.
                    type_only_reexport_positions.insert((line, col));
                    // Skip if already captured by non-type-only named re-export query
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::NamedReexport && e.line == line && e.col == col
                    });
                    if !already {
                        exports.push(RawExport {
                            name,
                            kind: RawExportKind::NamedReexport,
                            line,
                            col,
                            source: Some(src),
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // --- Default exports (expression form) ------------------------------------
    // Captures `export default <expr>` where the default value is an expression
    // (e.g. `export default 42`, `export default obj`, `export default function() {}`).
    // Named default declarations (`export default class Foo {}`) are handled below.
    {
        let query = grammar::cached_query(&language, queries::DEFAULT_EXPORT_QUERY)
            .context("failed to compile DEFAULT_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // @value gives us the position of the default export expression.
            if let Some(capture) = m.captures.first() {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                exports.push(RawExport {
                    name: "default".to_string(),
                    kind: RawExportKind::Default,
                    line,
                    col,
                    source: None,
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                    declaration_site: None,
                });
            }
        }
    }

    // --- Default exports (declaration form) -----------------------------------
    // Captures `export default function foo() {}` and `export default class Foo {}`
    // which appear as `declaration:` children rather than `value:` children.
    // We only emit a "default" entry if one hasn't already been added above.
    {
        let query = grammar::cached_query(&language, queries::DEFAULT_DECL_EXPORT_QUERY)
            .context("failed to compile DEFAULT_DECL_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            if let Some(capture) = m.captures.first() {
                let node = capture.node;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                // Only add if no default has been emitted for this position yet.
                let already_emitted = exports
                    .iter()
                    .any(|e| e.kind == RawExportKind::Default && e.line == line && e.col == col);
                if !already_emitted {
                    exports.push(RawExport {
                        name: "default".to_string(),
                        kind: RawExportKind::Default,
                        line,
                        col,
                        source: None,
                        decl_kind: None,
                        is_type_only: false,
                        release_tag: None,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                        typeof_import_refs: Vec::new(),
                        is_proxy_dispatched: false,
                        local_name: None,
                        declaration_site: None,
                    });
                }
            }
        }
    }

    // --- Export assignment (`export = Foo`, TypeScript only) -----------------
    // CommonJS-style TypeScript default export. Produces a "default" entry.
    // Only emit if no default export already exists at this position.
    // INF-01: TypeScriptTsx included — `export = Foo` is TS syntax
    // the TSX grammar also parses.
    if matches!(
        lang,
        grammar::SourceLanguage::TypeScript | grammar::SourceLanguage::TypeScriptTsx
    ) {
        if let Ok(query) = grammar::cached_query(&language, queries::EXPORT_ASSIGNMENT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                if let Some(capture) = m.captures.first() {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    // Only add if no default export has already been emitted
                    let already_emitted = exports
                        .iter()
                        .any(|e| e.kind == RawExportKind::Default && e.line == line);
                    if !already_emitted {
                        exports.push(RawExport {
                            name: "default".to_string(),
                            kind: RawExportKind::Default,
                            line,
                            col,
                            source: None,
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // --- Declaration exports --------------------------------------------------
    // Use grammar-specific queries: TypeScript uses `type_identifier` for class/
    // interface/type names; JavaScript uses `identifier` for all names.
    //
    // We skip any declaration that is under `export default` (which has a
    // "default" keyword child in the parent export_statement node) — those are
    // handled by the default export queries above.
    {
        // INF-01: TypeScriptTsx shares DECL_EXPORT_QUERY_TS with
        // TypeScript — TSX is a strict superset of LANGUAGE_TYPESCRIPT's query
        // surface.
        let decl_query_str = match lang {
            grammar::SourceLanguage::TypeScript | grammar::SourceLanguage::TypeScriptTsx => {
                queries::DECL_EXPORT_QUERY_TS
            }
            grammar::SourceLanguage::JavaScript => queries::DECL_EXPORT_QUERY_JS,
        };
        let query = grammar::cached_query(&language, decl_query_str)
            .context("failed to compile DECL_EXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @name
                    let node = capture.node;

                    // Skip if this declaration is under `export default …`.
                    // The export_statement parent will have a child with kind "default".
                    let is_default_export = node
                        .parent() // declaration node
                        .and_then(|decl| decl.parent()) // export_statement
                        .map(|stmt| {
                            (0..stmt.child_count() as u32).any(|i| {
                                stmt.child(i)
                                    .map(|c| c.kind() == "default")
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false);

                    if is_default_export {
                        continue;
                    }

                    let name = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on declaration name")?
                        .to_string();
                    // Get the declaration node kind from the parent of the captured @name node.
                    // The @name node's parent is the declaration node (function_declaration, etc.).
                    let decl_node = node.parent();
                    let decl_kind = decl_node
                        .as_ref()
                        .map(|parent| decl_kind_from_node_kind(parent.kind()));
                    // META-04: extract TSDoc release tag from preceding block comment.
                    let release_tag = decl_node
                        .as_ref()
                        .and_then(|decl| annotations::extract_release_tag(*decl, source));
                    exports.push(RawExport {
                        name,
                        kind: RawExportKind::Named,
                        line: node.start_position().row as u32 + 1,
                        col: node.start_position().column as u32 + 1,
                        source: None,
                        decl_kind,
                        is_type_only: false,
                        release_tag,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                        typeof_import_refs: Vec::new(),
                        is_proxy_dispatched: false,
                        local_name: None,
                        declaration_site: None,
                    });
                }
            }
        }
    }

    // --- Ambient declaration exports (`export declare...`, TypeScript only) ---
    // In `.d.ts` files and `declare` blocks, declarations are wrapped in
    // `ambient_declaration`. The existing DECL_EXPORT_QUERY_TS doesn't match
    // through this wrapper.
    // INF-01: TypeScriptTsx included — ambient declarations also
    // appear in `.tsx` source (e.g., `declare global {... }` inside a JSX-
    // bearing component file).
    if matches!(
        lang,
        grammar::SourceLanguage::TypeScript | grammar::SourceLanguage::TypeScriptTsx
    ) {
        if let Ok(query) = grammar::cached_query(&language, queries::AMBIENT_DECL_EXPORT_QUERY_TS) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        let node = capture.node;
                        let name = match node.utf8_text(source) {
                            Ok(n) => n.to_string(),
                            Err(_) => continue,
                        };

                        // Skip if already captured (ambient + non-ambient declarations
                        // on the same symbol would be redundant).
                        let already = exports.iter().any(|e| e.name == name);
                        if already {
                            continue;
                        }

                        // The captured @name node's parent is the inner declaration
                        // (e.g., function_declaration) OR variable_declarator (for lexical_declaration).
                        // For lexical_declaration: @name → variable_declarator → lexical_declaration.
                        // We need to check the correct level for decl_kind.
                        let immediate_parent = node.parent();
                        let decl_kind = immediate_parent.map(|ip| {
                            let parent_kind = ip.kind();
                            if parent_kind == "variable_declarator" {
                                // Navigate up one more level to lexical_declaration
                                ip.parent()
                                    .map(|ld| decl_kind_from_node_kind(ld.kind()))
                                    .unwrap_or(js_model::DeclarationKind::Variable)
                            } else {
                                decl_kind_from_node_kind(parent_kind)
                            }
                        });

                        // META-04: extract TSDoc release tag from preceding block comment.
                        // For ambient declarations, the relevant node to check is the
                        // inner declaration (parent of @name).
                        let release_tag = node
                            .parent()
                            .and_then(|decl| annotations::extract_release_tag(decl, source));

                        exports.push(RawExport {
                            name,
                            kind: RawExportKind::Named,
                            line: node.start_position().row as u32 + 1,
                            col: node.start_position().column as u32 + 1,
                            source: None,
                            decl_kind,
                            is_type_only: false,
                            release_tag,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // --- Destructured variable exports (`export const { foo, bar } = obj`) ---
    // These are variable exports where the name field is an object_pattern instead of
    // an identifier. The existing DECL_EXPORT_QUERY misses these because it only
    // handles `variable_declarator name: (identifier)`.
    // Works for both JS and TS.

    // Shorthand patterns: `export const { foo, bar } = obj`
    if let Ok(query) = grammar::cached_query(&language, queries::DESTRUCTURED_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(js_model::DeclarationKind::Variable),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // Pair patterns: `export const { foo: renamed } = obj` — key is the public name
    if let Ok(query) = grammar::cached_query(&language, queries::DESTRUCTURED_PAIR_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(js_model::DeclarationKind::Variable),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // --- Star re-exports (plain `export * from '...'`) ------------------------
    // The JS/TS grammar represents `export * from '...'` with a bare `*` token
    // directly inside the export_statement (no namespace_export child node).
    if let Ok(query) = grammar::cached_query(&language, queries::STAR_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // index 0 = @source
                    let node = capture.node;
                    let raw_source = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on star re-export source")?;
                    let module_spec = strip_quotes(raw_source);
                    let spec_str = module_spec.to_string();
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::StarReexport
                            && e.source.as_deref() == Some(&spec_str)
                    });
                    // TLF-07: Detect `export type * from '...'`.
                    // In tree-sitter-typescript 0.23.2, `export type *` is parsed as:
                    // export_statement { export, ERROR { type }, *, from, string }
                    // The `type` keyword is inside an ERROR child, not a direct unnamed child.
                    // We check both direct children AND ERROR children for the `type` token.
                    let is_type_only = {
                        let mut found_type = false;
                        if let Some(stmt) = node.parent() {
                            let mut sc = stmt.walk();
                            for c in stmt.children(&mut sc) {
                                if !c.is_named() && c.utf8_text(source).ok() == Some("type") {
                                    found_type = true;
                                    break;
                                }
                                if c.kind() == "ERROR" {
                                    let mut ec = c.walk();
                                    for inner in c.children(&mut ec) {
                                        if !inner.is_named()
                                            && inner.utf8_text(source).ok() == Some("type")
                                        {
                                            found_type = true;
                                            break;
                                        }
                                    }
                                    if found_type {
                                        break;
                                    }
                                }
                            }
                        }
                        found_type
                    };
                    if !already {
                        exports.push(RawExport {
                            name: "*".to_string(),
                            kind: RawExportKind::StarReexport,
                            line: node.start_position().row as u32 + 1,
                            col: node.start_position().column as u32 + 1,
                            source: Some(spec_str),
                            decl_kind: None,
                            is_type_only,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // --- Namespace re-exports (`export * as name from '...'`) -----------------
    // NAMESPACE_EXPORT_QUERY captures: index 0 = @ns_name, index 1 = @source.
    {
        if let Ok(query) = grammar::cached_query(&language, queries::NAMESPACE_EXPORT_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, source);
            while let Some(m) = matches.next() {
                let mut ns_name_opt: Option<(String, u32, u32)> = None;
                let mut src_opt: Option<String> = None;
                for capture in m.captures {
                    let node = capture.node;
                    let text = node
                        .utf8_text(source)
                        .context("UTF-8 decode failed on namespace re-export")?;
                    if capture.index == 0 {
                        // @ns_name — the public namespace identifier
                        ns_name_opt = Some((
                            text.to_string(),
                            node.start_position().row as u32 + 1,
                            node.start_position().column as u32 + 1,
                        ));
                    } else if capture.index == 1 {
                        // @source — the module specifier
                        src_opt = Some(strip_quotes(text).to_string());
                    }
                }
                if let (Some((ns_name, line, col)), Some(src)) = (ns_name_opt, src_opt) {
                    // Include the name in the dedup check so that a plain star
                    // re-export (name="*") and a namespace re-export (name="utils") from the
                    // same source are NOT considered duplicates and can coexist.
                    let already = exports.iter().any(|e| {
                        e.kind == RawExportKind::StarReexport
                            && e.source.as_deref() == Some(src.as_str())
                            && e.name == ns_name
                    });
                    if !already {
                        exports.push(RawExport {
                            name: ns_name,
                            kind: RawExportKind::StarReexport,
                            line,
                            col,
                            source: Some(src),
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // --- Named re-exports -----------------------------------------------------
    // Process aliased re-exports first (`export { foo as bar } from './mod'`),
    // then non-aliased re-exports (`export { foo } from './mod'`).
    // Track local-name positions from the alias query to skip them in the plain query.
    let mut reexport_alias_local_positions: std::collections::HashSet<(u32, u32)> =
        std::collections::HashSet::new();
    {
        let query = grammar::cached_query(&language, queries::NAMED_REEXPORT_ALIAS_QUERY)
            .context("failed to compile NAMED_REEXPORT_ALIAS_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // index 0 = @local_name, index 1 = @alias_name, index 2 = @source
            let mut local_pos: Option<(u32, u32)> = None;
            let mut alias_name_opt: Option<(String, u32, u32)> = None;
            let mut src_opt: Option<String> = None;
            for capture in m.captures {
                let node = capture.node;
                let text = node
                    .utf8_text(source)
                    .context("UTF-8 decode failed on named re-export alias")?;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    local_pos = Some((line, col));
                } else if capture.index == 1 {
                    alias_name_opt = Some((text.to_string(), line, col));
                } else if capture.index == 2 {
                    src_opt = Some(strip_quotes(text).to_string());
                }
            }
            if let (Some(lp), Some((name, line, col)), Some(src)) =
                (local_pos, alias_name_opt, src_opt)
            {
                reexport_alias_local_positions.insert(lp);
                exports.push(RawExport {
                    name,
                    kind: RawExportKind::NamedReexport,
                    line,
                    col,
                    source: Some(src),
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                    declaration_site: None,
                });
            }
        }
    }
    {
        let query = grammar::cached_query(&language, queries::NAMED_REEXPORT_QUERY)
            .context("failed to compile NAMED_REEXPORT_QUERY")?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            // Match has two captures: @name (index 0) and @source (index 1).
            let mut name_opt: Option<(String, u32, u32)> = None;
            let mut source_opt: Option<String> = None;

            for capture in m.captures {
                let node = capture.node;
                let text = node
                    .utf8_text(source)
                    .context("UTF-8 decode failed on named re-export")?;
                let line = node.start_position().row as u32 + 1;
                let col = node.start_position().column as u32 + 1;
                if capture.index == 0 {
                    name_opt = Some((text.to_string(), line, col));
                } else if capture.index == 1 {
                    source_opt = Some(strip_quotes(text).to_string());
                }
            }

            if let (Some((name, line, col)), Some(src)) = (name_opt, source_opt) {
                // Skip if already handled by the alias query above.
                if reexport_alias_local_positions.contains(&(line, col)) {
                    continue;
                }
                // Skip if already handled by the type-only re-export query
                // (TypeScript `export type { Foo } from '...'` — avoids double-emit).
                if type_only_reexport_positions.contains(&(line, col)) {
                    continue;
                }
                exports.push(RawExport {
                    name,
                    kind: RawExportKind::NamedReexport,
                    line,
                    col,
                    source: Some(src),
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                    declaration_site: None,
                });
            }
        }
    }

    // --- Array destructuring: `export const [a, b] = arr` ----------
    if let Ok(query) = grammar::cached_query(&language, queries::ARRAY_DESTRUCTURED_EXPORT_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(DeclarationKind::Variable),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // Rest patterns: `export const [a,...rest] = arr`
    if let Ok(query) =
        grammar::cached_query(&language, queries::ARRAY_DESTRUCTURED_REST_EXPORT_QUERY)
    {
        let mut cursor = QueryCursor::new();
        let mut matches_iter = cursor.matches(&query, root, source);
        while let Some(m) = matches_iter.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    if let Ok(name) = node.utf8_text(source) {
                        exports.push(RawExport {
                            name: name.to_string(),
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind: Some(DeclarationKind::Variable),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }

    // --- Nested destructuring: `export const { a: { b } } = obj` ---
    // Tree-sitter queries only match 1-level deep. Walk export statements with
    // object_pattern or array_pattern and recursively extract nested identifiers.
    // Runs AFTER flat destructuring queries so the dedup check correctly skips
    // names already captured by the flat queries (only adds the nested-level names).
    {
        let nested_query_str = r#"
        (export_statement
          declaration: [
            (lexical_declaration (variable_declarator name: [(object_pattern) (array_pattern)] @pattern))
            (variable_declaration (variable_declarator name: [(object_pattern) (array_pattern)] @pattern))
          ])
        "#;
        if let Ok(query) = grammar::cached_query(&language, nested_query_str) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        let mut names = Vec::new();
                        extract_nested_destructuring_names(capture.node, source, &mut names);
                        for (name, line, col) in names {
                            // Dedup: only add if not already captured by flat queries
                            let already = exports.iter().any(|e| e.name == name);
                            if !already {
                                exports.push(RawExport {
                                    name,
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: Some(DeclarationKind::Variable),
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                    typeof_import_refs: Vec::new(),
                                    is_proxy_dispatched: false,
                                    local_name: None,
                                    declaration_site: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // --- CJS module.exports / exports.foo detection ---------------
    // Only for JavaScript files — TypeScript uses `export =` instead.
    // Validation (checking "module"/"exports" text) is done in Rust after capture
    // since #eq? predicates are not automatically applied by cursor.matches().
    if matches!(lang, grammar::SourceLanguage::JavaScript) {
        // 1. module.exports = { foo, bar } — shorthand properties
        if let Ok(query) =
            grammar::cached_query(&language, queries::CJS_MODULE_EXPORTS_SHORTHAND_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Validate: capture 0 (@obj) must be "module", capture 1 (@prop) must be "exports"
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            // @name — shorthand property identifier
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                        is_type_only: false,
                                        release_tag: None,
                                        compound_members: Vec::new(),
                                        type_signature: None,
                                        satisfies_type: None,
                                        typeof_import_refs: Vec::new(),
                                        is_proxy_dispatched: false,
                                        local_name: None,
                                        declaration_site: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. module.exports = { key: value } — explicit pair properties
        if let Ok(query) = grammar::cached_query(&language, queries::CJS_MODULE_EXPORTS_PAIR_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                        is_type_only: false,
                                        release_tag: None,
                                        compound_members: Vec::new(),
                                        type_signature: None,
                                        satisfies_type: None,
                                        typeof_import_refs: Vec::new(),
                                        is_proxy_dispatched: false,
                                        local_name: None,
                                        declaration_site: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. exports.foo = bar — property-by-property
        if let Ok(query) = grammar::cached_query(&language, queries::CJS_EXPORTS_PROPERTY_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Validate: capture 0 (@obj) must be "exports" (not "module" or anything else)
                let obj_text = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                if obj_text == Some("exports") {
                    for capture in m.captures {
                        if capture.index == 1 {
                            // @name — the property being assigned
                            let node = capture.node;
                            if let Ok(name) = node.utf8_text(source) {
                                let line = node.start_position().row as u32 + 1;
                                let col = node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: Some(DeclarationKind::Variable),
                                        is_type_only: false,
                                        release_tag: None,
                                        compound_members: Vec::new(),
                                        type_signature: None,
                                        satisfies_type: None,
                                        typeof_import_refs: Vec::new(),
                                        is_proxy_dispatched: false,
                                        local_name: None,
                                        declaration_site: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4a. module.exports = class {... } — CJS inline class export
        // Run BEFORE query #4 (module.exports = Identifier) so the dedup check on "default"
        // skips the identifier-form query when an inline class was already emitted.
        if let Ok(query) = grammar::cached_query(&language, queries::CJS_MODULE_EXPORTS_CLASS_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    if let Some(class_capture) = m.captures.iter().find(|c| c.index == 2) {
                        let node = class_capture.node;
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        let already = exports.iter().any(|e| e.name == "default");
                        if !already {
                            exports.push(RawExport {
                                name: "default".to_string(),
                                kind: RawExportKind::Default,
                                line,
                                col,
                                source: None,
                                decl_kind: Some(DeclarationKind::Class),
                                is_type_only: false,
                                release_tag: None,
                                compound_members: Vec::new(),
                                type_signature: None,
                                satisfies_type: None,
                                typeof_import_refs: Vec::new(),
                                is_proxy_dispatched: false,
                                local_name: None,
                                declaration_site: None,
                            });
                        }
                    }
                }
            }
        }

        // 4b. module.exports = function/arrow — CJS inline function export
        if let Ok(query) =
            grammar::cached_query(&language, queries::CJS_MODULE_EXPORTS_FUNCTION_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    if let Some(fn_capture) = m.captures.iter().find(|c| c.index == 2) {
                        let node = fn_capture.node;
                        let line = node.start_position().row as u32 + 1;
                        let col = node.start_position().column as u32 + 1;
                        let already = exports.iter().any(|e| e.name == "default");
                        if !already {
                            exports.push(RawExport {
                                name: "default".to_string(),
                                kind: RawExportKind::Default,
                                line,
                                col,
                                source: None,
                                decl_kind: Some(DeclarationKind::Function),
                                is_type_only: false,
                                release_tag: None,
                                compound_members: Vec::new(),
                                type_signature: None,
                                satisfies_type: None,
                                typeof_import_refs: Vec::new(),
                                is_proxy_dispatched: false,
                                local_name: None,
                                declaration_site: None,
                            });
                        }
                    }
                }
            }
        }

        // 4c. exports.NAME = function/arrow — CJS named function property export
        // Only when @obj is "exports" (NOT "module"). The existing CJS_EXPORTS_PROPERTY_QUERY
        // already emits these as Variable; we emit a more specific Function decl_kind version.
        // Use a dedup check on name to avoid double-emitting with the existing query (#3).
        if let Ok(query) =
            grammar::cached_query(&language, queries::CJS_EXPORTS_PROPERTY_FUNCTION_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // @obj must be exactly "exports" (not "module")
                let obj_text = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                if obj_text != Some("exports") {
                    continue;
                }
                // Extract @name (index 1) from the member expression property
                let name_capture = m.captures.iter().find(|c| c.index == 1);
                if let Some(nc) = name_capture {
                    let name_node = nc.node;
                    if let Ok(export_name) = name_node.utf8_text(source) {
                        let line = name_node.start_position().row as u32 + 1;
                        let col = name_node.start_position().column as u32 + 1;
                        // Update existing entry's decl_kind if already emitted as Variable by query #3
                        if let Some(existing) = exports.iter_mut().find(|e| {
                            e.name == export_name && e.decl_kind == Some(DeclarationKind::Variable)
                        }) {
                            existing.decl_kind = Some(DeclarationKind::Function);
                        } else {
                            let already = exports.iter().any(|e| e.name == export_name);
                            if !already {
                                exports.push(RawExport {
                                    name: export_name.to_string(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: Some(DeclarationKind::Function),
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                    typeof_import_refs: Vec::new(),
                                    is_proxy_dispatched: false,
                                    local_name: None,
                                    declaration_site: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 4. module.exports = SomeIdentifier — default-like export
        if let Ok(query) =
            grammar::cached_query(&language, queries::CJS_MODULE_EXPORTS_DEFAULT_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if obj_ok && prop_ok {
                    for capture in m.captures {
                        if capture.index == 2 {
                            // @value — the identifier being assigned as default
                            let node = capture.node;
                            let line = node.start_position().row as u32 + 1;
                            let col = node.start_position().column as u32 + 1;
                            let already = exports.iter().any(|e| e.name == "default");
                            if !already {
                                exports.push(RawExport {
                                    name: "default".to_string(),
                                    kind: RawExportKind::Default,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: None,
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                    typeof_import_refs: Vec::new(),
                                    is_proxy_dispatched: false,
                                    local_name: None,
                                    declaration_site: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // DOC-01: module.exports = new Proxy(...) | Reflect.construct(...)
        if let Ok(query) = grammar::cached_query(&language, queries::CJS_MODULE_EXPORTS_PROXY_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let obj_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 0 && c.node.utf8_text(source).ok() == Some("module"));
                let prop_ok = m
                    .captures
                    .iter()
                    .any(|c| c.index == 1 && c.node.utf8_text(source).ok() == Some("exports"));
                if !obj_ok || !prop_ok {
                    continue;
                }

                // Branch A: new_expression with constructor 'Proxy'
                let is_proxy_new = m
                    .captures
                    .iter()
                    .any(|c| c.index == 2 && c.node.utf8_text(source).ok() == Some("Proxy"));

                // Branch B: call_expression with Reflect.{construct,get}
                let is_reflect_call = {
                    let reflect_obj_ok = m
                        .captures
                        .iter()
                        .any(|c| c.index == 3 && c.node.utf8_text(source).ok() == Some("Reflect"));
                    let reflect_method_ok = m.captures.iter().any(|c| {
                        c.index == 4
                            && matches!(
                                c.node.utf8_text(source).ok(),
                                Some("construct") | Some("get")
                            )
                    });
                    reflect_obj_ok && reflect_method_ok
                };

                if !(is_proxy_new || is_reflect_call) {
                    continue;
                }

                // Find the position of the right-hand expression
                let pos_capture = m.captures.iter().find(|c| c.index == 2 || c.index == 3);
                if let Some(c) = pos_capture {
                    let node = c.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    let already = exports.iter().any(|e| e.name == "default");
                    if !already {
                        exports.push(RawExport {
                            name: "default".to_string(),
                            kind: RawExportKind::Default,
                            line,
                            col,
                            source: None,
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: true,
                            local_name: None,
                            declaration_site: None,
                        });
                    } else {
                        // Existing default already captured by a sibling pattern (rare).
                        // Mark it as proxy-dispatched so the comparator still emits the limitation.
                        for e in exports.iter_mut() {
                            if e.name == "default" {
                                e.is_proxy_dispatched = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // --- CJS aliasing: `exports = module.exports = {... }` --------
    // Some packages use a chained assignment pattern to alias exports and module.exports.
    //.
    if matches!(lang, grammar::SourceLanguage::JavaScript) {
        if let Ok(query) = grammar::cached_query(&language, queries::CJS_EXPORTS_ALIASING_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Verify: @lhs is "exports", @mod_obj is "module", @mod_prop is "exports"
                let lhs = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let mod_obj = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let mod_prop = m
                    .captures
                    .iter()
                    .find(|c| c.index == 2)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let obj_node = m.captures.iter().find(|c| c.index == 3).map(|c| c.node);

                if lhs == Some("exports")
                    && mod_obj == Some("module")
                    && mod_prop == Some("exports")
                {
                    if let Some(obj) = obj_node {
                        // Extract properties from the object literal
                        let mut obj_cursor = obj.walk();
                        for child in obj.children(&mut obj_cursor) {
                            match child.kind() {
                                "shorthand_property_identifier" => {
                                    if let Ok(name) = child.utf8_text(source) {
                                        let line = child.start_position().row as u32 + 1;
                                        let col = child.start_position().column as u32 + 1;
                                        let already = exports.iter().any(|e| e.name == name);
                                        if !already {
                                            exports.push(RawExport {
                                                name: name.to_string(),
                                                kind: RawExportKind::Named,
                                                line,
                                                col,
                                                source: None,
                                                decl_kind: None,
                                                is_type_only: false,
                                                release_tag: None,
                                                compound_members: Vec::new(),
                                                type_signature: None,
                                                satisfies_type: None,
                                                typeof_import_refs: Vec::new(),
                                                is_proxy_dispatched: false,
                                                local_name: None,
                                                declaration_site: None,
                                            });
                                        }
                                    }
                                }
                                "pair" => {
                                    if let Some(key_node) = child.child_by_field_name("key") {
                                        if let Ok(name) = key_node.utf8_text(source) {
                                            let line = key_node.start_position().row as u32 + 1;
                                            let col = key_node.start_position().column as u32 + 1;
                                            let already = exports.iter().any(|e| e.name == name);
                                            if !already {
                                                exports.push(RawExport {
                                                    name: name.to_string(),
                                                    kind: RawExportKind::Named,
                                                    line,
                                                    col,
                                                    source: None,
                                                    decl_kind: None,
                                                    is_type_only: false,
                                                    release_tag: None,
                                                    compound_members: Vec::new(),
                                                    type_signature: None,
                                                    satisfies_type: None,
                                                    typeof_import_refs: Vec::new(),
                                                    is_proxy_dispatched: false,
                                                    local_name: None,
                                                    declaration_site: None,
                                                });
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    // --- Module augmentation capture (TypeScript only) ---
    // Captures `declare module 'express' {... }` and `declare global {... }`.
    // Module augmentations extend external package types — their removal breaks consumers.
    // Emitted with synthetic `augment:<module>` key (or `augment:global`) so the existing
    // export-removed detection handles their removal without extra code.
    // INF-01: TypeScriptTsx included — module augmentation is TS
    // syntax that TSX also parses.
    if matches!(
        lang,
        grammar::SourceLanguage::TypeScript | grammar::SourceLanguage::TypeScriptTsx
    ) {
        // declare module 'express' {... }
        if let Ok(query) =
            grammar::cached_query(&language, queries::DECLARE_MODULE_AUGMENTATION_QUERY_TS)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Extract both captures: index 0 = @module_name, index 1 = @module_body
                let mut module_name_opt: Option<String> = None;
                let mut body_node_opt: Option<tree_sitter::Node> = None;
                let mut name_line = 1u32;
                let mut name_col = 1u32;
                for capture in m.captures {
                    if capture.index == 0 {
                        // @module_name — the string literal (e.g., "'express'")
                        if let Ok(text) = capture.node.utf8_text(source) {
                            module_name_opt = Some(strip_quotes(text).to_string());
                            name_line = capture.node.start_position().row as u32 + 1;
                            name_col = capture.node.start_position().column as u32 + 1;
                        }
                    } else if capture.index == 1 {
                        // @module_body — the statement_block
                        body_node_opt = Some(capture.node);
                    }
                }
                if let Some(module_name) = module_name_opt {
                    let augment_key = format!("augment:{}", module_name);
                    let already = exports.iter().any(|e| e.name == augment_key);
                    if !already {
                        exports.push(RawExport {
                            name: augment_key,
                            kind: RawExportKind::Named,
                            line: name_line,
                            col: name_col,
                            source: None,
                            decl_kind: Some(DeclarationKind::Namespace),
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                    // META-03: emit synthetic keys for each top-level member in the body.
                    // Naming convention: `augment:<module>:*<member>`.
                    if let Some(body) = body_node_opt {
                        emit_augmentation_members(body, &module_name, source, &mut exports);
                    }
                }
            }
        }

        // declare global {... }
        // Grammar: `declare global {... }` parses as `(ambient_declaration (statement_block...))`.
        // The `statement_block` is the first (and only) named child of `ambient_declaration`.
        // We match ambient_declaration nodes whose first named child is statement_block and
        // have no preceding module-name node — that distinguishes `declare global` from
        // `declare module 'foo' { }` (which has a module child with a string name).
        if let Ok(query) =
            grammar::cached_query(&language, queries::DECLARE_GLOBAL_AUGMENTATION_QUERY_TS)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                for capture in m.captures {
                    if capture.index == 0 {
                        // @global_body — the statement_block inside ambient_declaration.
                        // Verify the parent is ambient_declaration with no module-name child
                        // to exclude false positives from `declare module 'foo' { }` body blocks
                        // (those are nested inside a `module` node, not directly in ambient_declaration).
                        let body_node = capture.node;
                        // Check parent is ambient_declaration
                        if let Some(parent) = body_node.parent() {
                            if parent.kind() == "ambient_declaration" {
                                // Ensure no `module` child — that would be `declare module 'foo'`
                                let has_module_child = (0..parent.child_count() as u32)
                                    .filter_map(|i| parent.child(i))
                                    .any(|c| c.kind() == "module");
                                if !has_module_child {
                                    let line = body_node.start_position().row as u32 + 1;
                                    let col = body_node.start_position().column as u32 + 1;
                                    let already =
                                        exports.iter().any(|e| e.name == "augment:global");
                                    if !already {
                                        exports.push(RawExport {
                                            name: "augment:global".to_string(),
                                            kind: RawExportKind::Named,
                                            line,
                                            col,
                                            source: None,
                                            decl_kind: Some(DeclarationKind::Namespace),
                                            is_type_only: false,
                                            release_tag: None,
                                            compound_members: Vec::new(),
                                            type_signature: None,
                                            satisfies_type: None,
                                            typeof_import_refs: Vec::new(),
                                            is_proxy_dispatched: false,
                                            local_name: None,
                                            declaration_site: None,
                                        });
                                    }
                                    // META-03: emit synthetic keys for top-level members.
                                    emit_augmentation_members(
                                        body_node,
                                        "global",
                                        source,
                                        &mut exports,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // --- Namespace member expansion  ---------------------
    // Recursively expands namespace members at all nesting levels.
    // For `export namespace A { export namespace B { export function baz() {} } }`,
    // emits "A.B" and "A.B.baz" as separate Named exports.
    // supports 2+ levels of nesting.
    {
        let ns_query_str = "(export_statement (internal_module name: (identifier) @ns_name body: (statement_block) @ns_body))";
        if let Ok(query) = grammar::cached_query(&language, ns_query_str) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let mut ns_name_opt: Option<String> = None;
                let mut ns_body_opt: Option<tree_sitter::Node> = None;
                for capture in m.captures {
                    if capture.index == 0 {
                        // @ns_name
                        if let Ok(name) = capture.node.utf8_text(source) {
                            ns_name_opt = Some(name.to_string());
                        }
                    } else if capture.index == 1 {
                        // @ns_body
                        ns_body_opt = Some(capture.node);
                    }
                }
                if let (Some(ns_name), Some(ns_body)) = (ns_name_opt, ns_body_opt) {
                    expand_namespace_members(ns_body, &ns_name, source, &mut exports, &language);
                }
            }
        }
    }

    // --- Bundler CJS output: BUN-01..07 (JavaScript only) ----------------------
    // Detects esbuild tree-shake wrappers, __export helper calls, __defProp aliases,
    // tsc void 0 placeholders, swc star re-export shims, Object.defineProperty/
    // defineProperties/assign forms, and bracket-subscript assignments.
    //
    // BUN-11-B Path 1A scratch: lodash-style binding identifiers
    // pushed by the BUN-11-B driver (e.g. "_" for lodash). The post-process
    // after `attach_compound_members` (~line 3730) consumes this list to
    // rename the synthetic Default's `name` from the binding to "default".
    // Declared at parse_exports scope so the post-process — outside the
    // JS-only gate — can see it.
    let mut umd_default_bindings: Vec<String> = Vec::new();
    if matches!(lang, grammar::SourceLanguage::JavaScript) {
        // BUN-01: esbuild `0 && (module.exports = {... })` tree-shake wrapper
        if let Ok(query) =
            grammar::cached_query(&language, queries::ESBUILD_TREE_SHAKE_WRAPPER_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let mod_obj = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let mod_prop = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok());
                let exports_obj = m.captures.iter().find(|c| c.index == 2).map(|c| c.node);

                if mod_obj == Some("module") && mod_prop == Some("exports") {
                    if let Some(obj) = exports_obj {
                        let mut obj_cursor = obj.walk();
                        for child in obj.children(&mut obj_cursor) {
                            if child.kind() == "pair" {
                                if let Some(key_node) = child.child_by_field_name("key") {
                                    if let Ok(name) = key_node.utf8_text(source) {
                                        if name == "__esModule" {
                                            continue;
                                        }
                                        let line = key_node.start_position().row as u32 + 1;
                                        let col = key_node.start_position().column as u32 + 1;
                                        let already = exports.iter().any(|e| e.name == name);
                                        if !already {
                                            exports.push(RawExport {
                                                name: name.to_string(),
                                                kind: RawExportKind::Named,
                                                line,
                                                col,
                                                source: None,
                                                decl_kind: None,
                                                is_type_only: false,
                                                release_tag: None,
                                                compound_members: Vec::new(),
                                                type_signature: None,
                                                satisfies_type: None,
                                                typeof_import_refs: Vec::new(),
                                                is_proxy_dispatched: false,
                                                local_name: None,
                                                declaration_site: None,
                                            });
                                        }
                                    }
                                }
                            } else if child.kind() == "shorthand_property_identifier" {
                                if let Ok(name) = child.utf8_text(source) {
                                    if name == "__esModule" {
                                        continue;
                                    }
                                    let line = child.start_position().row as u32 + 1;
                                    let col = child.start_position().column as u32 + 1;
                                    let already = exports.iter().any(|e| e.name == name);
                                    if !already {
                                        exports.push(RawExport {
                                            name: name.to_string(),
                                            kind: RawExportKind::Named,
                                            line,
                                            col,
                                            source: None,
                                            decl_kind: None,
                                            is_type_only: false,
                                            release_tag: None,
                                            compound_members: Vec::new(),
                                            type_signature: None,
                                            satisfies_type: None,
                                            typeof_import_refs: Vec::new(),
                                            is_proxy_dispatched: false,
                                            local_name: None,
                                            declaration_site: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // BUN-02: esbuild `__export(target, { foo: () => foo })` helper
        if let Ok(query) = grammar::cached_query(&language, queries::ESBUILD_EXPORT_HELPER_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let fn_name = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let target = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let prop_obj = m.captures.iter().find(|c| c.index == 2).map(|c| c.node);

                // Heuristic: fn_name is "__export*" or short (minified); target must be exports/module
                let is_export_helper = fn_name.starts_with("__export") || fn_name.len() <= 3;
                let is_valid_target = target == "exports" || target == "module";

                if is_export_helper && is_valid_target {
                    if let Some(obj) = prop_obj {
                        let mut obj_cursor = obj.walk();
                        for child in obj.children(&mut obj_cursor) {
                            if child.kind() == "pair" {
                                // Only emit if value is an arrow_function (getter)
                                if let Some(val_node) = child.child_by_field_name("value") {
                                    if val_node.kind() != "arrow_function" {
                                        continue;
                                    }
                                }
                                if let Some(key_node) = child.child_by_field_name("key") {
                                    if let Ok(name) = key_node.utf8_text(source) {
                                        if name == "__esModule" {
                                            continue;
                                        }
                                        let line = key_node.start_position().row as u32 + 1;
                                        let col = key_node.start_position().column as u32 + 1;
                                        let already = exports.iter().any(|e| e.name == name);
                                        if !already {
                                            exports.push(RawExport {
                                                name: name.to_string(),
                                                kind: RawExportKind::Named,
                                                line,
                                                col,
                                                source: None,
                                                decl_kind: None,
                                                is_type_only: false,
                                                release_tag: None,
                                                compound_members: Vec::new(),
                                                type_signature: None,
                                                satisfies_type: None,
                                                typeof_import_refs: Vec::new(),
                                                is_proxy_dispatched: false,
                                                local_name: None,
                                                declaration_site: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // BUN-04: tsc `exports.foo = void 0;` placeholder
        if let Ok(query) = grammar::cached_query(&language, queries::TSC_VOID_EXPORTS_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let exports_id = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let name_node = m.captures.iter().find(|c| c.index == 1).map(|c| c.node);
                let rhs_node = m.captures.iter().find(|c| c.index == 2).map(|c| c.node);

                if exports_id != "exports" {
                    continue;
                }

                // Only capture void 0 placeholders here (real assignments captured by
                // the existing CJS_EXPORTS_PROPERTY_QUERY)
                let is_void_rhs = rhs_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|t| t.trim() == "void 0")
                    .unwrap_or(false);

                if is_void_rhs {
                    if let Some(name_n) = name_node {
                        if let Ok(name) = name_n.utf8_text(source) {
                            if name == "__esModule" {
                                continue;
                            }
                            let line = name_n.start_position().row as u32 + 1;
                            let col = name_n.start_position().column as u32 + 1;
                            let already = exports.iter().any(|e| e.name == name);
                            if !already {
                                exports.push(RawExport {
                                    name: name.to_string(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: None,
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                    typeof_import_refs: Vec::new(),
                                    is_proxy_dispatched: false,
                                    local_name: None,
                                    declaration_site: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // BUN-05: swc `Object.keys(m).forEach(k => exports[k] = m[k])` star re-export shim.
        // Build a quick require-binding map first: var m = require('...')
        let mut require_bindings: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        {
            let require_query_str = r#"
            (variable_declarator
              name: (identifier) @binding
              value: (call_expression
                function: (identifier) @req_fn
                arguments: (arguments (string) @req_path)))
            "#;
            if let Ok(rq) = grammar::cached_query(&language, require_query_str) {
                let mut cur = QueryCursor::new();
                let mut mit = cur.matches(&rq, root, source);
                while let Some(rm) = mit.next() {
                    let binding = rm
                        .captures
                        .iter()
                        .find(|c| c.index == 0)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or("")
                        .to_string();
                    let req_fn = rm
                        .captures
                        .iter()
                        .find(|c| c.index == 1)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or("");
                    let req_path = rm
                        .captures
                        .iter()
                        .find(|c| c.index == 2)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .map(strip_quotes)
                        .unwrap_or_default();
                    if req_fn == "require" && !binding.is_empty() && !req_path.is_empty() {
                        require_bindings.insert(binding, req_path.to_string());
                    }
                }
            }
        }
        if let Ok(query) = grammar::cached_query(&language, queries::SWC_STAR_REEXPORT_SHIM_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let object_id = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let keys_method = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let source_module_ident = m
                    .captures
                    .iter()
                    .find(|c| c.index == 2)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let foreach_method = m
                    .captures
                    .iter()
                    .find(|c| c.index == 3)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");

                if object_id != "Object" || keys_method != "keys" || foreach_method != "forEach" {
                    continue;
                }

                // Resolve the identifier to a require() target
                if let Some(require_target) = require_bindings.get(source_module_ident) {
                    let first_cap = m.captures.first().map(|c| c.node);
                    let (line, col) = first_cap
                        .map(|n| {
                            (
                                n.start_position().row as u32 + 1,
                                n.start_position().column as u32 + 1,
                            )
                        })
                        .unwrap_or((1, 1));
                    let star_key = format!("*{}", require_target);
                    let already = exports.iter().any(|e| e.name == star_key);
                    if !already {
                        exports.push(RawExport {
                            name: star_key,
                            kind: RawExportKind::StarReexport,
                            line,
                            col,
                            source: Some(require_target.clone()),
                            decl_kind: None,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }

        for property in super::object_exports::collect(root, &language, source)? {
            let name = property.key.export_name();
            let kind = match property.key {
                property_key::PropertyKey::String(_) => RawExportKind::Named,
                _ => RawExportKind::Symbol,
            };
            if !exports
                .iter()
                .any(|export| export.name == name && export.kind == kind)
            {
                exports.push(RawExport {
                    name,
                    kind,
                    line: property.node.start_position().row as u32 + 1,
                    col: property.node.start_position().column as u32 + 1,
                    source: None,
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                    declaration_site: None,
                });
            }
        }

        // BUN-06b: `exports["foo"] = bar;` bracket-subscript assignment
        if let Ok(query) = grammar::cached_query(&language, queries::BRACKET_SUBSCRIPT_EXPORT_QUERY)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let exports_id = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let name_str_node = m.captures.iter().find(|c| c.index == 1).map(|c| c.node);

                if exports_id != "exports" {
                    continue;
                }

                if let Some(name_n) = name_str_node {
                    if let Ok(raw) = name_n.utf8_text(source) {
                        let name = strip_quotes(raw);
                        if name == "__esModule" || name.is_empty() {
                            continue;
                        }
                        let line = name_n.start_position().row as u32 + 1;
                        let col = name_n.start_position().column as u32 + 1;
                        let already = exports.iter().any(|e| e.name == name);
                        if !already {
                            exports.push(RawExport {
                                name: name.to_string(),
                                kind: RawExportKind::Named,
                                line,
                                col,
                                source: None,
                                decl_kind: None,
                                is_type_only: false,
                                release_tag: None,
                                compound_members: Vec::new(),
                                type_signature: None,
                                satisfies_type: None,
                                typeof_import_refs: Vec::new(),
                                is_proxy_dispatched: false,
                                local_name: None,
                                declaration_site: None,
                            });
                        }
                    }
                }
            }
        }

        // BUN-11-A: UMD wrapper factory-returns-object literal unwrap.
        //
        // ORDERING INVARIANT: this driver MUST run AFTER
        // `CJS_EXPORTS_PROPERTY_QUERY` (and `OBJECT_DEFINE_EXPORT_QUERY`) so
        // that the dedup check `exports.iter().any(|e| e.name == name)` below
        // sees those earlier emissions FIRST. If a future refactor moves this
        // driver earlier in the JS-only block — or introduces a new export
        // shape that surfaces names AFTER BUN-11-A — the dedup precedence
        // flips: BUN-11-A becomes the first writer with empty
        // `compound_members` and no `decl_kind`, and subsequent
        // `exports.X = …` emissions get suppressed. Do NOT reorder without
        // adding a regression test for the hybrid "factory both `return { foo }`
        // AND `exports.foo = …`" shape.
        //
        // Detects the canonical UMD shape:
        //  (function (root, factory) {... })(this, function () { return { foo, bar }; }); // Form 1
        //  (function (root, factory) {... }(this, function () { return { foo, bar }; })); // Form 2
        // and emits each pair in the factory's `return {... }` as a Named export.
        //
        // Sub-cases covered ELSEWHERE (no code needed here):
        //  - Factory uses `exports.X = …` / `Object.defineProperty(exports, …)`:
        // already detected by the depth-agnostic CJS_EXPORTS_PROPERTY_QUERY and
        // OBJECT_DEFINE_EXPORT_QUERY at the outer pass. The dedup check below
        // prevents double emission (see ORDERING INVARIANT above).
        //  - Lodash-style `(module.exports = <ident>)._ = <ident>`: handled by the
        // BUN-11-B driver after this driver.
        //
        // Each query uses a fresh cursor and shares the factory-body walker.
        //
        // The companion query handles wrappers where parentheses enclose the
        // whole call rather than only the function expression.
        let process_factory_body = |body: tree_sitter::Node, exports: &mut Vec<RawExport>| {
            let mut body_cursor = body.walk();
            for stmt in body.children(&mut body_cursor) {
                if stmt.kind() != "return_statement" {
                    continue;
                }
                // The return_statement's argument is the returned value.
                // tree-sitter-javascript exposes it as a child or via field name
                // "argument" — try both for grammar-version resilience.
                let arg = stmt
                    .child_by_field_name("argument")
                    .or_else(|| stmt.named_child(0));
                let Some(arg_node) = arg else { continue };
                if arg_node.kind() != "object" {
                    // Returns a non-object (e.g. an identifier — that's the
                    // lodash-style adjacent case, handled by the BUN-11-B
                    // driver below, NOT here).
                    continue;
                }

                // Walk the object literal's pairs / shorthands — same shape as
                // the BUN-06a `assign` arm walker.
                let mut obj_cur = arg_node.walk();
                for child in arg_node.children(&mut obj_cur) {
                    if child.kind() == "pair" {
                        if let Some(key_node) = child.child_by_field_name("key") {
                            if let Ok(name) = key_node.utf8_text(source) {
                                if name == "__esModule" {
                                    continue;
                                }
                                let line = key_node.start_position().row as u32 + 1;
                                let col = key_node.start_position().column as u32 + 1;
                                let already = exports.iter().any(|e| e.name == name);
                                if !already {
                                    exports.push(RawExport {
                                        name: name.to_string(),
                                        kind: RawExportKind::Named,
                                        line,
                                        col,
                                        source: None,
                                        decl_kind: None,
                                        is_type_only: false,
                                        release_tag: None,
                                        compound_members: Vec::new(),
                                        type_signature: None,
                                        satisfies_type: None,
                                        typeof_import_refs: Vec::new(),
                                        is_proxy_dispatched: false,
                                        local_name: None,
                                        declaration_site: None,
                                    });
                                }
                            }
                        }
                    } else if child.kind() == "shorthand_property_identifier" {
                        if let Ok(name) = child.utf8_text(source) {
                            if name == "__esModule" {
                                continue;
                            }
                            let line = child.start_position().row as u32 + 1;
                            let col = child.start_position().column as u32 + 1;
                            let already = exports.iter().any(|e| e.name == name);
                            if !already {
                                exports.push(RawExport {
                                    name: name.to_string(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: None,
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                    typeof_import_refs: Vec::new(),
                                    is_proxy_dispatched: false,
                                    local_name: None,
                                    declaration_site: None,
                                });
                            }
                        }
                    }
                }
            }
        };

        // Form 1: `(func)(args)` — capture index 4 = @umd_factory_body.
        // Capture indices are stable by declaration order in the query string:
        //  0 = @umd_root_param, 1 = @umd_factory_param, 2 = @umd_global_arg,
        //  3 = @umd_factory_params, 4 = @umd_factory_body.
        if let Ok(query) = grammar::cached_query(&language, queries::UMD_FACTORY_WRAPPER_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let factory_body = m.captures.iter().find(|c| c.index == 4).map(|c| c.node);
                let Some(body) = factory_body else { continue };
                process_factory_body(body, &mut exports);
            }
        }

        // Form 2: `(func(args))` — capture index 4 = @umd_factory_body.
        if let Ok(query) =
            grammar::cached_query(&language, queries::UMD_FACTORY_WRAPPER_QUERY_FORM2)
        {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                let factory_body = m.captures.iter().find(|c| c.index == 4).map(|c| c.node);
                let Some(body) = factory_body else { continue };
                process_factory_body(body, &mut exports);
            }
        }

        // BUN-11-B Path 1A: lodash-style IIFE with `module.exports = <ident>`.
        //
        // Detects the assignment depth-agnostically so the
        // parenthesized_expression chain `(module.exports = X)._ = X` from the lodash
        // canonical shape is captured the same way as a clean
        // `module.exports = X;` at top level.
        //
        // Path 1A synthesis: pushes a transient Default RawExport tagged with the
        // captured binding NAME as its `name` field. This makes attach_compound_members
        // accept the binding in its exported_names
        // gate, collecting every `<ident>.<member> = …` assignment as a
        // compound_member. The post-pass after attach_compound_members renames the
        // synthetic export's `name` from the binding to "default".
        if let Ok(query) = grammar::cached_query(&language, queries::LODASH_STYLE_DEFAULT_QUERY) {
            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root, source);
            while let Some(m) = matches_iter.next() {
                // Capture indices: 0=mod_obj, 1=mod_prop, 2=umd_default_binding.
                let mod_obj = m
                    .captures
                    .iter()
                    .find(|c| c.index == 0)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                let mod_prop = m
                    .captures
                    .iter()
                    .find(|c| c.index == 1)
                    .and_then(|c| c.node.utf8_text(source).ok())
                    .unwrap_or("");
                // Gate: must be `module.exports =...`.
                if mod_obj != "module" || mod_prop != "exports" {
                    continue;
                }
                let binding_node = m.captures.iter().find(|c| c.index == 2).map(|c| c.node);
                let Some(node) = binding_node else { continue };
                let Ok(binding) = node.utf8_text(source) else {
                    continue;
                };
                // Skip `module.exports = exports` and similar (already handled by
                // the existing CJS exports-aliasing logic).
                if binding == "exports" || binding.starts_with("module.") {
                    continue;
                }
                // Skip if a Default with name == "default" already exists from a
                // direct top-level `module.exports = X;` capture; that's handled by
                // CJS_MODULE_EXPORTS_DEFAULT_QUERY at line 565. We only want to fire
                // for the lodash-style IIFE-wrapped case where the program-level
                // CJS query did NOT match.
                let already_default = exports
                    .iter()
                    .any(|e| e.name == "default" && matches!(e.kind, RawExportKind::Default));
                if already_default {
                    continue;
                }

                // Synthesize a transient Default tagged with the binding name.
                // attach_compound_members's gate looks up `target` in
                // `exported_names: HashSet<String>` built from `exports.iter().map(|r| r.name)`.
                // Setting `name = binding` causes the gate to accept `<binding>.<member>`
                // attachments. We post-process this entry after attach_compound_members
                // returns to rename to "default".
                let already_synth = exports.iter().any(|e| e.name == binding);
                if !already_synth {
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    exports.push(RawExport {
                        name: binding.to_string(),
                        kind: RawExportKind::Default,
                        line,
                        col,
                        source: None,
                        decl_kind: None,
                        is_type_only: false,
                        release_tag: None,
                        compound_members: Vec::new(),
                        type_signature: None,
                        satisfies_type: None,
                        typeof_import_refs: Vec::new(),
                        is_proxy_dispatched: false,
                        local_name: None,
                        declaration_site: None,
                    });
                    umd_default_bindings.push(binding.to_string());
                }
            }
        }

        // BUN-11-B Path 1A: depth-agnostic compound-member scan for
        // synthesized lodash-style bindings.
        //
        // The pre-existing attach_compound_members (called below at the post-pass)
        // uses COMPOUND_ASSIGN_MEMBER_QUERY which is anchored at program level —
        // it intentionally rejects nested-inside-function assignments. The
        // lodash-style binding's `_.chunk = …` assignments live INSIDE the
        // wrapper IIFE body, so the program-anchored query cannot see them.
        //
        // Run BUN_11_B_DEPTH_ANY_MEMBER_ASSIGN_QUERY (depth-agnostic) and
        // attach members directly onto the synthesized transient Default.
        // Over-capture is bounded: the runtime gate accepts a member only when
        // its `target` matches a binding identifier in `umd_default_bindings`,
        // which contains only identifiers we just synthesized as Default.
        if !umd_default_bindings.is_empty() {
            if let Ok(query) =
                grammar::cached_query(&language, queries::BUN_11_B_DEPTH_ANY_MEMBER_ASSIGN_QUERY)
            {
                let mut cursor = QueryCursor::new();
                let mut matches_iter = cursor.matches(&query, root, source);
                while let Some(m) = matches_iter.next() {
                    let target = m
                        .captures
                        .iter()
                        .find(|c| c.index == 0)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or("");
                    let member = m
                        .captures
                        .iter()
                        .find(|c| c.index == 1)
                        .and_then(|c| c.node.utf8_text(source).ok())
                        .unwrap_or("");
                    if target.is_empty() || member.is_empty() {
                        continue;
                    }
                    // Gate: target must be a binding we synthesized.
                    if !umd_default_bindings.iter().any(|b| b == target) {
                        continue;
                    }
                    // Skip __esModule assignments (defensive — these would be
                    // filtered by the post-pass anyway).
                    if member == "__esModule" {
                        continue;
                    }
                    // Attach to the synthesized Default whose `name == target`.
                    for rx in exports.iter_mut() {
                        if rx.name == target && matches!(rx.kind, RawExportKind::Default) {
                            if !rx.compound_members.iter().any(|mm| mm == member) {
                                rx.compound_members.push(member.to_string());
                            }
                            break;
                        }
                    }
                }
            }
        }
    } // end matches!(lang, SourceLanguage::JavaScript) BUN-01..07

    // BUN-07: Belt-and-suspenders __esModule final sweep.
    // Individual query processors already skip __esModule, but existing queries
    // (CJS_EXPORTS_PROPERTY_QUERY, CJS_EXPORTS_SHORTHAND_QUERY) may have captured it
    // from unrelated patterns. Remove unconditionally regardless of source.
    exports.retain(|e| e.name != "__esModule");

    // ARCH-02: compound-component capture as a second pass.
    // Per conservative: members attach ONLY when the target identifier is
    // itself an exported symbol discovered by the first pass.
    attach_compound_members(root, source, &language, &mut exports);

    // BUN-11-B Path 1A post-process: rename synthetic default exports.
    //
    // The BUN-11-B driver pushed transient Default RawExports whose `name` field
    // equals the local binding identifier (e.g. "_" for lodash). attach_compound_members
    // just used those names to gate compound-member attachment. Now rename to
    // "default" so the export surface reflects the user-facing semantics.
    //
    // Edge case: if multiple synthetic defaults somehow ended up tagged with
    // different binding names (shouldn't happen — the driver short-circuits on
    // `already_default`), merge their compound_members onto a single Default and
    // drop the duplicates.
    if !umd_default_bindings.is_empty() {
        let mut merged_members: Vec<String> = Vec::new();
        let mut merged_line: u32 = 0;
        let mut merged_col: u32 = 0;
        let mut found = false;
        exports.retain(|rx| {
            let is_synth = umd_default_bindings.iter().any(|b| b == &rx.name)
                && matches!(rx.kind, RawExportKind::Default);
            if is_synth {
                for m in &rx.compound_members {
                    if !merged_members.iter().any(|mm| mm == m) {
                        merged_members.push(m.clone());
                    }
                }
                if !found {
                    merged_line = rx.line;
                    merged_col = rx.col;
                    found = true;
                }
                false // drop the synthetic entry; replaced below
            } else {
                true
            }
        });
        if found {
            // Re-emit ONE Default named "default" with the merged compound_members.
            // Skip if a "default" entry already exists from elsewhere (defensive
            // the driver's `already_default` short-circuit makes this rare).
            let already_default = exports
                .iter()
                .any(|e| e.name == "default" && matches!(e.kind, RawExportKind::Default));
            if !already_default {
                exports.push(RawExport {
                    name: "default".to_string(),
                    kind: RawExportKind::Default,
                    line: merged_line,
                    col: merged_col,
                    source: None,
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: merged_members,
                    type_signature: None,
                    satisfies_type: None,
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                    declaration_site: None,
                });
            } else {
                // Merge the gathered members into the existing default.
                //
                //: if the existing `default` was emitted
                // by a path that did not record real source coordinates (e.g. a
                // synthetic placeholder with `(line, col) == (0, 0)`), upgrade
                // its location from the synthesized entry's coords. Real coords
                // win over the (0, 0) sentinel; otherwise the existing entry's
                // location is preserved (first-real-writer-wins).
                for rx in exports.iter_mut() {
                    if rx.name == "default" && matches!(rx.kind, RawExportKind::Default) {
                        if rx.line == 0 && rx.col == 0 {
                            rx.line = merged_line;
                            rx.col = merged_col;
                        }
                        for m in &merged_members {
                            if !rx.compound_members.iter().any(|mm| mm == m) {
                                rx.compound_members.push(m.clone());
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    // INF-02 link-pass: for each Named (non-re-export) RawExport,
    // look up the matching declare-site in the file-scope ambient sidecar
    // built in Pass 1. When found, enrich decl_kind and type_signature when
    // the export's own values are None. This restores type identity for
    // rollup-plugin-dts / api-extractor bundle output where `export { foo }`
    // appears without inline declaration context.
    //
    // Guards:
    // - Only `RawExportKind::Named` entries — Default/StarReexport/NamedReexport
    // are excluded because their identity does not come from a local declare.
    // - Skip when `rx.source.is_some()` — re-exports (`export { foo } from...`)
    // carry an external origin; the sidecar's local declare must NOT leak in.
    // - Skip when `rx.compound_members` is populated on a hybrid target (N/A —
    // compound_members is orthogonal to the declare-linking pathway).
    for rx in exports.iter_mut() {
        if !matches!(rx.kind, RawExportKind::Named) {
            continue;
        }
        if rx.source.is_some() {
            continue;
        }
        if let Some((kind, type_sig)) = ambient_decls.get(&rx.name) {
            if rx.decl_kind.is_none() {
                rx.decl_kind = Some(kind.clone());
            }
            if rx.type_signature.is_none() && type_sig.is_some() {
                rx.type_signature = type_sig.clone();
            }
        }
    }

    for (node, key) in commonjs::computed_exports(root, source) {
        let Some(key) = key else { continue };
        let name = key.export_name();
        let kind = match key {
            property_key::PropertyKey::String(_) => RawExportKind::Named,
            property_key::PropertyKey::Symbol(_) | property_key::PropertyKey::Unique(_) => {
                RawExportKind::Symbol
            }
        };
        if exports
            .iter()
            .any(|export| export.name == name && export.kind == kind)
        {
            continue;
        }
        exports.push(RawExport {
            name,
            kind,
            line: node.start_position().row as u32 + 1,
            col: node.start_position().column as u32 + 1,
            source: None,
            decl_kind: None,
            is_type_only: false,
            release_tag: None,
            compound_members: Vec::new(),
            type_signature: None,
            satisfies_type: None,
            typeof_import_refs: Vec::new(),
            is_proxy_dispatched: false,
            local_name: None,
            declaration_site: None,
        });
    }

    let commonjs = commonjs::analyze(root, source);
    exports.retain(|export| commonjs.retains_export_at(root, source, export.line, export.col));
    link_export_declarations(root, source, &mut exports);
    for export in &mut exports {
        let (line, col) = export.declaration_site.unwrap_or((export.line, export.col));
        if let Some(dispatched) = runtime_surface::dispatch_at(root, source, line, col) {
            export.is_proxy_dispatched = dispatched;
        }
    }
    normalize_export_names(root, source, &mut exports);
    Ok(exports)
}

fn normalize_export_names(root: tree_sitter::Node, source: &[u8], exports: &mut [RawExport]) {
    for export in exports {
        let position = tree_sitter::Point::new(
            export.line.saturating_sub(1) as usize,
            export.col.saturating_sub(1) as usize,
        );
        let Some(node) = root.named_descendant_for_point_range(position, position) else {
            continue;
        };
        if node.utf8_text(source).ok() == Some(export.name.as_str()) {
            if let Some(name) = property_key::source_name(node, source) {
                export.name = name;
            }
        }
        if let Some(local) = &mut export.local_name {
            if local.starts_with(['\'', '"']) {
                if let Some(name) = property_key::decode_string(local) {
                    *local = name;
                }
            } else if local.contains('\\') {
                if let Some(name) = property_key::decode_string(&format!("\"{local}\"")) {
                    *local = name;
                }
            }
        }
    }
}

/// Retain alias source names and point local exports at declarations, not trailers.
fn link_export_declarations(root: tree_sitter::Node, source: &[u8], exports: &mut [RawExport]) {
    fn declaration<'a>(
        node: tree_sitter::Node<'a>,
        source: &[u8],
        name: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        if node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            == Some(name)
            && matches!(
                node.kind(),
                "class_declaration"
                    | "abstract_class_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "function_declaration"
                    | "generator_function_declaration"
                    | "function_signature"
                    | "enum_declaration"
                    | "variable_declarator"
            )
        {
            return Some(node);
        }
        if !matches!(
            node.kind(),
            "program"
                | "export_statement"
                | "ambient_declaration"
                | "lexical_declaration"
                | "variable_declaration"
        ) {
            return None;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if let Some(found) = declaration(child, source, name) {
                return Some(found);
            }
        }
        None
    }
    for export in exports {
        let point = tree_sitter::Point::new(
            export.line.saturating_sub(1) as usize,
            export.col.saturating_sub(1) as usize,
        );
        let mut node = root.named_descendant_for_point_range(point, point);
        while let Some(current) = node {
            if current.kind() == "export_specifier" {
                export.local_name = current
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                export.is_type_only |= current
                    .utf8_text(source)
                    .is_ok_and(|text| text.trim_start().starts_with("type "));
            }
            if current.kind() == "export_statement" {
                if export.local_name.is_some() {
                    if let Some(specifier) = current
                        .child_by_field_name("source")
                        .and_then(|n| n.utf8_text(source).ok())
                    {
                        export.source = Some(strip_quotes(specifier).to_string());
                        export.kind = RawExportKind::NamedReexport;
                    }
                }
                export.is_type_only |= current
                    .utf8_text(source)
                    .is_ok_and(|text| text.trim_start().starts_with("export type "));
                break;
            }
            node = current.parent();
        }
        if export.source.is_some() {
            continue;
        }
        let name = export.local_name.as_deref().unwrap_or(&export.name);
        let Some(decl) = declaration(root, source, name) else {
            continue;
        };
        let Some(identifier) = decl.child_by_field_name("name") else {
            continue;
        };
        export.declaration_site = Some((
            identifier.start_position().row as u32 + 1,
            identifier.start_position().column as u32 + 1,
        ));
        export.decl_kind = Some(match decl.kind() {
            "class_declaration" | "abstract_class_declaration" => DeclarationKind::Class,
            "interface_declaration" => DeclarationKind::Interface,
            "type_alias_declaration" => DeclarationKind::TypeAlias,
            "function_declaration" | "function_signature" | "generator_function_declaration" => {
                DeclarationKind::Function
            }
            "enum_declaration" => DeclarationKind::Enum,
            _ => DeclarationKind::Variable,
        });
    }
}

/// ARCH-02: second-pass attachment of compound-component members.
///
/// Scans the program for `Target.Member = X;` and `Object.assign(Target, {... })`
/// patterns. When `Target` matches an already-exported symbol ( conservative),
/// appends matched members to that symbol's `compound_members` list. Silent when
/// the target is not an exported symbol — this correctly handles minified/internal
/// names where the target is an internal binding.
fn attach_compound_members(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    language: &tree_sitter::Language,
    exports: &mut [RawExport],
) {
    use std::collections::HashSet;
    let exported_names: HashSet<String> = exports.iter().map(|r| r.name.clone()).collect();

    let attach = |target: &str, member: &str, exports: &mut [RawExport]| {
        if !exported_names.contains(target) {
            return;
        }
        for rx in exports.iter_mut() {
            if rx.name == target && !rx.compound_members.iter().any(|m| m == member) {
                rx.compound_members.push(member.to_string());
            }
        }
    };

    // Form 1: `Target.Member = X;` top-level.
    if let Ok(q) = grammar::cached_query(language, queries::COMPOUND_ASSIGN_MEMBER_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, source);
        while let Some(m) = matches.next() {
            let mut target: Option<String> = None;
            let mut member: Option<String> = None;
            for cap in m.captures {
                let cap_name = q.capture_names()[cap.index as usize];
                let text = cap.node.utf8_text(source).ok().map(|s| s.to_string());
                match cap_name {
                    "target" => target = text,
                    "member" => member = text,
                    _ => {}
                }
            }
            if let (Some(t), Some(mm)) = (target, member) {
                attach(&t, &mm, exports);
            }
        }
    }

    // Form 3: `Object.assign(Target, { Member: X,... })`.
    // Guard with a runtime @obj == "Object" && @assign_prop == "assign" check.
    if let Ok(q) = grammar::cached_query(language, queries::COMPOUND_OBJECT_ASSIGN_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, source);
        while let Some(m) = matches.next() {
            let mut obj: Option<String> = None;
            let mut assign_prop: Option<String> = None;
            let mut target: Option<String> = None;
            let mut members: Vec<String> = Vec::new();
            for cap in m.captures {
                let cap_name = q.capture_names()[cap.index as usize];
                let text = cap.node.utf8_text(source).ok().map(|s| s.to_string());
                match cap_name {
                    "obj" => obj = text,
                    "assign_prop" => assign_prop = text,
                    "target" => target = text,
                    "member" => {
                        if let Some(t) = text {
                            members.push(t);
                        }
                    }
                    _ => {}
                }
            }
            if obj.as_deref() != Some("Object") || assign_prop.as_deref() != Some("assign") {
                continue;
            }
            if let Some(t) = target {
                for mm in members {
                    attach(&t, &mm, exports);
                }
            }
        }
    }
}

/// Recursively expand namespace members, producing dotted names like A.B.baz.
/// `prefix` is the dotted name so far (e.g., "A" or "A.B").
/// supports 2+ levels of nesting.
#[allow(clippy::only_used_in_recursion)]
fn expand_namespace_members(
    ns_body: tree_sitter::Node,
    prefix: &str,
    source: &[u8],
    exports: &mut Vec<RawExport>,
    language: &tree_sitter::Language,
) {
    let mut body_cursor = ns_body.walk();
    for stmt in ns_body.children(&mut body_cursor) {
        if stmt.kind() == "export_statement" {
            let mut stmt_cursor = stmt.walk();
            for decl_child in stmt.children(&mut stmt_cursor) {
                let decl_kind_str = decl_child.kind();

                // Check for nested namespace: `export namespace B {... }`
                if decl_kind_str == "internal_module" {
                    if let Some(nested_name_node) = decl_child.child_by_field_name("name") {
                        if let Ok(nested_name) = nested_name_node.utf8_text(source) {
                            let nested_prefix = format!("{}.{}", prefix, nested_name);
                            // Emit the namespace itself as a Named export
                            let line = decl_child.start_position().row as u32 + 1;
                            let col = decl_child.start_position().column as u32 + 1;
                            let already = exports.iter().any(|e| e.name == nested_prefix);
                            if !already {
                                exports.push(RawExport {
                                    name: nested_prefix.clone(),
                                    kind: RawExportKind::Named,
                                    line,
                                    col,
                                    source: None,
                                    decl_kind: Some(DeclarationKind::Namespace),
                                    is_type_only: false,
                                    release_tag: None,
                                    compound_members: Vec::new(),
                                    type_signature: None,
                                    satisfies_type: None,
                                    typeof_import_refs: Vec::new(),
                                    is_proxy_dispatched: false,
                                    local_name: None,
                                    declaration_site: None,
                                });
                            }
                            // Recurse into the nested namespace body
                            if let Some(body) = decl_child.child_by_field_name("body") {
                                expand_namespace_members(
                                    body,
                                    &nested_prefix,
                                    source,
                                    exports,
                                    language,
                                );
                            }
                        }
                    }
                    continue;
                }

                // Extract name from the inner declaration (same logic as before)
                let inner_name = match decl_kind_str {
                    "function_declaration"
                    | "generator_function_declaration"
                    | "class_declaration"
                    | "abstract_class_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "enum_declaration"
                    | "function_signature" => decl_child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string()),
                    "lexical_declaration" => {
                        // export const baz =... inside namespace
                        let mut found = None;
                        let mut ldc = decl_child.walk();
                        for ld_child in decl_child.children(&mut ldc) {
                            if ld_child.kind() == "variable_declarator" {
                                found = ld_child
                                    .child_by_field_name("name")
                                    .and_then(|n| n.utf8_text(source).ok())
                                    .map(|s| s.to_string());
                                break;
                            }
                        }
                        found
                    }
                    _ => None,
                };
                if let Some(member_name) = inner_name {
                    let dotted = format!("{}.{}", prefix, member_name);
                    let line = decl_child.start_position().row as u32 + 1;
                    let col = decl_child.start_position().column as u32 + 1;
                    let decl_kind = Some(decl_kind_from_node_kind(decl_kind_str));
                    // Don't emit duplicates
                    let already = exports.iter().any(|e| e.name == dotted);
                    if !already {
                        exports.push(RawExport {
                            name: dotted,
                            kind: RawExportKind::Named,
                            line,
                            col,
                            source: None,
                            decl_kind,
                            is_type_only: false,
                            release_tag: None,
                            compound_members: Vec::new(),
                            type_signature: None,
                            satisfies_type: None,
                            typeof_import_refs: Vec::new(),
                            is_proxy_dispatched: false,
                            local_name: None,
                            declaration_site: None,
                        });
                    }
                }
            }
        }
    }
}

/// META-03: Emit synthetic keys for top-level declaration members inside an
/// augmentation body (`declare module 'X' {... }` or `declare global {... }`).
///
/// Naming convention: `augment:<scope>:*<member>` where scope is either the
/// module name (e.g., `"express"`) or `"global"`.
///
/// Each top-level declaration kind that is commonly augmented — interface,
/// type alias, class, function, const/let/var — is detected by node kind.
/// Other kinds (comments, punctuation) are skipped.
fn emit_augmentation_members(
    body: tree_sitter::Node,
    augment_scope: &str,
    source: &[u8],
    exports: &mut Vec<RawExport>,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        let kind = child.kind();
        let member_name = match kind {
            "interface_declaration"
            | "type_alias_declaration"
            | "class_declaration"
            | "abstract_class_declaration"
            | "function_declaration"
            | "function_signature" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(String::from),
            "variable_declaration" | "lexical_declaration" => {
                // Find first variable_declarator name — collect to avoid borrow issue
                let mut vc = child.walk();
                let children_vec: Vec<_> = child.children(&mut vc).collect();
                children_vec
                    .into_iter()
                    .find(|n| n.kind() == "variable_declarator")
                    .and_then(|decl| decl.child_by_field_name("name"))
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(String::from)
            }
            _ => None,
        };
        if let Some(name) = member_name {
            let synthetic_key = format!("augment:{}:*{}", augment_scope, name);
            if !exports.iter().any(|e| e.name == synthetic_key) {
                exports.push(RawExport {
                    name: synthetic_key,
                    kind: RawExportKind::Named,
                    line: child.start_position().row as u32 + 1,
                    col: child.start_position().column as u32 + 1,
                    source: Some(augment_scope.to_string()),
                    decl_kind: None,
                    is_type_only: false,
                    release_tag: None,
                    compound_members: Vec::new(),
                    type_signature: None,
                    satisfies_type: None,
                    typeof_import_refs: Vec::new(),
                    is_proxy_dispatched: false,
                    local_name: None,
                    declaration_site: None,
                });
            }
        }
    }
}

/// Recursively extract exported names from nested destructuring patterns.
/// Handles `{ a: { b } }`, `[a, [b, c]]`, `{ a: [b, c] }`, etc.
///.
fn extract_nested_destructuring_names(
    pattern_node: tree_sitter::Node,
    source: &[u8],
    names: &mut Vec<(String, u32, u32)>,
) {
    let mut cursor = pattern_node.walk();
    for child in pattern_node.children(&mut cursor) {
        match child.kind() {
            "shorthand_property_identifier_pattern" => {
                // `{ foo }` — foo is both key and value
                if let Ok(name) = child.utf8_text(source) {
                    let line = child.start_position().row as u32 + 1;
                    let col = child.start_position().column as u32 + 1;
                    names.push((name.to_string(), line, col));
                }
            }
            "pair_pattern" => {
                // `{ key: value }` — value is what we export
                if let Some(value_node) = child.child_by_field_name("value") {
                    match value_node.kind() {
                        "identifier" => {
                            if let Ok(name) = value_node.utf8_text(source) {
                                let line = value_node.start_position().row as u32 + 1;
                                let col = value_node.start_position().column as u32 + 1;
                                names.push((name.to_string(), line, col));
                            }
                        }
                        "object_pattern" | "array_pattern" => {
                            // Nested: `{ a: { b } }` or `{ a: [b, c] }`
                            extract_nested_destructuring_names(value_node, source, names);
                        }
                        _ => {}
                    }
                }
            }
            "identifier" => {
                // Direct identifier in array pattern: `[a, b]`
                if let Ok(name) = child.utf8_text(source) {
                    let line = child.start_position().row as u32 + 1;
                    let col = child.start_position().column as u32 + 1;
                    names.push((name.to_string(), line, col));
                }
            }
            "rest_pattern" => {
                // `...rest` in array pattern
                if let Some(inner) = child.child(1) {
                    if inner.kind() == "identifier" {
                        if let Ok(name) = inner.utf8_text(source) {
                            let line = inner.start_position().row as u32 + 1;
                            let col = inner.start_position().column as u32 + 1;
                            names.push((name.to_string(), line, col));
                        }
                    }
                }
            }
            "object_pattern" | "array_pattern" => {
                // Nested pattern: `[a, [b, c]]`
                extract_nested_destructuring_names(child, source, names);
            }
            _ => {}
        }
    }
}

/// Collect (line, col) positions of local names captured by re-export queries.
/// Used to exclude those positions from the plain named export results.
///
/// Handles both `export { foo } from '...'` and `export { foo as bar } from '...'`.
/// For the alias form, the local-name (`foo`) position is collected so the plain
/// export query doesn't double-count it.
fn collect_reexport_positions(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    language: &tree_sitter::Language,
) -> anyhow::Result<std::collections::HashSet<(u32, u32)>> {
    let mut positions = std::collections::HashSet::new();

    // Collect positions from the plain re-export query (@name = index 0).
    let query = grammar::cached_query(language, queries::NAMED_REEXPORT_QUERY)
        .context("failed to compile NAMED_REEXPORT_QUERY for dedup")?;
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @name — the local name node
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    positions.insert((line, col));
                }
            }
        }
    }

    // Also collect local-name positions from the alias re-export query (@local_name = index 0).
    if let Ok(alias_query) = grammar::cached_query(language, queries::NAMED_REEXPORT_ALIAS_QUERY) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&alias_query, root, source);
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == 0 {
                    // @local_name
                    let node = capture.node;
                    let line = node.start_position().row as u32 + 1;
                    let col = node.start_position().column as u32 + 1;
                    positions.insert((line, col));
                }
            }
        }
    }

    Ok(positions)
}

/// Strip leading and trailing quote characters from a string literal as returned
/// by tree-sitter (e.g., `"'./utils'"` -> `"./utils"`).
fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
        || (s.starts_with('`') && s.ends_with('`'))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod umd_query_tests;

#[cfg(test)]
mod proxy_detection_tests;
