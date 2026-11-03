//! Tsx compatibility checks.

use pledge::ecosystems::node::discovery::discover_exports;

fn write_pkg(dir: &std::path::Path, pkg: serde_json::Value) {
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();
}

#[test]
fn tsx_jsx_body_discovers_exports() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("comp.tsx"),
        r#"
export const Comp = () => <div>hi</div>;
export interface Props { children: string }
export function other(): number { return 1; }
"#,
    )
    .unwrap();
    write_pkg(
        dir.path(),
        serde_json::json!({
            "name": "tsx-pkg",
            "version": "1.0.0",
            "main": "./comp.tsx"
        }),
    );

    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    let names: std::collections::HashSet<String> = exports.keys().map(|k| k.name.clone()).collect();

    assert!(
        names.contains("Comp"),
        "Comp must be discovered; got {:?}",
        names
    );
    assert!(
        names.contains("Props"),
        "Props must be discovered; got {:?}",
        names
    );
    assert!(
        names.contains("other"),
        "other must be discovered; got {:?}",
        names
    );

    let parse_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("parse") || w.message.contains("ERROR"))
        .collect();
    assert!(
        parse_warnings.is_empty(),
        "TSX grammar should parse JSX body cleanly; got: {:?}",
        parse_warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}
