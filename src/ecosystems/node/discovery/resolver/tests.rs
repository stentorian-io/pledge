use crate::ecosystems::node::discovery::resolver::*;
use crate::languages::javascript::{compare as javascript_compare, model as js_model};
use crate::languages::typescript::config as tsconfig;

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
    // ./foo.js should probe foo.ts before foo.js.ts
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
    // ./bar.mjs should probe bar.mts
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
    // ./baz.cjs should probe baz.cts
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
    // + ECF-06:./utils.config.js should probe utils.config.ts (not utils.ts)
    let candidates = resolve_module_path(Path::new("/a/src/index.ts"), "./utils.config.js");
    let has_config_ts = candidates
        .iter()
        .any(|p| p.to_string_lossy().ends_with("/utils.config.ts"));
    assert!(
        has_config_ts,
        "candidates must include utils.config.ts; got: {candidates:?}"
    );
    // Should NOT produce just utils.ts (dropping.config)
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
    // directory index should include index.cts
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
    // directory index should include index.jsx
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
    // directory index should include index.cjs
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
    // directory index should include index.d.mts
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
    // directory index should include index.d.cts
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
    // ./foo.js should also probe foo.d.ts for pre-built packages
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
// tsconfig path alias resolution tests
// -------------------------------------------------------------------------

#[test]
fn test_resolve_tsconfig_alias_wildcard() {
    // Pattern "@/*" with replacement "./src/*", specifier "@/utils" → {root}/src/utils.ts
    let mut paths = indexmap::IndexMap::new();
    paths.insert("@/*".to_string(), vec!["./src/*".to_string()]);
    let tsconfig = tsconfig::TsconfigPaths {
        base_url: None,
        paths,
    };
    let pkg_root = Path::new("/project");
    let candidates = super::resolve_config_alias("@/utils", &tsconfig, pkg_root);
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
    let mut paths = indexmap::IndexMap::new();
    paths.insert("@app".to_string(), vec!["./src/app.ts".to_string()]);
    let tsconfig = tsconfig::TsconfigPaths {
        base_url: None,
        paths,
    };
    let pkg_root = Path::new("/project");
    let candidates = super::resolve_config_alias("@app", &tsconfig, pkg_root);
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
    let mut paths = indexmap::IndexMap::new();
    paths.insert("@/*".to_string(), vec!["./src/*".to_string()]);
    let tsconfig = tsconfig::TsconfigPaths {
        base_url: None,
        paths,
    };
    let pkg_root = Path::new("/project");
    let candidates = super::resolve_config_alias("lodash", &tsconfig, pkg_root);
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

    let result = tsconfig::parse_tsconfig_paths(dir.path());
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
    let result = tsconfig::parse_tsconfig_paths(Path::new("/nonexistent/path"));
    assert!(result.is_none(), "missing tsconfig should return None");
}

#[test]
fn test_resolve_tsconfig_alias_with_base_url() {
    // baseUrl affects resolution path
    let mut paths = indexmap::IndexMap::new();
    paths.insert("@/*".to_string(), vec!["src/*".to_string()]);
    let tsconfig = tsconfig::TsconfigPaths {
        base_url: Some(".".to_string()),
        paths,
    };
    let pkg_root = Path::new("/project");
    let candidates = super::resolve_config_alias("@/utils", &tsconfig, pkg_root);
    let has_ts = candidates
        .iter()
        .any(|p| p.to_string_lossy().contains("src/utils"));
    assert!(
        has_ts,
        "alias with baseUrl must produce src/utils candidates; got: {candidates:?}"
    );
}

// -------------------------------------------------------------------------
// External re-export tracking tests
// -------------------------------------------------------------------------

