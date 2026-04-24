// Comparison engine: diff old vs new ExportMaps

pub mod asset_diff;
pub mod behavior_diff;
pub mod class_diff;
pub mod css_diff;
pub mod directive_diff;
pub mod enum_diff;
pub mod exports_diff;
pub mod generics_diff;
pub mod interface_diff;
pub mod json_diff;
pub mod metadata;
pub mod ns_move_cascade;
pub mod ref_diff;
pub mod rename_coalesce;
pub mod subtype;
pub mod type_diff;

use std::collections::HashSet;
use std::path::Path;

use crate::discover::tree_query;
use crate::types::{
    ClassShape, ConstructSignature, EnumShape, ExportMap, Finding, ReleaseTag, Severity, TypeParam,
};
use subtype::{RULE_AUGMENT_GLOBAL_CHANGED, RULE_AUGMENT_MODULE_CHANGED, RULE_EXPORT_DEPRECATED};

/// SEM-08: Normalize rollup-plugin-dts / api-extractor-style internal
/// disambiguation suffixes (`T$1`, `T$2`, `T$10`) back to the base name `T`,
/// but ONLY when the base name `T` is in the provided exported-names set.
///
/// If `T$1` is mentioned in a type signature but the base `T` is not exported,
/// leave it as-is because it represents a genuinely distinct internal type
/// (Pitfall 10 in Phase 17 research — prevent false normalization of unrelated
/// internal identifiers that happen to use `$` convention).
///
/// Fast path: if the input contains no `$` character, returns a clone immediately.
fn normalize_rollup_suffix(input: &str, exported_names: &HashSet<String>) -> String {
    // Quick check: if no "$" in the input, skip all work.
    if !input.contains('$') {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        // Accumulate an identifier candidate (alphabetic or underscore start)
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();

            // Check for $<digits> suffix immediately following the identifier
            if i < chars.len() && chars[i] == '$' {
                let dollar_pos = i;
                i += 1; // consume '$'
                let digits_start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i > digits_start {
                    // WR-01 (Phase 17 review): after `$<digits>`, check for an identifier-
                    // continuation char. If present, the `$N` sits MID-identifier
                    // (e.g., `price$1plus2`) and must NOT be split — we'd otherwise
                    // concatenate two parse tokens (`price` + `plus2` -> `priceplus2`)
                    // or, worse, emit `ident` verbatim when it's exported.
                    if i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        // Treat the entire span as a single identifier. Emit the
                        // prefix + `$digits` + the continuation chars verbatim.
                        out.push_str(&ident);
                        out.extend(chars[dollar_pos..i].iter());
                        let rest_start = i;
                        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                            i += 1;
                        }
                        out.extend(chars[rest_start..i].iter());
                        continue;
                    }

                    // Pure suffix form `Ident$N` — safe to normalize.
                    if exported_names.contains(&ident) {
                        // Base is exported: emit just `ident` (strip $digits)
                        out.push_str(&ident);
                    } else {
                        // Base NOT exported: preserve full `ident$digits` form
                        out.push_str(&ident);
                        out.extend(chars[dollar_pos..i].iter());
                    }
                    continue;
                } else {
                    // `$` not followed by digits — emit identifier and `$` literally
                    out.push_str(&ident);
                    out.push('$');
                    continue;
                }
            } else {
                // No `$` suffix — emit identifier as-is
                out.push_str(&ident);
            }
        } else {
            out.push(c);
            i += 1;
        }
    }

    out
}

/// Compare two export maps and produce a list of findings.
///
/// - Exports present in `old` but absent in `new` → `export-removed` (Major)
/// - Exports present in `new` but absent in `old` → `export-added` (Minor)
///
/// D-04 big-bang (Phase 18): unified signature always takes `pkg_root`. IDN-01
/// rename detection scans new-side source files for alias trailers; IDN-02
/// namespace-move consolidation doesn't read files but accepts `pkg_root`
/// uniformly for symmetry. All callers pass the package root they're comparing
/// against (typically the "current"/new-side package root).
///
/// The public `compare_exports` wraps `compare_exports_core` (base diff without
/// sort/post-pass) with the IDN-01 + IDN-02 post-passes and a single
/// total-order sort at the tail (Warning 10: no double-sort).
///
/// Findings are sorted by (severity, file, line, col, rule) at the tail.
pub fn compare_exports(old: &ExportMap, new: &ExportMap, pkg_root: &Path) -> Vec<Finding> {
    // Base diff: ExportKey iteration, augment routing, type-aware compare.
    let mut findings = compare_exports_core(old, new);
    // IDN-01: transparent rename coalescing (alias-trailer-only, D-05).
    findings = rename_coalesce::coalesce_renames(old, new, findings, pkg_root);
    // IDN-02: namespace-move cascade consolidation (exact-subtree, depth 1).
    findings = ns_move_cascade::consolidate_namespace_moves(old, new, findings);
    // Warning 10 (resolved by Blocker 1 option a): single total-order sort at
    // the tail. Post-passes see unsorted findings; this is the only sort call.
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
            .then_with(|| a.rule.cmp(b.rule))
    });
    findings
}

