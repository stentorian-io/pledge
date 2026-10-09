//! Config comments compatibility checks.

use interlint::discover::entry_points::resolve_entry_points;

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
