//! IDN-01 (Phase 18): Rename detection via alias-trailer-only signal.
//!
//! Per D-05 (strict alias-only): fires ONLY when the new-side source contains
//! the transparent-rename pattern `export { NewName as OldName, NewName };`
//! (alias trailer with back-compat alias AND the plain new-name specifier).
//! This yields zero false positives by construction.
//!
//! Per D-07: detected renames are Minor severity — the back-compat alias
//! preserves consumer imports at the old name, so no break occurs.
//!
//! Per D-08: coalescing happens WITHIN a single (subpath, condition, space)
//! bucket. Named exports only — defaults and re-exports stay on the regular
//! add/remove path.
//!
//! Warning 9: suppression of the base `export-removed` / `export-added`
//! findings is guarded by explicit rule identity AND structural
//! (file, line, col) position match against the symbols participating in a
//! coalesced pair (see WR-01 fix). Future rules with similar message shapes
//! cannot be accidentally swallowed by the filter.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::compare::subtype::RULE_EXPORT_RENAMED;
use crate::discover::tree_query::{language_for_file, SourceLanguage};
use crate::types::{ExportKey, ExportMap, Finding, Severity};

use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

/// IDN-01 post-pass: detect transparent renames via alias-trailer pattern in
/// the new-side source, coalesce paired removed/added findings into a single
/// `export-renamed` Minor finding.
///
/// # Arguments
/// - `old`, `new`: the compared ExportMaps (for looking up symbol file paths
///   during suppression).
/// - `base_findings`: the findings vector produced by `compare_exports_core`
///   (no sort, no prior post-pass).
/// - `pkg_root`: package root for resolving ExportSymbol relative file paths.
///
/// # Returns
/// A new findings vector: base findings with the coalesced pairs suppressed,
/// plus one `export-renamed` finding per detected rename.
#[allow(dead_code)]
pub fn coalesce_renames(
    old: &ExportMap,
    new: &ExportMap,
    base_findings: Vec<Finding>,
    pkg_root: &Path,
) -> Vec<Finding> {
    // old_only / new_only: keys present on only one side, candidates for rename.
    let old_only: Vec<&ExportKey> = old.keys().filter(|k| !new.contains_key(*k)).collect();
    let new_only: Vec<&ExportKey> = new.keys().filter(|k| !old.contains_key(*k)).collect();

    // Each coalesced rename: (old_key, new_key, file_for_new_finding, line, col).
    let mut coalesced: Vec<(ExportKey, ExportKey, std::path::PathBuf, u32, u32)> = Vec::new();
    let mut used_old: std::collections::HashSet<ExportKey> = std::collections::HashSet::new();
    let mut used_new: std::collections::HashSet<ExportKey> = std::collections::HashSet::new();

    for old_key in &old_only {
        // D-08: named exports only. Default exports are positional, not nominal.
        if old_key.name == "default" {
            continue;
        }
        if used_old.contains(*old_key) {
            continue;
        }

        for new_key in &new_only {
            if used_new.contains(*new_key) {
                continue;
            }
            // D-08: coalesce only within the same (subpath, condition, space) bucket.
            if (old_key.subpath != new_key.subpath)
                || (old_key.condition != new_key.condition)
                || (old_key.space != new_key.space)
            {
                continue;
            }
            if new_key.name == "default" {
                continue;
            }

            let new_sym = match new.get(*new_key) {
                Some(s) => s,
                None => continue,
            };
            // Resolve relative file paths against pkg_root for IDN-01 source-scan.
            let abs_path = if new_sym.file.is_absolute() {
                new_sym.file.clone()
            } else {
                pkg_root.join(&new_sym.file)
            };

            if source_has_alias_trailer(&abs_path, &new_key.name, &old_key.name) {
                coalesced.push((
                    (*old_key).clone(),
                    (*new_key).clone(),
                    new_sym.file.clone(),
                    new_sym.line,
                    new_sym.col,
                ));
                used_old.insert((*old_key).clone());
                used_new.insert((*new_key).clone());
                break;
            }
        }
    }

    // WR-01 fix: Structural (file, line, col) HashSet for suppression. Eliminates
    // the fragile message-substring dependency — a Finding is suppressed iff its
    // exact source position matches that of an `old_sym` (for removals) or an
    // `new_sym` (for additions) participating in a coalesced pair.
    //
    // Warning 9: still rule-type-guarded. Only `export-removed` and
    // `export-added` findings are eligible for suppression by this post-pass.
    let suppress_removed_positions: HashSet<(PathBuf, u32, u32)> = coalesced
        .iter()
        .filter_map(|(old_key, _, _, _, _)| {
            old.get(old_key).map(|s| (s.file.clone(), s.line, s.col))
        })
        .collect();
    let suppress_added_positions: HashSet<(PathBuf, u32, u32)> = coalesced
        .iter()
        .map(|(_, _, new_file, line, col)| (new_file.clone(), *line, *col))
        .collect();

    let mut result: Vec<Finding> = base_findings
        .into_iter()
        .filter(|f| {
            let is_suppressed_removal = f.rule == "export-removed"
                && suppress_removed_positions.contains(&(f.file.clone(), f.line, f.col));
            let is_suppressed_addition = f.rule == "export-added"
                && suppress_added_positions.contains(&(f.file.clone(), f.line, f.col));
            !(is_suppressed_removal || is_suppressed_addition)
        })
        .collect();

    // Emit one Minor `export-renamed` finding per coalesced pair.
    for (old_key, new_key, file, line, col) in coalesced {
        result.push(Finding {
            severity: Severity::Minor,
            rule: RULE_EXPORT_RENAMED,
            file,
            line,
            col,
            message: format!(
                "export `{}` renamed to `{}` (alias trailer preserves back-compat)",
                old_key.name, new_key.name
            ),
        });
    }

    result
}

