use interlint::compare::{compare_exports, compare_structural_diff, has_breaking_changes};
use interlint::discover::discover_exports;
use interlint::types::SymbolSpace;

fn package(index: &str, implementation: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"exports":"./index.ts"}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("index.ts"), index).unwrap();
    std::fs::write(dir.path().join("impl.ts"), implementation).unwrap();
    dir
}

#[test]
fn named_barrels_preserve_declaration_names_and_positions() {
    let dir = package(
        "export { internal as api } from './impl.js';",
        "\n\n\nexport function internal(x: string): string { return x; }",
    );
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let symbol = exports.values().find(|s| s.name == "api").unwrap();
    assert_eq!(symbol.local_name.as_deref(), Some("internal"));
    assert_eq!(symbol.line, 4, "{exports:?}");
    assert_eq!(symbol.col, 17);
    assert_eq!(symbol.file.to_str(), Some("impl.ts"));
}

#[test]
fn local_class_aliases_keep_their_structural_surface() {
    let old = package(
        "class Internal { method(): void {} } export { Internal as Public };",
        "",
    );
    let new = package("class Internal {} export { Internal as Public };", "");
    let (old_exports, _) = discover_exports(old.path()).unwrap();
    let (new_exports, _) = discover_exports(new.path()).unwrap();
    let findings = compare_structural_diff(old.path(), new.path(), &old_exports, &new_exports);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "class-method-removed" && f.message.contains("Public")),
        "{findings:?}"
    );
}

#[test]
fn star_exports_exclude_default_and_preserve_type_only_boundaries() {
    let old = package(
        "export * from './impl.js';",
        "export default 1; export class Public {}",
    );
    let new = package("export type * from './impl.js';", "export class Public {}");
    let (old_exports, _) = discover_exports(old.path()).unwrap();
    let (new_exports, _) = discover_exports(new.path()).unwrap();
    assert!(!old_exports.keys().any(|k| k.name == "default"));
    assert!(new_exports
        .iter()
        .any(|(k, s)| k.name == "Public" && k.space == SymbolSpace::Type && s.is_type_only));
    let findings = compare_exports(&old_exports, &new_exports, new.path(), false);
    assert!(has_breaking_changes(&findings), "{findings:?}");
    assert!(!findings.iter().any(|f| f.message.contains("default")));
}

#[test]
fn explicit_exports_win_over_stars_in_either_source_order() {
    for index in [
        "export * from './impl.js'; export const api = 1;",
        "export const api = 1; export * from './impl.js';",
    ] {
        let dir = package(index, "export const api = 2;");
        let (exports, warnings) = discover_exports(dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(
            exports
                .values()
                .find(|s| s.name == "api")
                .unwrap()
                .file
                .to_str(),
            Some("index.ts")
        );
    }
}

#[test]
fn ambiguous_star_names_are_not_public_exports() {
    let dir = package(
        "export * from './impl.js'; export * from './other.js';",
        "export const api = 1;",
    );
    std::fs::write(
        dir.path().join("other.ts"),
        "export const api = 2; export const other = 3;",
    )
    .unwrap();
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(!exports.keys().any(|k| k.name == "api"));
    assert!(exports.keys().any(|k| k.name == "other"));
}

#[test]
fn multiple_barrels_to_the_same_declaration_are_unambiguous() {
    let dir = package(
        "export * from './impl.js'; export * from './other.js';",
        "export const api = 1;",
    );
    std::fs::write(dir.path().join("other.ts"), "export * from './impl.js';").unwrap();
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(exports.keys().filter(|k| k.name == "api").count(), 1);
}

#[test]
fn namespace_stars_expose_members_under_the_namespace() {
    let dir = package(
        "export * as ns from './impl.js';",
        "export default 1; export const api = 2;",
    );
    let (exports, warnings) = discover_exports(dir.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(exports.keys().any(|k| k.name == "ns.api"));
    assert!(exports.keys().any(|k| k.name == "ns.default"));
    assert!(!exports
        .keys()
        .any(|k| k.name == "api" || k.name == "default"));
}

#[test]
fn shared_diamond_graphs_finish_without_losing_either_public_entry() {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        for (directory, leaf) in [(&old, "export const api = 1;"), (&new, "export {};")] {
            std::fs::write(
                directory.path().join("package.json"),
                r#"{"type":"module","exports":{"./a":"./a0.mjs","./b":"./b0.mjs"}}"#,
            )
            .unwrap();
            for depth in 0..18 {
                let source = format!(
                    "export * from './a{}.mjs'; export * from './b{}.mjs';",
                    depth + 1,
                    depth + 1
                );
                for branch in ["a", "b"] {
                    std::fs::write(
                        directory.path().join(format!("{branch}{depth}.mjs")),
                        &source,
                    )
                    .unwrap();
                }
            }
            for branch in ["a", "b"] {
                std::fs::write(directory.path().join(format!("{branch}18.mjs")), leaf).unwrap();
            }
        }
        // Both paths must reach the same binding, so the two leaves re-export it.
        std::fs::write(old.path().join("b18.mjs"), "export * from './a18.mjs';").unwrap();
        let (before, warnings) = interlint::discover::discover_exports(old.path()).unwrap();
        assert!(warnings.is_empty());
        let (after, warnings) = interlint::discover::discover_exports(new.path()).unwrap();
        assert!(warnings.is_empty());
        let findings = interlint::compare::compare_exports(&before, &after, new.path(), false);
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.rule == "export-removed")
                .count(),
            2
        );
        assert!(interlint::compare::compare_exports_map_diff(old.path(), new.path()).is_empty());
        send.send(()).unwrap();
    });
    receive
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("shared graph traversal exceeded its time bound");
}
