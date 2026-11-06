//! Config comments compatibility checks.

use pledge::ecosystems::node::discovery::discover_exports;
use pledge::ecosystems::node::discovery::entry_points::resolve_entry_points;
use pledge::languages::javascript::compare;

#[test]
fn tsconfig_jsonc_block_comment_accepted() {
    // ROB-04: strip_jsonc is exercised by tsconfig parsing.
    // Test indirectly by resolving a package that has a tsconfig with block comments.
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let pkg = dir.path();
    std::fs::write(
        pkg.join("package.json"),
        r#"{"name":"t","version":"1.0.0","main":"./index.ts"}"#,
    )
    .unwrap();
    std::fs::write(pkg.join("index.ts"), b"export function foo() {}").unwrap();
    std::fs::write(
        pkg.join("tsconfig.json"),
        r#"{
            "compilerOptions": {
                /* block comment */
                "paths": { "@/*": ["./src/*"] },
            }
        }"#,
    )
    .unwrap();
    // resolve_entry_points reads tsconfig for path aliases — should not panic
    let result = resolve_entry_points(pkg);
    assert!(
        result.is_ok(),
        "ROB-04: tsconfig with block comments should not fail. Got: {:?}",
        result
    );
}

#[test]
fn trailing_comma_syntax_preserves_quoted_alias_paths() {
    let old = tempfile::tempdir().unwrap();
    let new = tempfile::tempdir().unwrap();
    let mut surfaces = Vec::new();
    for (directory, parameter) in [(&old, "string"), (&new, "number")] {
        let root = directory.path();
        std::fs::write(root.join("package.json"), r#"{"exports":"./index.d.ts"}"#).unwrap();
        std::fs::write(root.join("index.d.ts"), "export {parse} from '@alias';").unwrap();
        std::fs::write(
            root.join("tsconfig.json"),
            r#"{"compilerOptions":{"paths":{"@alias":["./leaf,].d.ts",],},},}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("leaf,].d.ts"),
            format!("export declare function parse(input: {parameter}): string;"),
        )
        .unwrap();
        std::fs::write(
            root.join("leaf].d.ts"),
            "export declare function parse(input: number): string;",
        )
        .unwrap();

        let (exports, warnings) = discover_exports(root).unwrap();

        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(exports
            .values()
            .any(|symbol| { symbol.name == "parse" && symbol.file.ends_with("leaf,].d.ts") }));
        surfaces.push(exports);
    }

    let findings = compare::compare_exports(&surfaces[0], &surfaces[1], new.path(), true);

    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "param-type-narrowed"),
        "{findings:?}"
    );
}