/// Parse `file` and return true iff it contains BOTH:
///   1. An aliased specifier `NewName as OldName` (local=new_name, alias=old_name), AND
///   2. A plain (non-aliased) specifier naming `NewName` in an `export { ... }` clause.
///
/// Returns false on any parse failure, invalid UTF-8, unknown extension, or
/// missing file — gracefully degrading per Phase 17 key decision.
fn source_has_alias_trailer(file: &Path, new_name: &str, old_name: &str) -> bool {
    let Some(lang) = language_for_file(file) else {
        return false;
    };
    let Ok(source) = std::fs::read(file) else {
        return false;
    };
    if std::str::from_utf8(&source).is_err() {
        return false;
    }

    let language: tree_sitter::Language = match &lang {
        SourceLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        SourceLanguage::TypeScriptTsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    };
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return false;
    }
    let Some(tree) = parser.parse(&source, None) else {
        return false;
    };
    let root = tree.root_node();

    // Query 1: aliased specifier where local=new_name AND alias=old_name.
    let alias_query = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @local_name
      alias: (identifier) @alias_name)))
"#;
    let Ok(q1) = Query::new(&language, alias_query) else {
        return false;
    };
    let mut alias_found = false;
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q1, root, source.as_slice());
        while let Some(m) = matches.next() {
            let mut local: Option<&str> = None;
            let mut alias: Option<&str> = None;
            for cap in m.captures {
                let cap_name = q1.capture_names()[cap.index as usize];
                let text = cap.node.utf8_text(&source).ok();
                match cap_name {
                    "local_name" => local = text,
                    "alias_name" => alias = text,
                    _ => {}
                }
            }
            if local == Some(new_name) && alias == Some(old_name) {
                alias_found = true;
                break;
            }
        }
    }
    if !alias_found {
        return false;
    }

    // Query 2: plain (non-aliased) specifier with name = new_name.
    // The grammar exposes aliased specifiers as export_specifier with an
    // `alias` field child; plain specifiers have no `alias` child.
    let plain_query = r#"
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @name) @spec))
"#;
    let Ok(q2) = Query::new(&language, plain_query) else {
        return false;
    };
    let mut plain_found = false;
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q2, root, source.as_slice());
        while let Some(m) = matches.next() {
            let mut name: Option<&str> = None;
            let mut spec_node: Option<tree_sitter::Node> = None;
            for cap in m.captures {
                let cap_name = q2.capture_names()[cap.index as usize];
                match cap_name {
                    "name" => name = cap.node.utf8_text(&source).ok(),
                    "spec" => spec_node = Some(cap.node),
                    _ => {}
                }
            }
            if let (Some(n), Some(node)) = (name, spec_node) {
                if n == new_name && node.child_by_field_name("alias").is_none() {
                    plain_found = true;
                    break;
                }
            }
        }
    }

    plain_found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExportKey, ExportKind, ExportSymbol, SymbolSpace};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_sym(name: &str, file: PathBuf, line: u32) -> ExportSymbol {
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
            typeof_import_refs: Vec::new(),
            is_proxy_dispatched: false,
        }
    }

    // Test 1: strict alias-trailer coalesces removed+added into Minor renamed.
    #[test]
    fn test_idn01_alias_trailer_coalesces_to_minor_renamed() {
        let dir = TempDir::new().unwrap();
        let file_rel = PathBuf::from("x.ts");
        std::fs::write(
            dir.path().join(&file_rel),
            r#"
const NewName = 1;
export { NewName as OldName, NewName };
"#,
        )
        .unwrap();

        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "OldName"),
            make_sym("OldName", file_rel.clone(), 1),
        );
        new.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "NewName"),
            make_sym("NewName", file_rel, 2),
        );

        let base = vec![
            Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: PathBuf::from("x.ts"),
                line: 1,
                col: 1,
                message: "exported symbol `OldName` was removed".into(),
            },
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: PathBuf::from("x.ts"),
                line: 2,
                col: 1,
                message: "exported symbol `NewName` was added".into(),
            },
        ];
        let out = coalesce_renames(&old, &new, base, dir.path());
        assert_eq!(
            out.iter().filter(|f| f.rule == RULE_EXPORT_RENAMED).count(),
            1,
            "exactly one export-renamed expected; got: {:?}",
            out
        );
        assert_eq!(
            out.iter().filter(|f| f.rule == "export-removed").count(),
            0,
            "base export-removed must be suppressed; got: {:?}",
            out
        );
        assert_eq!(
            out.iter().filter(|f| f.rule == "export-added").count(),
            0,
            "base export-added must be suppressed; got: {:?}",
            out
        );
        let renamed = out.iter().find(|f| f.rule == RULE_EXPORT_RENAMED).unwrap();
        assert_eq!(renamed.severity, Severity::Minor);
        assert!(renamed.message.contains("OldName"));
        assert!(renamed.message.contains("NewName"));
    }

    // Test 2: no alias trailer → zero coalesce (pure removal + addition preserved).
    #[test]
    fn test_idn01_no_trailer_no_coalesce() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("x.ts"), "const Bar = 1;\nexport { Bar };\n").unwrap();
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "Foo"),
            make_sym("Foo", PathBuf::from("x.ts"), 1),
        );
        new.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "Bar"),
            make_sym("Bar", PathBuf::from("x.ts"), 2),
        );
        let base = vec![
            Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: PathBuf::from("x.ts"),
                line: 1,
                col: 1,
                message: "exported symbol `Foo` was removed".into(),
            },
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: PathBuf::from("x.ts"),
                line: 2,
                col: 1,
                message: "exported symbol `Bar` was added".into(),
            },
        ];
        let out = coalesce_renames(&old, &new, base, dir.path());
        assert_eq!(
            out.iter().filter(|f| f.rule == RULE_EXPORT_RENAMED).count(),
            0
        );
        assert_eq!(out.iter().filter(|f| f.rule == "export-removed").count(), 1);
        assert_eq!(out.iter().filter(|f| f.rule == "export-added").count(), 1);
    }

    // Test 3: alias without back-compat plain specifier → zero coalesce.
    // D-05 strict requires BOTH `NewName as OldName` AND plain `NewName`.
    #[test]
    fn test_idn01_alias_without_plain_name_no_coalesce() {
        let dir = TempDir::new().unwrap();
        let file_rel = PathBuf::from("x.ts");
        // Only the aliased specifier — no plain `NewName` specifier.
        std::fs::write(
            dir.path().join(&file_rel),
            "const NewName = 1;\nexport { NewName as OldName };\n",
        )
        .unwrap();

        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "OldName"),
            make_sym("OldName", file_rel.clone(), 1),
        );
        new.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "NewName"),
            make_sym("NewName", file_rel, 2),
        );
        let base = vec![];
        let out = coalesce_renames(&old, &new, base, dir.path());
        assert_eq!(
            out.iter().filter(|f| f.rule == RULE_EXPORT_RENAMED).count(),
            0,
            "strict D-05: no plain NewName specifier → no coalesce"
        );
    }

    // Test 4: different buckets (subpath differs) → zero coalesce.
    #[test]
    fn test_idn01_different_buckets_no_coalesce() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("x.ts"),
            r#"
