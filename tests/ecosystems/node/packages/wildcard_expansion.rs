//! Wildcard expansion compatibility checks.

use pledge::ecosystems::node::discovery::entry_points::resolve_entry_points;

#[test]
fn subpath_wildcard_expansion() {
    let pkg_dir = tempfile::tempdir().unwrap();

    // Create dist directory with JS files
    std::fs::create_dir_all(pkg_dir.path().join("dist")).unwrap();
    std::fs::write(pkg_dir.path().join("dist/alpha.js"), b"// alpha").unwrap();
    std::fs::write(pkg_dir.path().join("dist/beta.js"), b"// beta").unwrap();

    let pkg = serde_json::json!({
        "name": "mypkg",
        "version": "1.0.0",
        "exports": {
            "./*": "./dist/*.js"
        }
    });
    std::fs::write(
        pkg_dir.path().join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();

    let entry_points = resolve_entry_points(pkg_dir.path()).unwrap();

    let paths: Vec<String> = entry_points
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        paths.contains(&"alpha.js".to_string()),
        "subpath wildcard ./* should expand to include alpha.js; got: {:?}",
        paths
    );
    assert!(
        paths.contains(&"beta.js".to_string()),
        "subpath wildcard ./* should expand to include beta.js; got: {:?}",
        paths
    );
}

#[test]
fn wildcard_targets_preserve_repeated_slots_and_packed_dotfiles() {
    use pledge::core::findings::Severity;
    use pledge::ecosystems::node::{
        compare::compare_exports_map_diff, discovery::discover_exports,
    };
    use pledge::languages::javascript::compare::compare_exports;

    for (target, file, subpath) in [
        ("./*/index-*.js", "feature/index-feature.js", "./feature"),
        ("./*/index-*.js", "café/index-café.js", "./café"),
        (
            "./*/index-*-*.js",
            "feature/index-feature-feature.js",
            "./feature",
        ),
        ("./star%2A*.js", "star*feature.js", "./feature"),
        ("./*.js?cache=1", "feature.js", "./feature"),
        ("./*.js", ".feature.js", "./.feature"),
        ("./*.js", ".internal/feature.js", "./.internal/feature"),
    ] {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        let compatible = tempfile::tempdir().unwrap();
        for (root, source) in [
            (old.path(), "exports.answer = 42;"),
            (new.path(), "exports.changed = 42;"),
            (compatible.path(), "exports.answer = 42; exports.extra = 1;"),
        ] {
            std::fs::write(
                root.join("package.json"),
                serde_json::json!({"name":"wildcard-probe", "exports":{"./*":target}}).to_string(),
            )
            .unwrap();
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }

        let (before, warnings) = discover_exports(old.path()).unwrap();
        assert!(warnings.is_empty(), "{target}: {warnings:?}");
        assert!(before
            .keys()
            .any(|key| key.name == "answer" && key.subpath == subpath));
        for (root, breaking) in [(new.path(), true), (compatible.path(), false)] {
            let (after, warnings) = discover_exports(root).unwrap();
            assert!(warnings.is_empty(), "{target}: {warnings:?}");
            let changes = compare_exports(&before, &after, root, false);
            assert_eq!(
                changes
                    .iter()
                    .any(|finding| finding.severity == Severity::Major),
                breaking,
                "{file}: {changes:?}"
            );
        }

        #[cfg(feature = "lsp-tests")]
        for (root, succeeds) in [
            (old.path(), true),
            (new.path(), false),
            (compatible.path(), true),
        ] {
            let specifier = format!("wildcard-probe/{}", subpath.trim_start_matches("./"));
            let consumer = format!(
                "require('node:assert/strict').equal(require({}).answer, 42)",
                serde_json::to_string(&specifier).unwrap()
            );
            let output = std::process::Command::new("node")
                .args(["--eval", &consumer])
                .current_dir(root)
                .output()
                .unwrap();
            assert_eq!(output.status.success(), succeeds, "{file}: {output:?}");
        }

        std::fs::remove_file(new.path().join(file)).unwrap();
        assert!(compare_exports_map_diff(old.path(), new.path())
            .iter()
            .any(
                |finding| finding.rule == "subpath-removed" && finding.severity == Severity::Major
            ));
    }
}

#[test]
fn repeated_target_slots_must_match_the_same_subpath() {
    let package = tempfile::tempdir().unwrap();
    std::fs::write(
        package.path().join("package.json"),
        r#"{"exports":{"./*":"./*/index-*.js"}}"#,
    )
    .unwrap();
    std::fs::create_dir(package.path().join("feature")).unwrap();
    std::fs::write(
        package.path().join("feature/index-other.js"),
        "exports.private=1;",
    )
    .unwrap();

    assert!(resolve_entry_points(package.path()).unwrap().is_empty());
}