/// Base export diff — ExportKey iteration, augment-prefix routing, type-aware
/// comparison. No sort, no post-passes. Called by `compare_exports` which
/// wraps this with IDN-01 + IDN-02 post-passes and a single total-order sort.
fn compare_exports_core(old: &ExportMap, new: &ExportMap) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    // META-04 / META-06 (Phase 19): Filter non-public release tags before comparison.
    // Symbols tagged @internal, @alpha, or @beta are considered non-public per the
    // locked v0.3 Phase 19 decision (D-01); only @public and no-tag remain comparable.
    //
    // WR-03 (Phase 19 review): `ExportMap::clone()` also deep-clones the
    // `directives: BTreeMap<PathBuf, Vec<DirectiveRecord>>` sidecar, but the
    // directive-diff block below reads from the UNFILTERED `old`/`new` maps
    // (directives are a file-level concept and are unrelated to per-symbol
    // release tags). Drop the cloned directives on the filtered copies so
    // we are not carrying dead data through the retain calls.
    let mut filtered_old = old.clone();
    filtered_old.directives.clear();
    filtered_old.retain(|_name, sym| {
        !matches!(
            sym.release_tag,
            Some(ReleaseTag::Internal) | Some(ReleaseTag::Alpha) | Some(ReleaseTag::Beta)
        )
    });
    let mut filtered_new = new.clone();
    filtered_new.directives.clear();
    filtered_new.retain(|_name, sym| {
        !matches!(
            sym.release_tag,
            Some(ReleaseTag::Internal) | Some(ReleaseTag::Alpha) | Some(ReleaseTag::Beta)
        )
    });

    // Removed exports (DET-01, DET-02)
    for (key, sym) in &filtered_old {
        let name = key.name.as_str();
        if !filtered_new.contains_key(key) {
            // META-03: route augmentation member keys to specific rules
            if name.starts_with("augment:global:") {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_AUGMENT_GLOBAL_CHANGED,
                    file: sym.file.clone(),
                    line: sym.line,
                    col: sym.col,
                    message: format!("global augmentation member removed: {}", name),
                });
                continue;
            }
            if name.starts_with("augment:") && name.contains(":*") {
                findings.push(Finding {
                    severity: Severity::Major,
                    rule: RULE_AUGMENT_MODULE_CHANGED,
                    file: sym.file.clone(),
                    line: sym.line,
                    col: sym.col,
                    message: format!("module augmentation member removed: {}", name),
                });
                continue;
            }
            // Phase 18: qualify message with subpath/condition when non-canonical
            // to disambiguate multi-subpath / multi-condition exports.
            let is_root_no_cond = key.subpath == "." && key.condition.is_none();
            let qualifier = if is_root_no_cond {
                String::new()
            } else {
                format!(
                    " from {}{}",
                    key.subpath,
                    key.condition
                        .as_deref()
                        .map(|c| format!(" ({})", c))
                        .unwrap_or_default()
                )
            };
            let message = if name == "default" {
                if is_root_no_cond {
                    "default export was removed".to_string()
                } else {
                    format!("default export was removed{}", qualifier)
                }
            } else if let Some(symbol_part) = name.strip_prefix("@@") {
                // ECF-13 (Phase 22): well-known-Symbol removal branch. Present
                // the synthetic `@@<name>` internal key as the human-readable
                // `[Symbol.<name>]` form in the CLI message. Same
                // `export-removed` rule-ID — CONTEXT.md D-07 locks reuse over a
                // new rule-ID.
                format!(
                    "well-known Symbol export `[Symbol.{}]` was removed{} - consumers relying on this protocol will break",
                    symbol_part, qualifier
                )
            } else {
                format!("exported symbol `{}` was removed{}", name, qualifier)
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message,
            });
        }
    }

    // New exports (DET-03, DET-04, OUT-05): compatible additions reported as Minor
    for (key, sym) in &filtered_new {
        let name = key.name.as_str();
        if !filtered_old.contains_key(key) {
            // META-03: augmentation member additions also use specific rules
            if name.starts_with("augment:global:") {
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_AUGMENT_GLOBAL_CHANGED,
                    file: sym.file.clone(),
                    line: sym.line,
                    col: sym.col,
                    message: format!("global augmentation member added: {}", name),
                });
                continue;
            }
            if name.starts_with("augment:") && name.contains(":*") {
                findings.push(Finding {
                    severity: Severity::Minor,
                    rule: RULE_AUGMENT_MODULE_CHANGED,
                    file: sym.file.clone(),
                    line: sym.line,
                    col: sym.col,
                    message: format!("module augmentation member added: {}", name),
                });
                continue;
            }
            // Phase 18: qualify message with subpath/condition when non-canonical
            let is_root_no_cond = key.subpath == "." && key.condition.is_none();
            let qualifier = if is_root_no_cond {
                String::new()
            } else {
                format!(
                    " at {}{}",
                    key.subpath,
                    key.condition
                        .as_deref()
                        .map(|c| format!(" ({})", c))
                        .unwrap_or_default()
                )
            };
            findings.push(Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!("exported symbol `{}` was added{}", name, qualifier),
            });
        }
    }

    // META-04: Surface @deprecated symbols that exist in both old and new as Warning.
    for (key, sym) in &filtered_new {
        let name = key.name.as_str();
        if matches!(sym.release_tag, Some(ReleaseTag::Deprecated)) && filtered_old.contains_key(key)
        {
            findings.push(Finding {
                severity: Severity::Warning,
                rule: RULE_EXPORT_DEPRECATED,
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!("export `{}` is marked @deprecated", name),
            });
        }
    }

    // Type-aware comparison (D-08, D-09): only when both symbols have TypeShape
    // Use first overload (primary signature) for type diff
    for (key, old_sym) in &filtered_old {
        let name = key.name.as_str();
        if let Some(new_sym) = filtered_new.get(key) {
            if let (Some(old_shape), Some(new_shape)) =
                (old_sym.overloads.first(), new_sym.overloads.first())
            {
                let type_findings = type_diff::diff_types(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                );
                findings.extend(type_findings);
            }
            // TYPE-01 (Phase 20): `satisfies T` removal emits Minor
            // `type-narrowing-widened`. Fires regardless of TypeShape overload
            // presence — the satisfies_type field is the direct source of truth.
            findings.extend(type_diff::diff_satisfies_types(
                old_sym,
                new_sym,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }
    }

    // INF-05 (Phase 19): triple-slash directive diffs per source file.
    // Iterate the union of keys from old.directives and new.directives and
    // feed each per-file directive list pair through compare_directives.
    // Directives are a file-level concept, not per-symbol, so we read from
    // the unfiltered maps (the @internal/@alpha/@beta filter that preceded
    // this block strips SYMBOLS; directives are unrelated to release tags).
    {
        let all_files: std::collections::BTreeSet<&std::path::PathBuf> = old
            .directives
            .keys()
            .chain(new.directives.keys())
            .collect();
        let empty_vec: Vec<crate::discover::comment_scan::DirectiveRecord> = Vec::new();
        for file in all_files {
            let old_dirs = old.directives.get(file).unwrap_or(&empty_vec).as_slice();
            let new_dirs = new.directives.get(file).unwrap_or(&empty_vec).as_slice();
            findings.extend(crate::compare::directive_diff::compare_directives(
                old_dirs, new_dirs, file,
            ));
        }
    }

    // Warning 10 (Blocker 1 fix, Phase 18): NO sort here — the public
    // `compare_exports` wrapper performs the single total-order sort after
    // IDN-01 + IDN-02 post-passes. Sorting here would mean double-sort.
    findings
}

/// Orchestrates exports map diffing (EXP-01..13).
/// Reads package.json from both roots and delegates to exports_diff::compare_exports_diff.
/// Per D-03: findings merge in compare/mod.rs, not in main.rs.
pub fn compare_exports_map_diff(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let old_pkg_path = old_root.join("package.json");
    let new_pkg_path = new_root.join("package.json");

    let old_content = match std::fs::read_to_string(&old_pkg_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let new_content = match std::fs::read_to_string(&new_pkg_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let old_pkg: serde_json::Value = match serde_json::from_str(&old_content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let new_pkg: serde_json::Value = match serde_json::from_str(&new_content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    exports_diff::compare_exports_diff(&old_pkg, &new_pkg, old_root, new_root)
}

/// Orchestrates asset & schema comparison (AST-01..06).
/// Runs JSON diff, CSS diff, and asset file diff, merging all findings.
pub fn compare_asset_diff(old_root: &Path, new_root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // AST-01, AST-02, AST-06: JSON field diff
    findings.extend(json_diff::compare_json_files(old_root, new_root));

    // AST-03, AST-04: CSS custom property and class name diff
    findings.extend(css_diff::compare_css_files(old_root, new_root));

    // AST-05: Asset file presence diff
    findings.extend(asset_diff::compare_asset_files(old_root, new_root));

    findings
}

/// Returns true if any finding has Major severity (per CLI-02).
pub fn has_breaking_changes(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Major)
}

// ─── Structural diff orchestrator ────────────────────────────────────────────

/// Owned extraction result for a single declaration node.
///
/// Created by `extract_declaration` which parses the file and immediately extracts
/// all needed data into owned types, avoiding lifetime issues with tree-sitter nodes.
#[allow(dead_code)]
struct ExtractedDecl {
    is_class: bool,
    is_enum: bool,
    is_function: bool,
    is_interface: bool,
    /// TYPE-02 (Phase 20): true iff the declaration is a `type X = <type>`
    /// alias. Used by `compare_structural_diff` to route type-alias RHS
    /// pairs through `diff_types` (the base `compare_exports_core` path
    /// only fires for functions because resolver-side `overloads` is
    /// empty for type aliases without LSP).
    is_type_alias: bool,
    class_shape: Option<ClassShape>,
    enum_shape: Option<EnumShape>,
    type_params: Vec<TypeParam>,
    // NEW for Phase 8 REF rules:
    index_signatures: Vec<tree_query::IndexSignature>,
    type_guard_type: Option<String>,
    overloads: Vec<crate::types::shape::TypeShape>,
    // NEW for Phase 9 BEH-01/BEH-02:
    is_async: bool,
    is_generator: bool,
    // NEW for Phase 17 TLF rules:
    construct_signatures: Vec<ConstructSignature>,
    this_param_type: Option<String>,
    assertion_target: Option<String>,
}

/// Check if an arrow_function node has the `async` keyword.
///
/// Arrow functions have no `"function"` keyword — instead, `"async"` appears
/// before the `"parameters"` or `"identifier"` children. Per Phase 14 Research Pitfall 7.
fn is_async_arrow(arrow_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = arrow_node.walk();
    for child in arrow_node.children(&mut cursor) {
        // Past the point where "async" would appear
        if child.kind() == "formal_parameters" || child.kind() == "identifier" {
            break;
        }
        if !child.is_named() && child.utf8_text(source).ok() == Some("async") {
            return true;
        }
    }
    false
}

/// Parse `source` with the given language, find the declaration for `name` at
/// approximately `hint_line` (1-indexed), extract class/enum/type-param data
/// into owned structures, and return them.
///
/// Returns `None` when the declaration cannot be located (e.g., unsupported
/// language, parse failure, name not found in tree).
fn extract_declaration(
    source: &[u8],
    lang: &tree_query::SourceLanguage,
    name: &str,
    hint_line: u32,
) -> Option<ExtractedDecl> {
    // INF-01 (Phase 18): TypeScriptTsx routes to LANGUAGE_TSX — strict superset
    // of LANGUAGE_TYPESCRIPT so every TS query still parses identically.
    let ts_lang: tree_sitter::Language = match lang {
        tree_query::SourceLanguage::TypeScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        tree_query::SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        tree_query::SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    };

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    // Walk top-level children of the program node looking for a declaration
    // whose name field matches `name` and whose start line is within ±3 of hint_line.
    //
    // Supported kinds:
    //   class_declaration, abstract_class_declaration,
    //   enum_declaration,
    //   function_declaration, interface_declaration, type_alias_declaration,
    //   lexical_declaration (export const/let — the variable_declarator carries the name)
    let hint = hint_line.saturating_sub(1) as usize; // 0-indexed

    let decl_node = find_decl_in_node(root, source, name, hint)?;
    let kind = decl_node.kind();

    // REQ-14-01: arrow_function and function_expression in variables
    // REQ-14-02: function_expression and generator_function_expression as anonymous defaults
    // Note: tree-sitter uses "function_expression" for `function() {}` values (not "function").
    let is_arrow_function = kind == "arrow_function";
    let is_anonymous_function = matches!(
        kind,
        "function_expression" | "generator_function_expression"
    );
    let is_anonymous_class = kind == "class";

    // Treat arrow functions and anonymous functions as function declarations
    let is_function = matches!(
        kind,
        "function_declaration" | "function_signature" | "generator_function_declaration"
    ) || is_arrow_function
        || is_anonymous_function;

    // REQ-14-02: anonymous class nodes (export default class {}) treated as classes
    let is_class =
        matches!(kind, "class_declaration" | "abstract_class_declaration") || is_anonymous_class;
    let is_enum = kind == "enum_declaration";
    let is_interface = kind == "interface_declaration";
    // TYPE-02 (Phase 20): recognize type aliases so compare_structural_diff
    // can route RHS pairs through diff_types.
    let is_type_alias = kind == "type_alias_declaration";

    // parse_class_shape handles both class_declaration and class (anonymous) nodes
    // since both have a class_body child
    let class_shape = if is_class {
        Some(tree_query::parse_class_shape(source, decl_node))
    } else {
        None
    };

    let enum_shape = if is_enum {
        Some(tree_query::parse_enum_shape(source, decl_node))
    } else {
        None
    };

    // Per D-06: extract type params from ALL declaration types.
    let type_params = tree_query::parse_type_params(source, decl_node);

    // Extract index signatures for interfaces (REF-07)
    let index_signatures = if is_interface {
        tree_query::find_interface_body(decl_node)
            .map(|body| tree_query::extract_index_signatures(body, source))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Extract type guard for functions (REF-03)
    // is_function now includes arrow functions, so extract_type_guard works automatically
    let type_guard_type = if is_function {
        tree_query::extract_type_guard(decl_node, source)
    } else {
        None
    };

    // Build overload shape for function signatures (REF-01, REF-02)
    // build_function_shape_from_node uses child_by_field_name("parameters") which
    // works on arrow_function nodes (field name is also "parameters")
    let overloads = if is_function {
        tree_query::build_function_shape_from_node(decl_node, source)
            .into_iter()
            .collect()
    } else if kind == "type_alias_declaration" {
        // TYPE-02 (Phase 20): route the RHS of `type X = <type>` through
        // `parse_type_string` into `overloads[0]` so the existing
        // `diff_types` pipeline can compare the shape. Without this,
        // non-LSP runs (where `type_signature` stays None) have no path
        // from a type-alias source to `TypeShape`, and the
        // (InferExtends, InferExtends) diff arm never fires.
        // Limited to a single TypeShape (no true overloads for type aliases).
        decl_node
            .child_by_field_name("value")
            .and_then(|value_node| value_node.utf8_text(source).ok())
            .map(crate::types::parser::parse_type_string)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    // BEH-01/BEH-02: extract async/generator modifiers from tree-sitter node
    // Arrow functions use is_async_arrow because they lack the "function" keyword
    let is_async_fn = if is_arrow_function {
        is_async_arrow(decl_node, source)
    } else if is_function {
        tree_query::is_async_function(decl_node, source)
    } else {
        false
    };
    // Arrow functions cannot be generators (JS/TS spec)
    let is_generator_fn = if is_arrow_function {
        false
    } else if is_function {
        tree_query::is_generator_function(decl_node, source)
    } else {
        false
    };

    // TLF-01: extract construct signatures from interface/class body
    let construct_signatures = if is_interface {
        tree_query::find_interface_body(decl_node)
            .map(|body| tree_query::extract_construct_signatures(body, source))
            .unwrap_or_default()
    } else if is_class {
        decl_node
            .child_by_field_name("body")
            .map(|body| tree_query::extract_construct_signatures(body, source))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // TLF-03: extract this parameter type from function declarations
    let this_param_type = if is_function {
        tree_query::extract_this_param_type(decl_node, source)
    } else {
        None
    };

    // TLF-04: extract assertion target from function declarations
    let assertion_target = if is_function {
        tree_query::extract_assertion_target(decl_node, source)
    } else {
        None
    };

    Some(ExtractedDecl {
        is_class,
        is_enum,
        is_function,
        is_interface,
        is_type_alias,
        class_shape,
        enum_shape,
        type_params,
        index_signatures,
        type_guard_type,
        overloads,
        is_async: is_async_fn,
        is_generator: is_generator_fn,
        construct_signatures,
        this_param_type,
        assertion_target,
    })
}

/// Recursively search `node` and its children for a declaration whose name
/// matches `name`. Returns the declaration node closest to `hint_row`
/// (0-indexed).
///
/// Uses a closest-match strategy: collects all candidates matching by name,
/// then returns the one nearest to `hint_row`. This handles file moves
/// between versions (GAP-13-05) and ambient declarations in .d.ts files
/// (BUG-13-01).
fn find_decl_in_node<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
    name: &str,
    hint_row: usize,
) -> Option<tree_sitter::Node<'a>> {
    // REQ-14-03: Handle dotted names (namespace members) by finding the namespace
    // first, then searching inside its body for the member.
    if name.contains('.') {
        let parts: Vec<&str> = name.splitn(2, '.').collect();
        let ns_name = parts[0];
        let member_name = parts[1];
        // Find the internal_module node for the namespace by scanning children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "internal_module" => {
                    let matches_ns = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|n| n == ns_name)
                        .unwrap_or(false);
                    if matches_ns {
                        if let Some(body) = child.child_by_field_name("body") {
                            return find_decl_in_node(body, source, member_name, hint_row);
                        }
                    }
                }
                "export_statement" | "ambient_declaration" => {
                    // Recurse into wrappers to find internal_module
                    if let Some(found) = find_decl_in_node(child, source, name, hint_row) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        return None;
    }

    let mut candidates: Vec<(usize, tree_sitter::Node<'a>)> = Vec::new(); // (distance, node)

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();

        let candidate = match kind {
            "class_declaration" | "abstract_class_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "function_declaration" | "generator_function_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "interface_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "type_alias_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "enum_declaration" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "function_signature" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "internal_module" => child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .filter(|n| *n == name)
                .map(|_| child),
            "lexical_declaration" | "variable_declaration" => {
                // REQ-14-01: Look for variable_declarator whose name matches,
                // then return the value node (arrow_function, function_expression, etc.)
                // Note: tree-sitter uses "function_expression" for `function(){}` values,
                // and "arrow_function" for `() => {}` values.
                let mut ld_cursor = child.walk();
                for ld_child in child.children(&mut ld_cursor) {
                    if ld_child.kind() == "variable_declarator" {
                        let matches_name = ld_child
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok())
                            .map(|n| n == name)
                            .unwrap_or(false);
                        if matches_name {
                            // Return the value node (arrow_function, function_expression, etc.)
                            if let Some(value) = ld_child.child_by_field_name("value") {
                                let value_kind = value.kind();
                                if matches!(
                                    value_kind,
                                    "arrow_function"
                                        | "function_expression"
                                        | "generator_function_expression"
                                ) {
                                    let found_row = value.start_position().row;
                                    let dist = found_row.abs_diff(hint_row);
                                    candidates.push((dist, value));
                                }
                            }
                            // Also recurse for nested declarations
                            if let Some(found) = find_decl_in_node(ld_child, source, name, hint_row)
                            {
                                let found_row = found.start_position().row;
                                let dist = found_row.abs_diff(hint_row);
                                candidates.push((dist, found));
                            }
                        }
                    }
                }
                None
            }
            "function_expression" | "generator_function_expression" => {
                // REQ-14-02: Anonymous function expression — only matches when name == "default"
                // (export default function() {})
                // Note: tree-sitter uses "function_expression" (not "function") for anonymous functions.
                if name == "default" {
                    let found_row = child.start_position().row;
                    let dist = found_row.abs_diff(hint_row);
                    candidates.push((dist, child));
                }
                None
            }
            "class" => {
                // REQ-14-02: Anonymous class — only matches when name == "default"
                // (export default class {})
                if name == "default" {
                    let found_row = child.start_position().row;
                    let dist = found_row.abs_diff(hint_row);
                    candidates.push((dist, child));
                }
                None
            }
            "export_statement" | "ambient_declaration" => {
                // Recurse into wrappers
                find_decl_in_node(child, source, name, hint_row)
            }
            "expression_statement" => {
                // REQ-15-01: CJS assignment patterns — module.exports = class/fn or exports.NAME = fn
                let mut es_cursor = child.walk();
                for es_child in child.children(&mut es_cursor) {
                    if es_child.kind() == "assignment_expression" {
                        let left = es_child.child_by_field_name("left");
                        let right = es_child.child_by_field_name("right");
                        if let (Some(left_node), Some(right_node)) = (left, right) {
                            if left_node.kind() == "member_expression" {
                                let obj = left_node
                                    .child_by_field_name("object")
                                    .and_then(|n| n.utf8_text(source).ok());
                                let prop = left_node
                                    .child_by_field_name("property")
                                    .and_then(|n| n.utf8_text(source).ok());

                                match (obj, prop) {
                                    (Some("module"), Some("exports")) if name == "default" => {
                                        // module.exports = class/function/arrow
                                        let rhs_kind = right_node.kind();
                                        if matches!(
                                            rhs_kind,
                                            "class"
                                                | "function_expression"
                                                | "generator_function"
                                                | "arrow_function"
                                        ) {
                                            let found_row = right_node.start_position().row;
                                            let dist = found_row.abs_diff(hint_row);
                                            candidates.push((dist, right_node));
                                        }
                                    }
                                    (Some("exports"), Some(prop_name)) if prop_name == name => {
                                        // exports.NAME = function/arrow
                                        let rhs_kind = right_node.kind();
                                        if matches!(
                                            rhs_kind,
                                            "function_expression"
                                                | "generator_function"
                                                | "arrow_function"
                                        ) {
                                            let found_row = right_node.start_position().row;
                                            let dist = found_row.abs_diff(hint_row);
                                            candidates.push((dist, right_node));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        };

        if let Some(found) = candidate {
            let found_row = found.start_position().row;
            let dist = found_row.abs_diff(hint_row);
            candidates.push((dist, found));
        }

        // Also recurse into export_statement and ambient_declaration wrappers
        // for deeper nesting
        if matches!(kind, "export_statement" | "ambient_declaration") {
            if let Some(found) = find_decl_in_node(child, source, name, hint_row) {
                let found_row = found.start_position().row;
                let dist = found_row.abs_diff(hint_row);
                candidates.push((dist, found));
            }
        }
    }

    // Return the candidate closest to hint_row
    candidates.sort_by_key(|(dist, _)| *dist);
    candidates.into_iter().next().map(|(_, n)| n)
}

/// Collect all overload signatures for a given function name from source.
///
/// Scans for `function_signature` nodes with matching name at the top level
/// (exported overload declarations). Returns Vec of TypeShape::Function,
/// one per function_signature node found.
fn collect_overload_shapes(
    source: &[u8],
    lang: &tree_query::SourceLanguage,
    name: &str,
) -> Vec<crate::types::shape::TypeShape> {
    // INF-01 (Phase 18): TypeScriptTsx routes to LANGUAGE_TSX so overload
    // signatures in JSX-bearing .tsx files are still discoverable.
    let ts_lang: tree_sitter::Language = match lang {
        tree_query::SourceLanguage::TypeScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        tree_query::SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        tree_query::SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&ts_lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let mut overloads = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        // Look for export_statement wrapping function_signature
        if child.kind() == "export_statement" {
            let mut ec = child.walk();
            for export_child in child.children(&mut ec) {
                if export_child.kind() == "function_signature" {
                    let matches_name = export_child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|n| n == name)
                        .unwrap_or(false);
                    if matches_name {
                        if let Some(shape) =
                            tree_query::build_function_shape_from_node(export_child, source)
                        {
                            overloads.push(shape);
                        }
                    }
                }
            }
        }
    }
    overloads
}

/// Collect members from ALL interface_declaration nodes with matching name.
///
/// Handles declaration merging (REQ-14-04): when two `interface Foo` declarations
/// exist in the same file, their members are unioned. This matches TypeScript
/// compiler behavior for merged interfaces.
fn collect_interface_members(
    source: &[u8],
    lang: &tree_query::SourceLanguage,
    name: &str,
) -> Vec<crate::types::ClassMember> {
    // INF-01 (Phase 18): TypeScriptTsx routes to LANGUAGE_TSX so merged-
    // interface members declared in JSX-bearing .tsx files are still collected.
    let ts_lang: tree_sitter::Language = match lang {
        tree_query::SourceLanguage::TypeScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        tree_query::SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        tree_query::SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&ts_lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let mut all_members: Vec<crate::types::ClassMember> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Walk top-level nodes (including through export_statement and ambient_declaration wrappers)
    fn scan_for_interfaces(
        node: tree_sitter::Node<'_>,
        source: &[u8],
        name: &str,
        all_members: &mut Vec<crate::types::ClassMember>,
        seen_names: &mut std::collections::HashSet<String>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "interface_declaration" => {
                    let matches = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|n| n == name)
                        .unwrap_or(false);
                    if matches {
                        if let Some(body) = tree_query::find_interface_body(child) {
                            let members = tree_query::parse_interface_members(source, body);
                            for member in members {
                                // Dedup by name — first declaration wins for same member
                                if seen_names.insert(member.name.clone()) {
                                    all_members.push(member);
                                }
                            }
                        }
                    }
                }
                "export_statement" | "ambient_declaration" => {
                    scan_for_interfaces(child, source, name, all_members, seen_names);
                }
                _ => {}
            }
        }
    }

    scan_for_interfaces(root, source, name, &mut all_members, &mut seen_names);
    all_members
}

/// Orchestrates class, enum, and generic type parameter diff for all exported symbols.
///
/// For each exported symbol present in both old and new:
/// - Reads the source file from both package roots
/// - Parses with tree-sitter to find the declaration node
/// - Extracts ClassShape / EnumShape / TypeParams depending on declaration type
/// - Runs the appropriate diff function
///
/// Per D-06: generic params are extracted from ALL declaration types
/// (class, function, interface, type alias), not just classes.
pub fn compare_structural_diff(
    old_root: &Path,
    new_root: &Path,
    old_exports: &ExportMap,
    new_exports: &ExportMap,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    // SEM-08: Build exported-names set ONCE per comparison for rollup suffix normalization.
    // The union of old and new exported names ensures any symbol visible in either version
    // can be used as a normalization anchor. This set is passed to normalize_rollup_suffix
    // to gate $N stripping on names that are actually exported (Pitfall 10).
    //
    // Phase 18: keys are ExportKey composites; we project to `name` for the
    // rollup-suffix matching code which operates on bare identifier strings.
    let exported_names: HashSet<String> = old_exports
        .keys()
        .chain(new_exports.keys())
        .map(|k| k.name.clone())
        .collect();

    for (key, old_sym) in old_exports {
        let name = key.name.as_str();
        let Some(new_sym) = new_exports.get(key) else {
            continue;
        };

        // Build absolute paths to the source files
        let old_path = old_root.join(&old_sym.file);
        let new_path = new_root.join(&new_sym.file);

        let Ok(old_source) = std::fs::read(&old_path) else {
            continue;
        };
        let Ok(new_source) = std::fs::read(&new_path) else {
            continue;
        };

        // Determine language from file extension
        let Some(old_lang) = tree_query::language_for_file(&old_sym.file) else {
            continue;
        };
        let Some(new_lang) = tree_query::language_for_file(&new_sym.file) else {
            continue;
        };

        // Extract all shapes from both versions
        let Some(old_decl) = extract_declaration(&old_source, &old_lang, name, old_sym.line) else {
            continue;
        };
        let Some(new_decl) = extract_declaration(&new_source, &new_lang, name, new_sym.line) else {
            continue;
        };

        // CLS-01..09: class diff
        if old_decl.is_class && new_decl.is_class {
            if let (Some(old_shape), Some(new_shape)) =
                (&old_decl.class_shape, &new_decl.class_shape)
            {
                findings.extend(class_diff::compare_class_shapes(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                ));
            }
        }

        // REQ-14-05: Interface member structural diff (degraded mode)
        // REQ-14-04: Declaration merging — collect members from ALL interface declarations
        if old_decl.is_interface && new_decl.is_interface {
            let old_members = collect_interface_members(&old_source, &old_lang, name);
            let new_members = collect_interface_members(&new_source, &new_lang, name);
            if !old_members.is_empty() || !new_members.is_empty() {
                findings.extend(interface_diff::compare_interface_members(
                    &old_members,
                    &new_members,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                ));
            }

            // GAP-16-04: Interface index signature diff (interface-level)
            findings.extend(interface_diff::compare_interface_index_signatures(
                &old_decl.index_signatures,
                &new_decl.index_signatures,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // ENUM-01..05: enum diff
        if old_decl.is_enum && new_decl.is_enum {
            if let (Some(old_shape), Some(new_shape)) = (&old_decl.enum_shape, &new_decl.enum_shape)
            {
                findings.extend(enum_diff::compare_enum_shapes(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                ));
            }
        }

        // GEN-01..06: generic type param diff — applies to ALL constructs per D-06
        if !old_decl.type_params.is_empty() || !new_decl.type_params.is_empty() {
            findings.extend(generics_diff::compare_type_params(
                &old_decl.type_params,
                &new_decl.type_params,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // REF-01..05, REF-07, REF-08: signature & type refinement diff
        // Supplement ExportSymbol data with tree-sitter extraction results
        // (LSP may not have set these fields)
        let mut old_sym_augmented = old_sym.clone();
        let mut new_sym_augmented = new_sym.clone();

        if old_sym_augmented.type_guard_type.is_none() {
            old_sym_augmented.type_guard_type = old_decl.type_guard_type.clone();
        }
        if new_sym_augmented.type_guard_type.is_none() {
            new_sym_augmented.type_guard_type = new_decl.type_guard_type.clone();
        }

        // BEH-01/BEH-02: augment async/generator from tree-sitter extraction
        if old_decl.is_function {
            old_sym_augmented.is_async = old_decl.is_async;
            old_sym_augmented.is_generator = old_decl.is_generator;
        }
        if new_decl.is_function {
            new_sym_augmented.is_async = new_decl.is_async;
            new_sym_augmented.is_generator = new_decl.is_generator;
        }

        // Collect all overload shapes for functions (REF-01)
        if old_decl.is_function {
            let collected = collect_overload_shapes(&old_source, &old_lang, name);
            if !collected.is_empty() {
                old_sym_augmented.overloads = collected;
            } else if old_sym_augmented.overloads.is_empty() && !old_decl.overloads.is_empty() {
                old_sym_augmented.overloads = old_decl.overloads.clone();
            }
        }
        if new_decl.is_function {
            let collected = collect_overload_shapes(&new_source, &new_lang, name);
            if !collected.is_empty() {
                new_sym_augmented.overloads = collected;
            } else if new_sym_augmented.overloads.is_empty() && !new_decl.overloads.is_empty() {
                new_sym_augmented.overloads = new_decl.overloads.clone();
            }
        }

        // SEM-08: Normalize rollup-plugin-dts $N suffixes on type_signature fields
        // before they flow into ref_diff and type_diff comparisons. We use local
        // normalized copies to avoid mutating the ExportMap.
        if let Some(sig) = &old_sym_augmented.type_signature {
            old_sym_augmented.type_signature = Some(normalize_rollup_suffix(sig, &exported_names));
        }
        if let Some(sig) = &new_sym_augmented.type_signature {
            new_sym_augmented.type_signature = Some(normalize_rollup_suffix(sig, &exported_names));
        }

        findings.extend(ref_diff::compare_ref_changes(
            &old_sym_augmented,
            &new_sym_augmented,
            name,
            &new_sym.file,
            new_sym.line,
            new_sym.col,
            &old_decl.index_signatures,
            &new_decl.index_signatures,
        ));

        // GEN-07 (Phase 20): NoInfer<T> wrapper removed from a function-
        // parameter position. Walks paired Function overloads from the
        // augmented symbols (which carry the tree-sitter-extracted shapes
        // even in non-LSP runs). Falls through Opaque param-text via
        // parse_type_string so the GEN-07 wrapper canonicalization (in
        // parse_generic) surfaces here as TypeShape::NoInferWrap. Out of
        // scope: return-type position (RESEARCH §GEN-07 Open Question 3).
        if old_decl.is_function && new_decl.is_function {
            findings.extend(type_diff::diff_function_param_noinfer_removed(
                &old_sym_augmented.overloads,
                &new_sym_augmented.overloads,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // BEH-01, BEH-02: behavioral modifier diff (async/generator)
        if old_decl.is_function && new_decl.is_function {
            findings.extend(behavior_diff::compare_behavior(
                &old_sym_augmented,
                &new_sym_augmented,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // TLF-01: construct signature removed (interfaces and classes)
        let is_class_or_interface = (old_decl.is_interface && new_decl.is_interface)
            || (old_decl.is_class && new_decl.is_class);
        if is_class_or_interface
            && (!old_decl.construct_signatures.is_empty()
                || !new_decl.construct_signatures.is_empty())
        {
            findings.extend(interface_diff::compare_construct_signatures(
                &old_decl.construct_signatures,
                &new_decl.construct_signatures,
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // TLF-03: this parameter type changed (functions)
        if old_decl.is_function && new_decl.is_function {
            findings.extend(ref_diff::compare_this_param(
                old_decl.this_param_type.as_deref(),
                new_decl.this_param_type.as_deref(),
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // TLF-04: assertion target widened (functions)
        if old_decl.is_function && new_decl.is_function {
            findings.extend(ref_diff::compare_assertion(
                old_decl.assertion_target.as_deref(),
                new_decl.assertion_target.as_deref(),
                name,
                &new_sym.file,
                new_sym.line,
                new_sym.col,
            ));
        }

        // TYPE-02 (Phase 20): type-alias shape diff.
        // The base `compare_exports_core` diff_types path only fires when
        // `sym.overloads` is populated — which happens only for functions
        // via resolver-side extraction. For `type X = <type>` aliases
        // `overloads` stays empty without LSP, so the (InferExtends,
        // InferExtends) arm and all other shape diffs never fire on
        // type-alias bodies. Here in `compare_structural_diff` we have
        // re-parsed the declaration; `extract_declaration` populates
        // `overloads[0]` with the type-alias RHS. Route that pair through
        // `diff_types` so the structural shape diff runs on type aliases.
        if old_decl.is_type_alias && new_decl.is_type_alias {
            if let (Some(old_shape), Some(new_shape)) =
                (old_decl.overloads.first(), new_decl.overloads.first())
            {
                findings.extend(type_diff::diff_types(
                    old_shape,
                    new_shape,
                    name,
                    &new_sym.file,
                    new_sym.line,
                    new_sym.col,
                ));
            }
        }

        // TLF-07: type-only re-export flag changed
        // Going from a value re-export to type-only re-export (or vice versa) changes
        // the consumer's ability to use the import as a value.
        if old_sym.is_reexport
            && new_sym.is_reexport
            && old_sym.is_type_only != new_sym.is_type_only
        {
            let change = if new_sym.is_type_only {
                "became type-only"
            } else {
                "became value re-export"
            };
            findings.push(Finding {
                severity: Severity::Major,
                rule: subtype::RULE_TYPE_ONLY_REEXPORT_CHANGED,
                file: new_sym.file.clone(),
                line: new_sym.line,
                col: new_sym.col,
                message: format!(
                    "re-export of `{}` {} — consumers relying on the {} may break",
                    name,
                    change,
                    if new_sym.is_type_only {
                        "value"
                    } else {
                        "type-only constraint"
                    },
                ),
            });
        }
    }

    findings
}

/// Produce a synthetic list of findings for a first-publish package (PKG-03).
///
/// All current exports are treated as `export-added` minor findings.
pub fn first_publish_findings(exports: &ExportMap) -> Vec<Finding> {
    let mut findings: Vec<Finding> = exports
        .iter()
        .map(|(key, sym)| {
            // Phase 18: qualify with subpath/condition when non-canonical
            let is_root_no_cond = key.subpath == "." && key.condition.is_none();
            let qualifier = if is_root_no_cond {
                String::new()
            } else {
                format!(
                    " at {}{}",
                    key.subpath,
                    key.condition
                        .as_deref()
                        .map(|c| format!(" ({})", c))
                        .unwrap_or_default()
                )
            };
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: sym.file.clone(),
                line: sym.line,
                col: sym.col,
                message: format!("exported symbol `{}` was added{}", key.name, qualifier),
            }
        })
        .collect();

    // Phase 18 Warning 8: total-order sort on (severity, file, line, col, rule).
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
            .then_with(|| a.rule.cmp(b.rule))
    });

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::tree_query::SourceLanguage;

    #[test]
    fn test_ambient_class_declaration_found() {
        // BUG-13-01: export declare class Foo should be found by extract_declaration
        let source = b"export declare class Foo { bar(): void; }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find ambient class Foo"
        );
        let decl = result.unwrap();
        assert!(decl.is_class, "declaration should be identified as a class");
        assert!(
            decl.class_shape.is_some(),
            "class shape should be extracted"
        );
    }

    #[test]
    fn test_ambient_function_declaration_found() {
        // BUG-13-01: export declare function foo should be found
        let source = b"export declare function foo(x: number): string;";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "foo", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find ambient function foo"
        );
        let decl = result.unwrap();
        assert!(
            decl.is_function,
            "declaration should be identified as a function"
        );
    }

    #[test]
    fn test_ambient_enum_declaration_found() {
        // BUG-13-01: export declare enum Status should be found
        let source = b"export declare enum Status { Active, Inactive }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Status", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find ambient enum Status"
        );
        let decl = result.unwrap();
        assert!(decl.is_enum, "declaration should be identified as an enum");
        assert!(decl.enum_shape.is_some(), "enum shape should be extracted");
    }

    #[test]
    fn test_ambient_interface_declaration_found() {
        // BUG-13-01: export declare interface Opts should be found
        let source = b"export declare interface Opts { debug: boolean; }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Opts", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find ambient interface Opts"
        );
        let decl = result.unwrap();
        assert!(
            decl.is_interface,
            "declaration should be identified as an interface"
        );
    }

    #[test]
    fn test_find_decl_tolerates_line_distance() {
        // GAP-13-05: declaration far from hint_line should still be found
        // Simulates a file move where line numbers shift dramatically
        let source = b"\n\n\n\n\n\n\n\n\n\nclass Foo { bar() {} }"; // Foo at line 11
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
        assert!(
            result.is_some(),
            "should find Foo even when hint_line is 1 but actual line is 11"
        );
    }

    #[test]
    fn test_find_decl_closest_match_with_multiple_same_name() {
        // GAP-13-05: when multiple declarations have the same name (overloads),
        // the closest to hint_row should be picked
        let source = b"function foo(x: string): void;\nfunction foo(x: number): void;\nfunction foo(x: any): void {}";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "foo", 2);
        assert!(result.is_some(), "should find function foo");
        // We can't easily check which overload was picked, but the function should not panic
    }

    #[test]
    fn test_arrow_function_extract() {
        // REQ-14-01: arrow function exports should be found by extract_declaration
        let source = b"export const add = (x: number, y: number): number => x + y;";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "add", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find arrow function 'add'"
        );
        let decl = result.unwrap();
        assert!(
            decl.is_function,
            "arrow function should be identified as a function"
        );
        assert!(!decl.is_async, "non-async arrow should not be async");
    }

    #[test]
    fn test_async_arrow_function_extract() {
        // REQ-14-01: async arrow function export
        let source =
            b"export const fetchData = async (url: string): Promise<string> => { return ''; };";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "fetchData", 1);
        assert!(result.is_some(), "should find async arrow function");
        let decl = result.unwrap();
        assert!(decl.is_function, "async arrow should be a function");
        assert!(decl.is_async, "async arrow should have is_async=true");
    }

    #[test]
    fn test_anonymous_default_function_extract() {
        // REQ-14-02: anonymous default function should be found
        let source = b"export default function(x: number): string { return ''; }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "default", 1);
        assert!(result.is_some(), "should find anonymous default function");
        let decl = result.unwrap();
        assert!(
            decl.is_function,
            "anonymous function should be identified as function"
        );
    }

    #[test]
    fn test_anonymous_default_class_extract() {
        // REQ-14-02: anonymous default class should be found
        let source = b"export default class { foo(): void {} }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "default", 1);
        assert!(result.is_some(), "should find anonymous default class");
        let decl = result.unwrap();
        assert!(
            decl.is_class,
            "anonymous class should be identified as class"
        );
        assert!(
            decl.class_shape.is_some(),
            "class shape should be extracted"
        );
    }

    #[test]
    fn test_function_expression_extract() {
        // REQ-14-01: function expression in variable should be found
        let source =
            b"export const handler = function(req: Request): Response { return new Response(); };";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "handler", 1);
        assert!(
            result.is_some(),
            "should find function expression 'handler'"
        );
        let decl = result.unwrap();
        assert!(
            decl.is_function,
            "function expression should be identified as function"
        );
    }

    #[test]
    fn test_arrow_function_overloads_extracted() {
        // REQ-14-01: Arrow functions should have overloads extracted via build_function_shape_from_node
        let source = b"export const greet = (name: string): string => `Hello ${name}`;";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "greet", 1);
        assert!(result.is_some());
        let decl = result.unwrap();
        assert!(
            !decl.overloads.is_empty(),
            "arrow function should have overload shape extracted"
        );
    }

    #[test]
    fn test_interface_extract_found() {
        // REQ-14-05: interface should be found and classified
        let source = b"export interface Foo { bar(): void; baz: string; }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo", 1);
        assert!(result.is_some());
        let decl = result.unwrap();
        assert!(decl.is_interface);
    }

    #[test]
    fn test_collect_interface_members_merged() {
        // REQ-14-04: two interface declarations with same name should union members
        let source = b"export interface Foo { a(): void; }\nexport interface Foo { b(): void; }";
        let members = collect_interface_members(source, &SourceLanguage::TypeScript, "Foo");
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(
            names.contains(&"a"),
            "member 'a' from first declaration should be present"
        );
        assert!(
            names.contains(&"b"),
            "member 'b' from second declaration should be present"
        );
        assert_eq!(
            members.len(),
            2,
            "should have exactly 2 members from merged declarations"
        );
    }

    #[test]
    fn test_collect_interface_members_dedup() {
        // Same member name in both declarations — first wins
        let source =
            b"export interface Foo { x(): void; }\nexport interface Foo { x(): void; y: number; }";
        let members = collect_interface_members(source, &SourceLanguage::TypeScript, "Foo");
        let x_count = members.iter().filter(|m| m.name == "x").count();
        assert_eq!(x_count, 1, "duplicate member 'x' should appear only once");
    }

    // ─── REQ-14-03: find_decl_in_node dotted name (namespace member) ─────────

    #[test]
    fn test_find_decl_dotted_name_in_namespace() {
        // REQ-14-03: find_decl_in_node should find member inside namespace body
        let source = b"export namespace Foo { export function bar(x: number): void {} }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "Foo.bar", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find 'Foo.bar' inside namespace"
        );
        let decl = result.unwrap();
        assert!(
            decl.is_function,
            "Foo.bar should be identified as a function"
        );
    }

    #[test]
    fn test_find_decl_dotted_name_class_in_namespace() {
        // REQ-14-03: class member inside namespace
        let source = b"export namespace NS { export class MyClass { foo(): void {} } }";
        let result = extract_declaration(source, &SourceLanguage::TypeScript, "NS.MyClass", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find 'NS.MyClass' inside namespace"
        );
        let decl = result.unwrap();
        assert!(decl.is_class, "NS.MyClass should be identified as a class");
    }

    // ─── REQ-15-01: CJS inline structural diff ──────────────────────────────

    #[test]
    fn test_cjs_module_exports_class_extract() {
        // REQ-15-01: module.exports = class Foo { method() {} } should be found as class
        let source = b"module.exports = class Foo { method() {} }";
        let result = extract_declaration(source, &SourceLanguage::JavaScript, "default", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find CJS inline class as 'default'"
        );
        let decl = result.unwrap();
        assert!(
            decl.is_class,
            "CJS inline class should be identified as class"
        );
        let shape = decl
            .class_shape
            .as_ref()
            .expect("class_shape should be extracted");
        assert!(
            shape.members.iter().any(|m| m.name == "method"),
            "class shape should contain 'method'; got: {:?}",
            shape.members.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_module_exports_function_extract() {
        // REQ-15-01: module.exports = function(a, b) {} should be found as function
        let source = b"module.exports = function(a, b) { return a + b; }";
        let result = extract_declaration(source, &SourceLanguage::JavaScript, "default", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find CJS inline function as 'default'"
        );
        let decl = result.unwrap();
        assert!(
            decl.is_function,
            "CJS inline function should be identified as function"
        );
    }

    #[test]
    fn test_cjs_exports_property_function_extract() {
        // REQ-15-01: exports.handler = async (req, res) => {} should be found
        let source = b"exports.handler = async (req, res) => { return 'ok'; }";
        let result = extract_declaration(source, &SourceLanguage::JavaScript, "handler", 1);
        assert!(
            result.is_some(),
            "extract_declaration should find CJS exports.handler arrow function"
        );
        let decl = result.unwrap();
        assert!(
            decl.is_function,
            "CJS exports.handler arrow should be identified as function"
        );
        assert!(decl.is_async, "async arrow should have is_async=true");
    }

    #[test]
    fn test_cjs_inline_class_structural_diff() {
        // REQ-15-01: module.exports = class { method() {} } with method removed
        // should produce class-method-removed finding via compare_structural_diff
        use crate::types::{DeclarationKind, ExportKind, ExportSymbol};

        let tmp = tempfile::tempdir().expect("tempdir");
        let old_dir = tmp.path().join("old");
        let new_dir = tmp.path().join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        std::fs::write(
            old_dir.join("index.js"),
            b"module.exports = class Foo { method() {} }",
        )
        .unwrap();
        std::fs::write(new_dir.join("index.js"), b"module.exports = class Foo { }").unwrap();

        let file = std::path::PathBuf::from("index.js");
        let sym = ExportSymbol {
            name: "default".to_string(),
            kind: ExportKind::Default,
            file: file.clone(),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: vec![DeclarationKind::Class],
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
        };

        let mut old_exports: ExportMap = ExportMap::new();
        old_exports.insert(
            crate::types::ExportKey::new_root_value("default"),
            sym.clone(),
        );
        let mut new_exports: ExportMap = ExportMap::new();
        new_exports.insert(crate::types::ExportKey::new_root_value("default"), sym);

        let findings = compare_structural_diff(&old_dir, &new_dir, &old_exports, &new_exports);
        assert!(
            findings.iter().any(|f| f.rule == "class-method-removed"),
            "removing a method from CJS inline class should produce class-method-removed; got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cjs_inline_function_async_diff() {
        // REQ-15-01: module.exports = function(a, b) {} changing to async function
        // should produce async-to-sync / sync-to-async finding
        use crate::types::{DeclarationKind, ExportKind, ExportSymbol};

        let tmp = tempfile::tempdir().expect("tempdir");
        let old_dir = tmp.path().join("old");
        let new_dir = tmp.path().join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        std::fs::write(
            old_dir.join("index.js"),
            b"module.exports = function(a, b) { return a + b; }",
        )
        .unwrap();
        std::fs::write(
            new_dir.join("index.js"),
            b"module.exports = async function(a, b) { return a + b; }",
        )
        .unwrap();

        let file = std::path::PathBuf::from("index.js");
        let sym = ExportSymbol {
            name: "default".to_string(),
            kind: ExportKind::Default,
            file: file.clone(),
            line: 1,
            col: 1,
            type_signature: None,
            overloads: Vec::new(),
            decl_kinds: vec![DeclarationKind::Function],
            type_guard_type: None,
            is_async: false,
            is_generator: false,
            is_reexport: false,
            is_type_only: false,
            release_tag: None,
            satisfies_type: None,
            compound_members: Vec::new(),
        };

        let mut old_exports: ExportMap = ExportMap::new();
        old_exports.insert(
            crate::types::ExportKey::new_root_value("default"),
            sym.clone(),
        );
        let mut new_exports_map: ExportMap = ExportMap::new();
        new_exports_map.insert(crate::types::ExportKey::new_root_value("default"), sym);

        let findings = compare_structural_diff(&old_dir, &new_dir, &old_exports, &new_exports_map);
        assert!(
            findings.iter().any(|f| f.rule == "sync-to-async"),
            "changing CJS function from sync to async should produce sync-to-async; got: {:?}",
            findings.iter().map(|f| f.rule).collect::<Vec<_>>()
        );
    }

    // ─── Phase 18 compare_exports ExportKey-iteration tests ─────────────────

    fn make_basic_sym(
        name: &str,
        file: std::path::PathBuf,
        line: u32,
    ) -> crate::types::ExportSymbol {
        use crate::types::{ExportKind, ExportSymbol};
        ExportSymbol {
            name: name.to_string(),
            kind: ExportKind::Named,
            file,
            line,
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
        }
    }

    #[test]
    fn test_arch01_different_subpaths_produce_separate_findings() {
        // Phase 18 ARCH-01: same name at different subpaths must fan out to
        // separate export-removed + export-added findings (no silent match).
        use crate::types::{ExportKey, ExportMap, SymbolSpace};
        use std::path::PathBuf;

        let mut old: ExportMap = ExportMap::new();
        let mut new: ExportMap = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "foo"),
            make_basic_sym("foo", PathBuf::from("a.ts"), 1),
        );
        new.insert(
            ExportKey::new("./sub", None, SymbolSpace::Value, "foo"),
            make_basic_sym("foo", PathBuf::from("b.ts"), 1),
        );
        let findings = compare_exports(&old, &new, std::path::Path::new("."));
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == "export-removed")
                .count(),
            1,
            "ARCH-01: exactly one export-removed for `foo` at `.`; got: {:?}",
            findings
        );
        assert_eq!(
            findings.iter().filter(|f| f.rule == "export-added").count(),
            1,
            "ARCH-01: exactly one export-added for `foo` at `./sub`; got: {:?}",
            findings
        );
        // Removed must reference the root `.` subpath in the message (canonical — no qualifier)
        let removed = findings
            .iter()
            .find(|f| f.rule == "export-removed")
            .unwrap();
        assert!(
            !removed.message.contains("./sub"),
            "ARCH-01: removed message for root subpath should not qualify; got: `{}`",
            removed.message
        );
        // Added must reference ./sub in the qualifier
        let added = findings.iter().find(|f| f.rule == "export-added").unwrap();
        assert!(
            added.message.contains("./sub"),
            "ARCH-01: added message must qualify with `./sub`; got: `{}`",
            added.message
        );
    }

    #[test]
    fn test_arch03_type_and_value_space_diff_independently() {
        // Phase 18 ARCH-03: `export type Foo` and `export const Foo` coexist in
        // different SymbolSpace buckets and diff independently.
        use crate::types::{ExportKey, ExportMap, SymbolSpace};
        use std::path::PathBuf;

        let mut old: ExportMap = ExportMap::new();
        let mut new: ExportMap = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Type, "Foo"),
            make_basic_sym("Foo", PathBuf::from("a.ts"), 1),
        );
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
            make_basic_sym("Foo", PathBuf::from("a.ts"), 1),
        );
        // New: only value-space Foo
        new.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
            make_basic_sym("Foo", PathBuf::from("a.ts"), 1),
        );
        let findings = compare_exports(&old, &new, std::path::Path::new("."));
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == "export-removed")
                .count(),
            1,
            "ARCH-03: exactly one export-removed for type-space Foo; got: {:?}",
            findings
        );
        assert_eq!(
            findings.iter().filter(|f| f.rule == "export-added").count(),
            0,
            "ARCH-03: value-space Foo present in both sides must NOT fire export-added; got: {:?}",
            findings
        );
    }

    #[test]
    fn test_arch04_multi_file_same_name_diffed_independently() {
        // Phase 18 ARCH-04: `User` from `./a.ts` at subpath `.` and `User` from
        // `./b.ts` at subpath `./admin` are distinct keys; removing one does not
        // collapse onto the other.
        use crate::types::{ExportKey, ExportMap, SymbolSpace};
        use std::path::PathBuf;

        let mut old: ExportMap = ExportMap::new();
        let mut new: ExportMap = ExportMap::new();
        let user_root_key = ExportKey::new(".", None, SymbolSpace::Value, "User");
        let user_admin_key = ExportKey::new("./admin", None, SymbolSpace::Value, "User");
        old.insert(
            user_root_key.clone(),
            make_basic_sym("User", PathBuf::from("src/a.ts"), 1),
        );
        old.insert(
            user_admin_key.clone(),
            make_basic_sym("User", PathBuf::from("src/b.ts"), 1),
        );
        new.insert(
            user_root_key,
            make_basic_sym("User", PathBuf::from("src/a.ts"), 1),
        );
        // ./admin User removed.
        let findings = compare_exports(&old, &new, std::path::Path::new("."));
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == "export-removed")
                .count(),
            1,
            "ARCH-04: exactly one export-removed for ./admin User; got: {:?}",
            findings
        );
        let removed = findings
            .iter()
            .find(|f| f.rule == "export-removed")
            .unwrap();
        assert_eq!(
            removed.file,
            PathBuf::from("src/b.ts"),
            "ARCH-04: removed finding must point to src/b.ts (admin subpath source); got: {:?}",
            removed
        );
    }

    #[test]
    fn test_warning8_compare_exports_deterministic_total_order() {
        // Phase 18 Warning 8: final sort must be total-order on
        // (severity, file, line, col, rule) so HashMap iteration non-determinism
        // never surfaces as Finding reorder.
        use crate::types::{ExportKey, ExportMap, SymbolSpace};
        use std::path::PathBuf;

        let mut old: ExportMap = ExportMap::new();
        let new: ExportMap = ExportMap::new();
        // 4 distinct ExportKeys, all removed → 4 Major findings.
        for n in ["alpha", "beta", "gamma", "delta"] {
            old.insert(
                ExportKey::new(".", None, SymbolSpace::Value, n),
                make_basic_sym(n, PathBuf::from(format!("{}.ts", n)), 1),
            );
        }

        let run_a = compare_exports(&old, &new, std::path::Path::new("."));
        let run_b = compare_exports(&old, &new, std::path::Path::new("."));

        assert_eq!(
            run_a.len(),
            run_b.len(),
            "Warning 8: two runs must produce equal-length Finding vecs"
        );
        for (a, b) in run_a.iter().zip(run_b.iter()) {
            assert_eq!(a.rule, b.rule, "Warning 8: rule order must be stable");
            assert_eq!(a.severity, b.severity);
            assert_eq!(a.file, b.file);
            assert_eq!(a.line, b.line);
            assert_eq!(a.col, b.col);
            assert_eq!(a.message, b.message);
        }

        // Stronger: findings of the same severity must be sorted by
        // (file, line, col, rule) — this validates the total-order comparator.
        for window in run_a.windows(2) {
            if window[0].severity == window[1].severity {
                let a_tuple = (
                    &window[0].file,
                    window[0].line,
                    window[0].col,
                    window[0].rule,
                );
                let b_tuple = (
                    &window[1].file,
                    window[1].line,
                    window[1].col,
                    window[1].rule,
                );
                assert!(
                    a_tuple <= b_tuple,
                    "Warning 8: same-severity findings must be in total order; {:?} !<= {:?}",
                    a_tuple,
                    b_tuple
                );
            }
        }
    }
}

// ─── SEM-08: normalize_rollup_suffix tests ───────────────────────────────────

#[cfg(test)]
mod sem_08_tests {
    use super::normalize_rollup_suffix;
    use std::collections::HashSet;

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sem_08_base_exported_suffix_normalized() {
        let exp = set(&["Foo"]);
        assert_eq!(normalize_rollup_suffix("Foo$1 | null", &exp), "Foo | null");
    }

    #[test]
    fn sem_08_base_not_exported_suffix_preserved() {
        let exp = set(&["Baz"]);
        assert_eq!(
            normalize_rollup_suffix("Bar$1 | null", &exp),
            "Bar$1 | null"
        );
    }

    #[test]
    fn sem_08_multi_digit_suffix() {
        let exp = set(&["Foo"]);
        assert_eq!(normalize_rollup_suffix("Foo$10", &exp), "Foo");
    }

    #[test]
    fn sem_08_mixed_names() {
        let exp = set(&["Foo"]); // Foo exported, Bar not
        assert_eq!(
            normalize_rollup_suffix("Array<Foo$1> | Record<string, Bar$2>", &exp),
            "Array<Foo> | Record<string, Bar$2>"
        );
    }

    #[test]
    fn sem_08_no_suffix_passthrough() {
        let exp = set(&["Foo"]);
        assert_eq!(normalize_rollup_suffix("Foo | Bar", &exp), "Foo | Bar");
    }

    #[test]
    fn sem_08_no_dollar_fast_path() {
        let exp = set(&["Foo"]);
        assert_eq!(
            normalize_rollup_suffix("string | number", &exp),
            "string | number"
        );
    }

    #[test]
    fn sem_08_dollar_without_digits_passthrough() {
        let exp = set(&["Foo"]);
        assert_eq!(normalize_rollup_suffix("Foo$ | Bar", &exp), "Foo$ | Bar");
    }

    // WR-01 (Phase 17 review): `$N` mid-identifier must NOT split the token.
    // The stripper previously treated `Foo$1Bar` as `Foo` + consumed `$1` +
    // new identifier `Bar`, concatenating two parse tokens into `FooBar`.
    #[test]
    fn sem_08_wr_01_dollar_n_mid_identifier_preserved() {
        let exp = set(&["Foo"]); // Foo is exported
                                 // `Foo$1Bar` is a single JS-valid identifier — must NOT become `FooBar`.
        assert_eq!(
            normalize_rollup_suffix("Foo$1Bar | null", &exp),
            "Foo$1Bar | null"
        );
    }

    #[test]
    fn sem_08_wr_01_dollar_n_mid_identifier_preserved_unexported_base() {
        let exp = set(&["Qux"]); // `price` not exported
        assert_eq!(normalize_rollup_suffix("price$2plus", &exp), "price$2plus");
    }

    #[test]
    fn sem_08_wr_01_dollar_n_mid_identifier_underscore_continuation() {
        let exp = set(&["Foo"]); // exported base
                                 // Underscore as continuation char after `$N` — also mid-identifier, do not split.
        assert_eq!(normalize_rollup_suffix("Foo$1_tail", &exp), "Foo$1_tail");
    }
}