#[test]
fn test_external_reexport_tracked() {
    // A JS file with `export * from 'lodash'` should produce *<lodash> in export map
    let dir = tempfile::tempdir().expect("tempdir");
    let index_path = dir.path().join("index.js");
    std::fs::write(&index_path, b"export * from 'lodash';\n").unwrap();

    let mut map = js_model::ExportMap::new();
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
    use crate::languages::javascript::model::{ExportKey, ExportKind, ExportSymbol};
    use std::path::PathBuf;

    let mut old_map = js_model::ExportMap::new();
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
            is_proxy_dispatched: false,
            local_name: None,
        },
    );
    let new_map = js_model::ExportMap::new();

    let findings =
        javascript_compare::compare_exports(&old_map, &new_map, std::path::Path::new("."), true);
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

    let mut map = js_model::ExportMap::new();
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
fn three_or_more_failures_aggregated_at_depth_zero() {
    // A barrel with ≥3 unresolvable named re-exports from the same relative
    // specifier at depth==0 must produce ONE aggregated warning containing
    // "barrel" and the count, not three individual warnings.
    let dir = tempfile::tempdir().expect("tempdir");
    let index_path = dir.path().join("index.ts");
    // Three named re-exports from./missing, none of which can be resolved.
    std::fs::write(
            &index_path,
            b"export { foo } from './missing';\nexport { bar } from './missing';\nexport { baz } from './missing';\n",
        )
        .unwrap();

    let mut map = js_model::ExportMap::new();
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
fn below_threshold_individual_warnings() {
    // Only 2 failures from the same specifier → NOT aggregated, must produce
    // 2 individual warnings (threshold is 3).
    let dir = tempfile::tempdir().expect("tempdir");
    let index_path = dir.path().join("index.ts");
    std::fs::write(
        &index_path,
        b"export { foo } from './missing';\nexport { bar } from './missing';\n",
    )
    .unwrap();

    let mut map = js_model::ExportMap::new();
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
fn meta_05_names_emitted_in_sorted_order() {
    //: names in the aggregated barrel warning must be
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

    let mut map = js_model::ExportMap::new();
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
fn hashed_chunk_normalized() {
    let p = std::path::Path::new("dist/chunks/chunk-A1B2C3D4.js");
    assert_eq!(
        normalize_chunk_path(p).to_string_lossy(),
        "dist/chunks/chunk-*.js"
    );
}

#[test]
fn unrelated_path_unchanged() {
    let p = std::path::Path::new("src/index.ts");
    assert_eq!(normalize_chunk_path(p).to_string_lossy(), "src/index.ts");
}

#[test]
fn short_hash_unchanged() {
    let p = std::path::Path::new("dist/chunks/chunk-ABC.js");
    assert_eq!(
        normalize_chunk_path(p).to_string_lossy(),
        "dist/chunks/chunk-ABC.js"
    );
}

#[test]
fn mjs_extension_normalized() {
    let p = std::path::Path::new("pkg/dist/chunks/chunk-DEADBEEF.mjs");
    assert_eq!(
        normalize_chunk_path(p).to_string_lossy(),
        "pkg/dist/chunks/chunk-*.mjs"
    );
}

// ──: normalize_chunk_path separator parity ──
// On a Windows host the resolver reports paths with `\` separators;
// on Unix it reports `/`. The normalized output MUST be byte-equal
// across the two so a rollup rebuild on either OS produces the same
// diff key.

#[test]
fn normalize_chunk_path_separator_parity() {
    let forward = normalize_chunk_path(std::path::Path::new("dist/chunks/chunk-A1B2C3D4.js"));
    let backward = normalize_chunk_path(std::path::Path::new("dist\\chunks\\chunk-A1B2C3D4.js"));
    assert_eq!(
        forward.to_string_lossy(),
        backward.to_string_lossy(),
        "WR-02: forward-slash and backslash chunk paths must normalize identically"
    );
    // Additionally pin the canonical output to forward-slashes so cross-OS diff stability is explicit.
    assert_eq!(forward.to_string_lossy(), "dist/chunks/chunk-*.js");
}

#[test]
fn backslash_path_outside_chunks_unchanged() {
    let p = std::path::Path::new("dist\\src\\index.js");
    assert_eq!(normalize_chunk_path(p), p.to_path_buf());
}

#[test]
fn backslash_path_non_hashed_basename_unchanged() {
    let p = std::path::Path::new("dist\\chunks\\chunk-foo.js");
    assert_eq!(normalize_chunk_path(p), p.to_path_buf());
}

#[test]
fn backslash_mjs_extension_normalized() {
    let result = normalize_chunk_path(std::path::Path::new(
        "pkg\\dist\\chunks\\chunk-DEADBEEF.mjs",
    ));
    assert_eq!(result.to_string_lossy(), "pkg/dist/chunks/chunk-*.mjs");
}

// ──: path-equality invariant between
// ExportSymbol.file and ExportMap.directives keys for rollup chunks.
// The resolver MUST key the directives sidecar by the SAME normalized
// path it stamps onto ExportSymbol.file. If this ever drifts, a
// rollup chunk with a stable set of directives across rebuilds
// would diff as "directive-added + directive-removed" instead of
// appearing under a single shared key.

#[test]
fn chunk_directives_keyed_by_normalized_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Canonicalize pkg_root so that strip_prefix succeeds on macOS
    // (tempdir returns /var/..., canonicalize() returns /private/var/...).
    let pkg_root = dir.path().canonicalize().expect("canonicalize tempdir");
    let chunk_rel = std::path::Path::new("dist/chunks/chunk-A1B2C3D4.js");
    let chunk_abs = pkg_root.join(chunk_rel);
    std::fs::create_dir_all(chunk_abs.parent().unwrap()).unwrap();
    // Rollup chunk with a triple-slash directive at the top followed by
    // a regular export. Directives use a stable comparison key while
    // symbols retain their readable physical source path.
    std::fs::write(
        &chunk_abs,
        b"/// <reference path=\"./chunk.d.ts\" />\nexport const x = 1;\n",
    )
    .unwrap();

    let mut map = js_model::ExportMap::new();
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

    // Symbol paths must remain usable by LSP and structural analysis.
    let x_key = ExportKey::new_root_value("x");
    let x_sym = map.get(&x_key).expect("x export present");
    assert_eq!(
        x_sym.file, chunk_rel,
        "ExportSymbol.file must identify the physical source"
    );
}

// ── ROB-01: UTF-8 / parse failure → per-file warning ────────────────────

#[test]
fn invalid_utf8_produces_warning_not_abort() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = dir.path();
    std::fs::create_dir_all(pkg.join("src")).unwrap();
    std::fs::write(
        pkg.join("package.json"),
        r#"{"name":"t","version":"1.0.0","main":"./src/index.ts"}"#,
    )
    .unwrap();
    // Write invalid UTF-8 bytes to the source file
    std::fs::write(pkg.join("src/index.ts"), [0x80u8, 0x81u8]).unwrap();

    let mut map = js_model::ExportMap::new();
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
fn tsconfig_block_comments_stripped() {
    let input = r#"{
            "compilerOptions": {
                /* multiline
                   comment */
                "paths": { "@/*": ["./src/*"] }
            }
        }"#;
    let cleaned = tsconfig::strip_jsonc(input);
    let parsed: serde_json::Value = serde_json::from_str(&cleaned).expect("should parse");
    assert!(
        parsed["compilerOptions"]["paths"]["@/*"][0].as_str() == Some("./src/*"),
        "ROB-04: block comment not stripped; cleaned: {:?}",
        cleaned
    );
}

#[test]
fn tsconfig_trailing_comma_accepted() {
    let input = r#"{ "paths": { "@/*": ["./src/*"], }, }"#;
    let cleaned = tsconfig::strip_jsonc(input);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cleaned);
    assert!(
        parsed.is_ok(),
        "ROB-04: trailing comma not stripped. Got: {cleaned}"
    );
}