const NewName = 1;
export { NewName as OldName, NewName };
"#,
        )
        .unwrap();
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "OldName"),
            make_sym("OldName", PathBuf::from("x.ts"), 1),
        );
        new.insert(
            ExportKey::new("./sub", None, SymbolSpace::Value, "NewName"),
            make_sym("NewName", PathBuf::from("x.ts"), 2),
        );
        let base = vec![];
        let out = coalesce_renames(&old, &new, base, dir.path());
        assert_eq!(
            out.iter().filter(|f| f.rule == RULE_EXPORT_RENAMED).count(),
            0,
            "D-08: cross-bucket coalesce forbidden"
        );
    }

    // Test 5: default exports are excluded from rename coalescing (D-08).
    #[test]
    fn test_idn01_default_exports_not_coalesced() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("x.ts"), "export default 1;\n").unwrap();
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "default"),
            make_sym("default", PathBuf::from("x.ts"), 1),
        );
        new.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "other_default"),
            make_sym("other_default", PathBuf::from("x.ts"), 1),
        );
        let base = vec![];
        let out = coalesce_renames(&old, &new, base, dir.path());
        assert_eq!(
            out.iter().filter(|f| f.rule == RULE_EXPORT_RENAMED).count(),
            0
        );
    }

    // Test 6 (Warning 9): a Finding with a different rule whose message happens
    // to mention `OldName` must NOT be suppressed — the rule-type guard
    // `f.rule == "export-removed"` prevents cross-contamination.
    #[test]
    fn test_idn01_warning9_rule_type_guard_prevents_cross_contamination() {
        let dir = TempDir::new().unwrap();
        let file_rel = PathBuf::from("x.ts");
        std::fs::write(
            dir.path().join(&file_rel),
            r#"
const NewName = 1;
export { NewName as OldName, NewName };
"#,
        )
        .unwrap();
        let mut old = ExportMap::new();
        let mut new = ExportMap::new();
        old.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "OldName"),
            make_sym("OldName", file_rel.clone(), 1),
        );
        new.insert(
            ExportKey::new(".", None, SymbolSpace::Value, "NewName"),
            make_sym("NewName", file_rel.clone(), 2),
        );

        let base = vec![
            Finding {
                severity: Severity::Major,
                rule: "export-removed",
                file: PathBuf::from("x.ts"),
                line: 1,
                col: 1,
                message: "exported symbol `OldName` was removed".into(),
            },
            Finding {
                severity: Severity::Minor,
                rule: "export-added",
                file: PathBuf::from("x.ts"),
                line: 2,
                col: 1,
                message: "exported symbol `NewName` was added".into(),
            },
            // Hypothetical future rule that happens to mention OldName in its
            // message. Must survive the rename coalesce because its rule is
            // NOT export-removed/export-added.
            Finding {
                severity: Severity::Warning,
                rule: "hypothetical-future-rule",
                file: PathBuf::from("x.ts"),
                line: 1,
                col: 1,
                message: "observation about `OldName` that is not a removal".into(),
            },
        ];
        let out = coalesce_renames(&old, &new, base, dir.path());
        assert!(
            out.iter().any(|f| f.rule == "hypothetical-future-rule"),
            "Warning 9: rule-type guard must preserve non-export-rule findings"
        );
        assert_eq!(
            out.iter().filter(|f| f.rule == RULE_EXPORT_RENAMED).count(),
            1
        );
        assert_eq!(
            out.iter().filter(|f| f.rule == "export-removed").count(),
            0,
            "export-removed matching the rename must still be suppressed"
        );
        assert_eq!(
            out.iter().filter(|f| f.rule == "export-added").count(),
            0,
            "export-added matching the rename must still be suppressed"
        );
    }
}
