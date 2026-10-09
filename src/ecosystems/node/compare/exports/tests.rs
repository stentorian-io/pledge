use crate::ecosystems::node::compare::exports::*;

// ─── TDD: CJS/ESM migration and types/runtime mismatch tests ───────

#[test]
fn test_exp08_cjs_to_esm_require_removed() {
    // EXP-08: old has require condition, new drops it -> cjs-to-esm finding
    let old =
        make_pkg(r#"{"exports": {".": {"require": "./index.cjs", "import": "./index.mjs"}}}"#);
    let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    let cjs_to_esm: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CJS_TO_ESM)
        .collect();
    assert_eq!(
        cjs_to_esm.len(),
        1,
        "expected 1 cjs-to-esm finding, got: {:?}",
        findings
    );
    assert_eq!(cjs_to_esm[0].severity, Severity::Major);
    assert!(cjs_to_esm[0].message.contains("."));
}

#[test]
fn test_exp08_no_cjs_to_esm_when_both_conditions_present() {
    // EXP-08 negative: both require and import exist in both versions, targets changed -> NO cjs-to-esm
    let old =
        make_pkg(r#"{"exports": {".": {"require": "./index.cjs", "import": "./index.mjs"}}}"#);
    let new = make_pkg(
        r#"{"exports": {".": {"require": "./dist/index.cjs", "import": "./dist/index.mjs"}}}"#,
    );
    let findings = diff_with_empty_dirs(&old, &new);
    assert!(
        findings.iter().all(|f| f.rule != RULE_CJS_TO_ESM),
        "cjs-to-esm should NOT fire when both conditions still present, got: {:?}",
        findings
    );
}

#[test]
fn test_exp09_esm_to_cjs_import_removed() {
    // EXP-09: old has import condition only, new has only require -> esm-to-cjs finding
    let old = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
    let new = make_pkg(r#"{"exports": {".": {"require": "./index.cjs"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    let esm_to_cjs: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_ESM_TO_CJS)
        .collect();
    assert_eq!(
        esm_to_cjs.len(),
        1,
        "expected 1 esm-to-cjs finding, got: {:?}",
        findings
    );
    assert_eq!(esm_to_cjs[0].severity, Severity::Major);
}

#[test]
fn test_exp10_types_runtime_mismatch_dcts_with_import() {
    // EXP-10: types ->.d.cts (CJS types) but only import condition (ESM) -> mismatch
    let new =
        make_pkg(r#"{"exports": {".": {"types": "./index.d.cts", "import": "./index.mjs"}}}"#);
    let old =
        make_pkg(r#"{"exports": {".": {"types": "./index.d.mts", "import": "./index.mjs"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    let mismatch: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_TYPES_RUNTIME_MISMATCH)
        .collect();
    assert_eq!(
        mismatch.len(),
        1,
        "expected 1 types-runtime-mismatch finding, got: {:?}",
        findings
    );
    assert!(
        diff_with_empty_dirs(&new, &new).is_empty(),
        "an unchanged package is not a breaking change"
    );
    assert_eq!(mismatch[0].severity, Severity::Major);
    assert!(mismatch[0].message.contains(".d.cts"));
}

#[test]
fn test_exp10_no_mismatch_for_format_neutral_dts() {
    // EXP-10 negative: types ->.d.ts (format-neutral) with import -> NO mismatch
    let old = make_pkg(r#"{"exports": {".": {"types": "./index.d.ts", "import": "./index.mjs"}}}"#);
    let new = make_pkg(r#"{"exports": {".": {"types": "./index.d.ts", "import": "./index.mjs"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert!(
        findings
            .iter()
            .all(|f| f.rule != RULE_TYPES_RUNTIME_MISMATCH),
        "types-runtime-mismatch should NOT fire for .d.ts (format-neutral), got: {:?}",
        findings
    );
}

#[test]
fn test_module_format_from_condition_key() {
    assert_eq!(
        format_from_condition_key("require"),
        Some(ModuleFormat::Cjs)
    );
    assert_eq!(format_from_condition_key("import"), Some(ModuleFormat::Esm));
    assert_eq!(format_from_condition_key("default"), None);
    assert_eq!(format_from_condition_key("types"), None);
}

#[test]
fn test_module_format_from_extension() {
    assert_eq!(
        format_from_extension("./index.cjs"),
        Some(ModuleFormat::Cjs)
    );
    assert_eq!(
        format_from_extension("./index.mjs"),
        Some(ModuleFormat::Esm)
    );
    assert_eq!(format_from_extension("./index.js"), None);
}

#[test]
fn test_types_format_from_extension() {
    assert_eq!(
        types_format_from_extension("./index.d.cts"),
        Some(ModuleFormat::Cjs)
    );
    assert_eq!(
        types_format_from_extension("./index.d.mts"),
        Some(ModuleFormat::Esm)
    );
    assert_eq!(types_format_from_extension("./index.d.ts"), None);
}

fn make_pkg(json_str: &str) -> serde_json::Value {
    serde_json::from_str(json_str).unwrap()
}

/// Helper: call compare_exports_diff with empty temp directories as pkg roots.
/// Used for tests that exercise the exports-field-based diffing (not filesystem scanning).
fn diff_with_empty_dirs(old_pkg: &serde_json::Value, new_pkg: &serde_json::Value) -> Vec<Finding> {
    let old_dir = tempfile::tempdir().unwrap();
    let new_dir = tempfile::tempdir().unwrap();
    compare_exports_diff(old_pkg, new_pkg, old_dir.path(), new_dir.path())
}

// ─── Parser tests ─────────────────────────────────────────────────────────

#[test]
fn test_parse_bare_string() {
    let v = serde_json::Value::String("./index.js".to_string());
    let map = parse_exports_map(&v);
    assert_eq!(map.len(), 1);
    assert_eq!(
        map.get(".").unwrap().node,
        ConditionNode::Leaf(Some("./index.js".to_string()))
    );
}

#[test]
fn test_parse_subpath_object() {
    let v = serde_json::json!({".": "./index.js", "./utils": "./utils.js"});
    let map = parse_exports_map(&v);
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get(".").unwrap().node,
        ConditionNode::Leaf(Some("./index.js".to_string()))
    );
    assert_eq!(
        map.get("./utils").unwrap().node,
        ConditionNode::Leaf(Some("./utils.js".to_string()))
    );
}

#[test]
fn test_parse_conditions_only_object() {
    // No keys start with "." — treat as conditions for implicit "." subpath
    let v = serde_json::json!({"import": "./index.mjs", "require": "./index.cjs"});
    let map = parse_exports_map(&v);
    assert_eq!(map.len(), 1);
    let node = &map.get(".").unwrap().node;
    match node {
        ConditionNode::Branch(conds) => {
            assert!(conds.contains_key("import"));
            assert!(conds.contains_key("require"));
        }
        other => panic!("expected Branch, got {:?}", other),
    }
}

#[test]
fn test_parse_nested_conditions() {
    let v = serde_json::json!({
        ".": {
            "import": {
                "types": "./index.d.mts",
                "default": "./index.mjs"
            },
            "require": "./index.cjs"
        }
    });
    let map = parse_exports_map(&v);
    assert_eq!(map.len(), 1);
    let node = &map.get(".").unwrap().node;
    match node {
        ConditionNode::Branch(top) => {
            assert!(top.contains_key("import"));
            match top.get("import").unwrap() {
                ConditionNode::Branch(inner) => {
                    assert!(inner.contains_key("types"));
                    assert!(inner.contains_key("default"));
                }
                other => panic!("expected nested Branch, got {:?}", other),
            }
        }
        other => panic!("expected Branch at top level, got {:?}", other),
    }
}

#[test]
fn test_parse_null_returns_empty() {
    let map = parse_exports_map(&serde_json::Value::Null);
    assert!(map.is_empty());
}

#[test]
fn test_parse_array_takes_first_element() {
    let v = serde_json::json!(["./index.js", "./fallback.js"]);
    let map = parse_exports_map(&v);
    assert_eq!(map.len(), 1);
    assert_eq!(
        map.get(".").unwrap().node,
        ConditionNode::Leaf(Some("./index.js".to_string()))
    );
}

#[test]
fn test_is_subpath_key() {
    assert!(is_subpath_key("."));
    assert!(is_subpath_key("./utils"));
    assert!(is_subpath_key("./*"));
    assert!(!is_subpath_key("import"));
    assert!(!is_subpath_key("require"));
    assert!(!is_subpath_key("types"));
    assert!(!is_subpath_key("default"));
}

// ─── Diff tests: EXP-01..07 ───────────────────────────────────────────────

#[test]
fn test_exp01_subpath_removed() {
    let old = make_pkg(r#"{"exports": {"./utils": "./utils.js", ".": "./index.js"}}"#);
    let new = make_pkg(r#"{"exports": {".": "./index.js"}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
    assert_eq!(findings[0].rule, RULE_SUBPATH_REMOVED);
    assert_eq!(findings[0].severity, Severity::Major);
    assert!(findings[0].message.contains("./utils"));
}

#[test]
fn test_exp02_exports_field_added() {
    let old = make_pkg(r#"{"name": "my-pkg"}"#);
    let new = make_pkg(r#"{"name": "my-pkg", "exports": {".": "./index.js"}}"#);
    // Use empty dirs — no old files, so message uses simpler form
    let findings = diff_with_empty_dirs(&old, &new);
    assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
    assert_eq!(findings[0].rule, RULE_EXPORTS_FIELD_ADDED);
    assert_eq!(findings[0].severity, Severity::Major);
}

#[test]
fn test_exp03_condition_removed() {
    // Removing the `require` condition fires both EXP-03 (condition-removed) and
    // EXP-08 (cjs-to-esm migration). Both are correct co-occurrences.
    let old =
        make_pkg(r#"{"exports": {".": {"import": "./index.mjs", "require": "./index.cjs"}}}"#);
    let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    // EXP-03: condition-removed must fire
    let condition_removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CONDITION_REMOVED)
        .collect();
    assert_eq!(
        condition_removed.len(),
        1,
        "expected 1 condition-removed finding, got: {:?}",
        findings
    );
    assert_eq!(condition_removed[0].severity, Severity::Major);
    assert!(condition_removed[0].message.contains("require"));
    // EXP-08: cjs-to-esm migration also fires for this scenario
    let cjs_to_esm: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_CJS_TO_ESM)
        .collect();
    assert_eq!(
        cjs_to_esm.len(),
        1,
        "expected 1 cjs-to-esm finding, got: {:?}",
        findings
    );
}

#[test]
fn test_exp04_condition_target_changed() {
    let old = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
    let new = make_pkg(r#"{"exports": {".": {"import": "./dist/index.mjs"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
    assert_eq!(findings[0].rule, RULE_CONDITION_TARGET_CHANGED);
    assert_eq!(findings[0].severity, Severity::Minor);
}

#[test]
fn test_exp05_types_condition_removed_no_double_fire() {
    let old = make_pkg(r#"{"exports": {".": {"types": "./index.d.ts", "import": "./index.mjs"}}}"#);
    let new = make_pkg(r#"{"exports": {".": {"import": "./index.mjs"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    // Should fire EXP-05 (types-condition-removed) but NOT EXP-03 (condition-removed)
    assert_eq!(
        findings.len(),
        1,
        "expected exactly 1 finding, got: {:?}",
        findings
    );
    assert_eq!(findings[0].rule, RULE_TYPES_CONDITION_REMOVED);
    assert_eq!(findings[0].severity, Severity::Major);
    // Verify no condition-removed double-fire
    assert!(
        findings.iter().all(|f| f.rule != RULE_CONDITION_REMOVED),
        "EXP-03 should not fire when EXP-05 fires"
    );
}

#[test]
fn test_exp06_wildcard_pattern_changed() {
    let old = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
    let new = make_pkg(r#"{"exports": {".": "./index.js"}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
    assert_eq!(findings[0].rule, RULE_SUBPATH_PATTERN_CHANGED);
    assert_eq!(findings[0].severity, Severity::Major);
    // Verify it did NOT fire subpath-removed for the wildcard
    assert!(
        findings.iter().all(|f| f.rule != RULE_SUBPATH_REMOVED),
        "EXP-01 should not fire for wildcard patterns"
    );
}

#[test]
fn test_exp07_subpath_added() {
    let old = make_pkg(r#"{"exports": {".": "./index.js"}}"#);
    let new = make_pkg(r#"{"exports": {".": "./index.js", "./newpath": "./newpath.js"}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert_eq!(findings.len(), 1, "expected 1 finding, got: {:?}", findings);
    assert_eq!(findings[0].rule, RULE_SUBPATH_ADDED);
    assert_eq!(findings[0].severity, Severity::Minor);
    assert!(findings[0].message.contains("./newpath"));
}

#[test]
fn test_unchanged_exports_no_findings() {
    let old =
        make_pkg(r#"{"exports": {".": {"import": "./index.mjs", "require": "./index.cjs"}}}"#);
    let new =
        make_pkg(r#"{"exports": {".": {"import": "./index.mjs", "require": "./index.cjs"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert!(
        findings.is_empty(),
        "expected no findings for unchanged exports, got: {:?}",
        findings
    );
}

// ─── Wildcard pattern semantic matching ────────────────────────

#[test]
fn wildcard_refinement() {
    // Old has "./*", new has "./*.js" — pattern refinement, single finding
    let old = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
    let new = make_pkg(r#"{"exports": {"./*.js": "./features/*.js", ".": "./index.js"}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    // Verify single subpath-pattern-changed with "changed to"
    let pattern_changed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_SUBPATH_PATTERN_CHANGED)
        .collect();
    assert_eq!(
        pattern_changed.len(),
        1,
        "expected 1 subpath-pattern-changed finding, got: {:?}",
        findings
    );
    assert!(
        pattern_changed[0].message.contains("changed to"),
        "message should contain 'changed to'; got: {}",
        pattern_changed[0].message
    );
    assert!(
        pattern_changed[0].message.contains("./*"),
        "message should mention old pattern; got: {}",
        pattern_changed[0].message
    );
    assert!(
        pattern_changed[0].message.contains("./*.js"),
        "message should mention new pattern; got: {}",
        pattern_changed[0].message
    );
    // Verify NO subpath-removed or subpath-added findings
    assert!(
        findings.iter().all(|f| f.rule != RULE_SUBPATH_REMOVED),
        "subpath-removed should NOT fire for wildcard refinement, got: {:?}",
        findings
    );
    assert!(
        findings.iter().all(|f| f.rule != RULE_SUBPATH_ADDED),
        "subpath-added should NOT fire for wildcard refinement, got: {:?}",
        findings
    );
}

#[test]
fn wildcard_unrelated() {
    // Old has "./*", new has "./utils/*.js" — different base paths, separate removed+added
    let old = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
    let new = make_pkg(r#"{"exports": {"./utils/*.js": "./utils/*.js", ".": "./index.js"}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    let pattern_changed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_SUBPATH_PATTERN_CHANGED)
        .collect();
    let subpath_added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_SUBPATH_ADDED)
        .collect();
    // ./* removed -> subpath-pattern-changed (no replacement found)
    assert_eq!(
        pattern_changed.len(),
        1,
        "expected 1 subpath-pattern-changed for unrelated wildcard removal, got: {:?}",
        findings
    );
    assert!(
        pattern_changed[0].message.contains("removed"),
        "removal message should say 'removed'; got: {}",
        pattern_changed[0].message
    );
    // ./utils/*.js added -> subpath-added
    assert_eq!(
        subpath_added.len(),
        1,
        "expected 1 subpath-added for new unrelated wildcard, got: {:?}",
        findings
    );
}

#[test]
fn wildcard_both_present() {
    // Old and new both have "./*" — no pattern-changed finding (just normal condition diffing)
    let old = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
    let new = make_pkg(r#"{"exports": {"./*": "./features/*.js", ".": "./index.js"}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert!(
        findings
            .iter()
            .all(|f| f.rule != RULE_SUBPATH_PATTERN_CHANGED),
        "subpath-pattern-changed should NOT fire when pattern exists in both, got: {:?}",
        findings
    );
    assert!(
        findings.is_empty(),
        "expected no findings when wildcard unchanged, got: {:?}",
        findings
    );
}

// ─── TDD: Packed output fallback scanner tests ─────────────────────

#[test]
fn test_exp13_importable_file_removed() {
    use std::fs;
    use tempfile::tempdir;

    // Old dir: lib/index.js, lib/utils.js
    let old_dir = tempdir().unwrap();
    let lib_dir = old_dir.path().join("lib");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(lib_dir.join("index.js"), "").unwrap();
    fs::write(lib_dir.join("utils.js"), "").unwrap();

    // New dir: lib/index.js only
    let new_dir = tempdir().unwrap();
    let new_lib_dir = new_dir.path().join("lib");
    fs::create_dir_all(&new_lib_dir).unwrap();
    fs::write(new_lib_dir.join("index.js"), "").unwrap();

    let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let new_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

    let removed: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_IMPORTABLE_FILE_REMOVED)
        .collect();
    assert_eq!(
        removed.len(),
        1,
        "expected 1 importable-file-removed finding, got: {:?}",
        findings
    );
    assert_eq!(removed[0].severity, Severity::Major);
    assert!(
        removed[0].message.contains("utils.js"),
        "message should contain utils.js, got: {}",
        removed[0].message
    );
}

#[test]
fn test_exp13_added_file_produces_subpath_added() {
    use std::fs;
    use tempfile::tempdir;

    // Old dir: lib/index.js only
    let old_dir = tempdir().unwrap();
    let old_lib = old_dir.path().join("lib");
    fs::create_dir_all(&old_lib).unwrap();
    fs::write(old_lib.join("index.js"), "").unwrap();

    // New dir: lib/index.js + lib/new-feature.js
    let new_dir = tempdir().unwrap();
    let new_lib = new_dir.path().join("lib");
    fs::create_dir_all(&new_lib).unwrap();
    fs::write(new_lib.join("index.js"), "").unwrap();
    fs::write(new_lib.join("new-feature.js"), "").unwrap();

    let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let new_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

    let added: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_SUBPATH_ADDED)
        .collect();
    assert_eq!(
        added.len(),
        1,
        "expected 1 subpath-added finding, got: {:?}",
        findings
    );
    assert_eq!(added[0].severity, Severity::Minor);
}

#[test]
fn test_exp13_dotfiles_skipped() {
    use std::fs;
    use tempfile::tempdir;

    let old_dir = tempdir().unwrap();
    fs::write(old_dir.path().join(".hidden"), "").unwrap();
    fs::write(old_dir.path().join("index.js"), "").unwrap();

    let new_dir = tempdir().unwrap();
    fs::write(new_dir.path().join("index.js"), "").unwrap();
    // .hidden removed from new — should NOT produce finding since dotfiles are skipped

    let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let new_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

    // No findings because.hidden is skipped
    assert!(
        findings.is_empty(),
        "expected no findings (dotfiles skipped), got: {:?}",
        findings
    );
}

#[test]
fn test_exp13_only_importable_extensions_included() {
    use std::fs;
    use tempfile::tempdir;

    let old_dir = tempdir().unwrap();
    fs::write(old_dir.path().join("README.md"), "").unwrap();
    fs::write(old_dir.path().join("notes.txt"), "").unwrap();
    fs::write(old_dir.path().join("index.js"), "").unwrap();

    let new_dir = tempdir().unwrap();
    fs::write(new_dir.path().join("index.js"), "").unwrap();
    // README.md and notes.txt removed from new — should NOT produce findings

    let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let new_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

    // No findings because.md and.txt are not importable
    assert!(
        findings.is_empty(),
        "expected no findings (.md/.txt not importable), got: {:?}",
        findings
    );
}

#[test]
fn test_exp02_enhanced_with_file_listing() {
    use std::fs;
    use tempfile::tempdir;

    // Old dir: lib/index.js, lib/utils.js, lib/helper.js
    let old_dir = tempdir().unwrap();
    let old_lib = old_dir.path().join("lib");
    fs::create_dir_all(&old_lib).unwrap();
    fs::write(old_lib.join("index.js"), "").unwrap();
    fs::write(old_lib.join("utils.js"), "").unwrap();
    fs::write(old_lib.join("helper.js"), "").unwrap();

    let new_dir = tempdir().unwrap();

    // Old has no exports, new adds exports
    let old_pkg = make_pkg(r#"{"name": "my-pkg"}"#);
    let new_pkg = make_pkg(r#"{"name": "my-pkg", "exports": {".": "./index.js"}}"#);
    let findings = compare_exports_diff(&old_pkg, &new_pkg, old_dir.path(), new_dir.path());

    let exp02: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == RULE_EXPORTS_FIELD_ADDED)
        .collect();
    assert_eq!(
        exp02.len(),
        1,
        "expected 1 exports-field-added finding, got: {:?}",
        findings
    );
    // Message should include file listing
    assert!(
        exp02[0].message.contains("index.js") || exp02[0].message.contains("encapsulating"),
        "message should mention files, got: {}",
        exp02[0].message
    );
}

#[test]
fn test_scan_importable_files_basic() {
    use std::fs;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    fs::write(dir.path().join("index.js"), "").unwrap();
    fs::write(dir.path().join("types.d.ts"), "").unwrap();
    fs::write(dir.path().join("module.mjs"), "").unwrap();
    fs::write(dir.path().join("README.md"), "").unwrap(); // not importable

    let files = scan_importable_files(dir.path());
    assert!(files.contains("index.js"), "should include index.js");
    assert!(files.contains("types.d.ts"), "should include types.d.ts");
    assert!(files.contains("module.mjs"), "should include module.mjs");
    assert!(!files.contains("README.md"), "should NOT include README.md");
}

// ── SEM-02: Direction-aware null handling in condition targets ────────────

#[test]
fn null_to_string_fires_minor_addition() {
    // "types": null → "types": "./real.d.ts" is additive — Minor (subpath-added)
    let old = make_pkg(r#"{"exports": {".": {"types": null, "default": "./index.js"}}}"#);
    let new = make_pkg(r#"{"exports": {".": {"types": "./real.d.ts", "default": "./index.js"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == RULE_CONDITION_TARGET_CHANGED),
        "SEM-02: null→string should NOT fire condition-target-changed (Major); got: {:?}",
        findings
    );
    // Should fire a Minor finding (subpath-added or similar)
    assert!(
        findings.iter().any(|f| f.severity == Severity::Minor),
        "SEM-02: null→string should produce a Minor finding; got: {:?}",
        findings
    );
}

#[test]
fn string_to_null_fires_major() {
    // "types": "./real.d.ts" → "types": null is breaking — Major (condition-target-changed)
    let old = make_pkg(r#"{"exports": {".": {"types": "./real.d.ts", "default": "./index.js"}}}"#);
    let new = make_pkg(r#"{"exports": {".": {"types": null, "default": "./index.js"}}}"#);
    let findings = diff_with_empty_dirs(&old, &new);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == RULE_CONDITION_TARGET_CHANGED && f.severity == Severity::Major),
        "SEM-02: string→null should fire condition-target-changed Major; got: {:?}",
        findings
    );
}