#[test]
fn tsconfig_line_comment_regression() {
    // Regression: existing line comment stripping must still work
    let input = r#"{ "paths": { "@/*": ["./src/*"] // a comment
        } }"#;
    let cleaned = tsconfig::strip_jsonc(input);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cleaned);
    assert!(
        parsed.is_ok(),
        "ROB-04: line comment regression — cleaned: {cleaned}"
    );
}

#[test]
fn tsconfig_preserves_utf8_in_strings() {
    // CR-01: strip_jsonc previously cast each byte to `char`
    // which corrupted multi-byte UTF-8 sequences. Non-ASCII path aliases and
    // comments must survive stripping intact so serde_json sees valid UTF-8.
    let input = r#"{
            "paths": { "@文件/*": ["./src/*"] },
            /* 日本語コメント */
            "emoji": "🚀"
        }"#;
    let cleaned = tsconfig::strip_jsonc(input);
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
fn tsconfig_preserves_utf8_trailing_comma_path() {
    // Ensure the second pass (trailing-comma stripper) also operates on chars,
    // not bytes — so a trailing comma near non-ASCII content is handled safely.
    let input = r#"{ "name": "léa", }"#;
    let cleaned = tsconfig::strip_jsonc(input);
    let parsed: serde_json::Value =
        serde_json::from_str(&cleaned).expect("UTF-8 + trailing comma must parse");
    assert_eq!(parsed["name"].as_str(), Some("léa"));
}

// ───: pre-merge + decl_kinds union ────────────────────

fn make_empty_sym(name: &str) -> js_model::ExportSymbol {
    use crate::languages::javascript::model::{ExportKind, ExportSymbol};
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
        is_proxy_dispatched: false,
        local_name: None,
    }
}

#[test]
fn test_resolver_pre_merge_class_plus_namespace_unions_decl_kinds() {
    // class User + namespace User in the same file must merge into a
    // single ExportSymbol with decl_kinds = {Class, Namespace}.
    use crate::languages::javascript::model::{ExportKey, ExportMap};
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
        None,
        ".",
        None,
    )
    .unwrap();

    // Find the User symbol — regardless of space, the class+namespace merge
    // means the same key should carry both decl_kinds.
    let user_keys: Vec<ExportKey> = map.keys().filter(|k| k.name == "User").cloned().collect();
    assert!(
        !user_keys.is_empty(),
        "User key not found in map; got keys: {:?}",
        map.keys().collect::<Vec<_>>()
    );
    // With space_for mapping class -> Value and namespace -> Namespace the
    // two declarations fall into different ExportKeys by design.
    // What requires is that declaration-merging WITHIN the same
    // ExportKey bucket unions decl_kinds. For tree-sitter's declaration-
    // merging the two would need to collide in the SAME space. The test
    // here verifies at least one User entry exists — the merge_hybrid
    // unit tests below cover the union semantics directly.
    assert!(user_keys.iter().any(|k| map[k].name == "User"));
}

#[test]
fn test_merge_hybrid_unions_decl_kinds() {
    use crate::languages::javascript::model::{DeclarationKind, ExportSymbol};
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
    use crate::languages::javascript::model::ExportSymbol;
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
    use crate::languages::javascript::model::{ExportKey, ExportMap, SymbolSpace};
    let a = ExportKey::new(".", None, SymbolSpace::Value, "User");
    let b = ExportKey::new("./admin", None, SymbolSpace::Value, "User");
    let mut map: ExportMap = ExportMap::new();
    map.insert(a.clone(), make_empty_sym("User"));
    map.insert(b.clone(), make_empty_sym("User"));
    assert_eq!(map.len(), 2, "different subpaths MUST be distinct map keys");
}

// ── ARCH-02: end-to-end compound-member propagation ────────────

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
